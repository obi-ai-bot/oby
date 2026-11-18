//! The binary each tool runs as, set from Settings › Agents (docs/contracts/accounts.md,
//! ACCOUNTS-26 to 28): `PUT /v1/settings/agents/{agent}` keeps it in `home/accounts.json` beside
//! the accounts, laid over the configuration's `agents.<tool>.bin` at every start, and `null` takes
//! it back to the configuration's. The configuration file is never rewritten, as for the accounts.
//!
//! It owns the `bins` of [`Saved`] and nothing else. A value is a path from `/` or a command's bare
//! name, found now as obid finds every tool's binary ([`crate::process::resolve_bin`]): a value
//! obid can't find is refused, so a typo never leaves a tool that can't start.

use super::*;

const SHAPE: &str = "{\"bin\": \"/a/path/to/the/binary\" or a command's name, or null}";

/// The longest value, in characters.
const LONGEST_BIN: usize = 1024;

/// Whether `bin` is a value obid keeps: a path from `/`, or a command's bare name, with no control
/// character.
pub(super) fn well_formed(bin: &str) -> bool {
    let named = !bin.is_empty()
        && bin
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    bin.chars().count() <= LONGEST_BIN
        && !bin.chars().any(char::is_control)
        && (named || (bin.starts_with('/') && bin.len() > 1))
}

/// The kept values that still read right, by tool (crate::accounts::checked).
pub(super) fn checked(bins: BTreeMap<String, String>, home: &Path) -> BTreeMap<String, String> {
    bins.into_iter()
        .filter(|(tool, bin)| {
            let fine = Tool::from_id(tool).is_some() && well_formed(bin);
            if !fine {
                tracing::warn!("{}: binary {tool:?} left out", home.join(FILE).display());
            }
            fine
        })
        .collect()
}

/// Lays the kept values over the agents' (crate::accounts::merged): a tool with no accounts has
/// nothing to run, so nothing to lay it on.
pub(super) fn lay(agents: &mut Agents, bins: &BTreeMap<String, String>) {
    for tool in Tool::ALL {
        if let (Some(bin), Some(agent)) = (bins.get(tool.id()), agent_of_mut(agents, tool).as_mut())
        {
            agent.bin = bin.clone();
        }
    }
}

/// `PUT /v1/settings/agents/{agent}`: `{"bin": "<path or name>" | null}`. `200` with the tool's
/// `{agent, bin, binSet, path, version}` as `GET /v1/status` gives them; `404` for a tool obid
/// doesn't run; `422` for a value that isn't a path from `/` or a name (`badBin`) or that obid can't
/// find (`notFound`).
pub async fn set(
    State(shared): State<Arc<Shared>>,
    UrlPath(agent): UrlPath<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let tool = Tool::from_id(&agent).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            format!("obid runs no tool {agent:?}."),
        )
    })?;
    let map = object(&body, &["bin"], SHAPE)?;
    let bin = match map.get("bin") {
        Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.trim().to_owned()),
        _ => return Err(unprocessable(format!("The body is {SHAPE}.")).because("badBin")),
    };
    if let Some(bin) = &bin {
        if !well_formed(bin) {
            return Err(unprocessable(format!(
                "{bin:?} isn't a path from / or a command's name: the body is {SHAPE}."
            ))
            .because("badBin"));
        }
        let looked = bin.clone();
        let found = tokio::task::spawn_blocking(move || crate::process::resolve_bin(&looked))
            .await
            .ok()
            .flatten();
        if found.is_none() {
            return Err(
                unprocessable(format!("obid can't find {bin} to run as {}.", tool.name()))
                    .because("notFound"),
            );
        }
    }
    shared.config.agents.change(|_, saved| {
        match &bin {
            Some(bin) => saved.bins.insert(tool.id().to_owned(), bin.clone()),
            None => saved.bins.remove(tool.id()),
        };
        Ok(())
    })?;
    tracing::info!(
        "{} runs as {}",
        tool.name(),
        bin.as_deref().unwrap_or("the configuration's binary")
    );
    let agents = shared.config.agents.get();
    let now = bin_of(&agents, tool).to_owned();
    let looked = now.clone();
    let read = tokio::task::spawn_blocking(move || crate::cli_versions::read(tool, &looked, true))
        .await
        .ok();
    Ok(answer(
        StatusCode::OK,
        json!({
            "agent": tool.id(),
            "bin": now,
            "binSet": shared.config.agents.bin_set(tool),
            "path": read.as_ref().and_then(|r| Some(r.path.as_ref()?.display().to_string())),
            "version": read.and_then(|r| r.version),
        }),
    ))
}

impl Live {
    /// Whether `tool`'s binary is one the owner set (ACCOUNTS-26), not the configuration's.
    pub fn bin_set(&self, tool: Tool) -> bool {
        self.read().saved.bins.contains_key(tool.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_27_a_path_from_slash_or_a_bare_name_is_kept_nothing_else() {
        for fine in [
            "/opt/homebrew/bin/claude",
            "claude",
            "cursor-agent",
            "codex.v2",
            "/Applications/My Tools/codex",
        ] {
            assert!(well_formed(fine), "{fine}");
        }
        for bad in [
            "",
            "/",
            "bin/claude",
            "./claude",
            "~/bin/claude",
            "claude --x",
            "a\nb",
            "a\u{0}b",
            &"a".repeat(LONGEST_BIN + 1),
        ] {
            assert!(!well_formed(bad), "{bad:?}");
        }
    }

    #[test]
    fn accounts_26_a_kept_binary_is_laid_over_the_configurations() {
        let mut agents: Agents = serde_json::from_value(json!({
            "claude": {"bin": "claude", "accounts": []},
            "codex": {"bin": "codex", "accounts": []}
        }))
        .unwrap();
        let bins = BTreeMap::from([
            ("codex".to_owned(), "/opt/codex".to_owned()),
            ("cursor".to_owned(), "/opt/cursor".to_owned()),
        ]);
        lay(&mut agents, &bins);
        assert_eq!(bin_of(&agents, Tool::Codex), "/opt/codex");
        assert_eq!(bin_of(&agents, Tool::Claude), "claude");
        // a tool with no agent set up has nothing to lay it on
        assert!(agents.cursor.is_none());
    }

    #[test]
    fn accounts_28_a_kept_value_that_doesnt_read_is_left_out() {
        let home = Path::new("/nowhere");
        let bins = BTreeMap::from([
            ("codex".to_owned(), "/opt/codex".to_owned()),
            ("gemini".to_owned(), "gemini".to_owned()),
            ("claude".to_owned(), "../claude".to_owned()),
        ]);
        assert_eq!(
            checked(bins, home),
            BTreeMap::from([("codex".to_owned(), "/opt/codex".to_owned())])
        );
    }
}
