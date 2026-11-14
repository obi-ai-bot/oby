//! Each signed-in account's model list, read from its own CLI (CONVERSATION-AGENT-22).
//!
//! Codex: `codex debug models` (visibility `"list"`, display names) or app-server `model/list`
//! (not hidden). Cursor: `cursor-agent models` (`id - name` lines). Claude Code stays on the fixed
//! list unless its CLI lists them. The last good list is kept; a failed read leaves it. Refreshed
//! with the Usage panel's account reads ([`crate::jobs::Jobs::want_usage`]), never on the turn or
//! job path, and never blocking those.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::agent::Tool;
use crate::config::Account;
use crate::settings;

/// How long one account's model-list read may take: same order as a usage read.
const READ_WITHIN: Duration = Duration::from_secs(15);

/// One model the account's menu and `PATCH` may choose: `id` is what runs, `name` what the person sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offered {
    pub id: String,
    pub name: String,
}

/// Last good list per account id. Empty means fall back to [`settings::models`].
#[derive(Default)]
pub struct ModelLists {
    lists: Mutex<HashMap<String, Vec<Offered>>>,
}

impl ModelLists {
    /// The account's last good list, or today's fixed list when none has been read yet.
    pub fn for_account(&self, tool: Tool, account: &str) -> Vec<Offered> {
        self.lists
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(account)
            .cloned()
            .unwrap_or_else(|| fallback(tool))
    }

    /// Whether this account's list is known (AGENTS-6): Claude's is the fixed list (its CLI lists
    /// none); Codex's and Cursor's only once a live read was kept since obid started.
    pub fn read(&self, tool: Tool, account: &str) -> bool {
        tool == Tool::Claude
            || self
                .lists
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(account)
    }

    /// What a saved account model is checked against when a job takes it (AGENTS-6): the
    /// account's list once [`Self::read`], else none, so the saved choice stands (one really gone
    /// fails with the tool's own error).
    pub fn to_check(&self, tool: Tool, account: &str) -> Vec<Offered> {
        if self.read(tool, account) {
            self.for_account(tool, account)
        } else {
            Vec::new()
        }
    }

    /// Keep a non-empty read; true when the kept list changed (so apps should hear).
    pub fn keep(&self, account: &str, list: Vec<Offered>) -> bool {
        if list.is_empty() {
            return false;
        }
        let mut lists = self.lists.lock().unwrap_or_else(|e| e.into_inner());
        match lists.get(account) {
            Some(was) if was == &list => false,
            _ => {
                lists.insert(account.to_owned(), list);
                true
            }
        }
    }

    /// Whether this account's list differs from the tool's options list (for `accounts[].models`).
    pub fn when_differs(
        &self,
        tool: Tool,
        account: &str,
        tool_options: &[Offered],
    ) -> Option<Vec<Offered>> {
        let own = self.for_account(tool, account);
        (own != tool_options).then_some(own)
    }

    /// Forget an account that was removed.
    pub fn forget(&self, account: &str) {
        self.lists
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(account);
    }
}

/// Today's fixed list, as Settings › Agents offered before each account's own was read.
pub fn fallback(tool: Tool) -> Vec<Offered> {
    settings::models(tool)
        .iter()
        .map(|(id, name)| Offered {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
        })
        .collect()
}

/// The tool-level choices `GET /v1/settings/models/options` answers: the first signed-in account's
/// last good list for that tool, else the fixed fallback. Claude stays on the fixed list unless a
/// read filled one.
pub fn tool_options(lists: &ModelLists, tool: Tool, accounts: &[(String, bool)]) -> Vec<Offered> {
    for (id, signed_in) in accounts {
        if *signed_in {
            let own = lists.for_account(tool, id);
            // A Claude account only replaces the fixed list once its CLI listed models.
            if tool != Tool::Claude || own != fallback(tool) {
                return own;
            }
        }
    }
    fallback(tool)
}

/// Read one account's list from its CLI. None when the tool isn't there, the read fails, or the
/// answer is empty. Never asks a model.
pub async fn read(
    tool: Tool,
    bin: &Path,
    account: &Account,
    scratch: &Path,
) -> Option<Vec<Offered>> {
    match tool {
        Tool::Claude => read_claude(bin, account.config_dir.as_deref(), scratch).await,
        Tool::Codex => read_codex(bin, account.config_dir.as_deref(), scratch).await,
        Tool::Cursor => read_cursor(bin, account.config_dir.as_deref(), scratch).await,
    }
}

/// Claude Code has no models subcommand (its Commands are agents, auth, doctor, mcp, …): a bare
/// `claude models` is a prompt and would call a model. No cache file in its config folder lists
/// them either. Keep today's fixed list; never probe the CLI for this.
async fn read_claude(_bin: &Path, _dir: Option<&Path>, _scratch: &Path) -> Option<Vec<Offered>> {
    None
}

/// Prefer `codex debug models` (visibility `list`); else app-server `model/list` (not hidden).
async fn read_codex(bin: &Path, dir: Option<&Path>, scratch: &Path) -> Option<Vec<Offered>> {
    if let Some(listed) = read_codex_debug(bin, dir, scratch)
        .await
        .filter(|l| !l.is_empty())
    {
        return Some(listed);
    }
    read_codex_app_server(bin, dir, scratch)
        .await
        .filter(|l| !l.is_empty())
}

async fn read_codex_debug(bin: &Path, dir: Option<&Path>, scratch: &Path) -> Option<Vec<Offered>> {
    let output = run_cli(bin, &["debug", "models"], Tool::Codex, dir, scratch).await?;
    parse_codex_debug(&String::from_utf8_lossy(&output))
}

async fn read_codex_app_server(
    bin: &Path,
    dir: Option<&Path>,
    scratch: &Path,
) -> Option<Vec<Offered>> {
    let params = json!({"limit": 100, "includeHidden": false});
    let said = crate::accounts::ask_codex(bin, dir, scratch, "model/list", params).await?;
    parse_codex_model_list(&said)
}

async fn read_cursor(bin: &Path, dir: Option<&Path>, scratch: &Path) -> Option<Vec<Offered>> {
    let output = run_cli(bin, &["models"], Tool::Cursor, dir, scratch).await?;
    let listed = parse_cursor_lines(&String::from_utf8_lossy(&output));
    (!listed.is_empty()).then_some(listed)
}

async fn run_cli(
    bin: &Path,
    args: &[&str],
    tool: Tool,
    dir: Option<&Path>,
    scratch: &Path,
) -> Option<Vec<u8>> {
    let mut command = crate::accounts::tool_command(bin, scratch);
    command
        .args(args)
        .envs(crate::accounts::account_env(tool, dir, scratch).ok()?)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = command.spawn().ok()?;
    let group = child.id();
    match tokio::time::timeout(READ_WITHIN, child.wait_with_output()).await {
        Ok(Ok(output)) if output.status.success() => Some(output.stdout),
        Ok(_) => None,
        Err(_) => {
            if let Some(group) = group {
                crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
            }
            None
        }
    }
}

/// `codex debug models`: `{ "models": [ { "slug", "display_name", "visibility": "list" }, … ] }`.
pub fn parse_codex_debug(text: &str) -> Option<Vec<Offered>> {
    let said: Value = serde_json::from_str(text.trim()).ok()?;
    let models = said
        .get("models")
        .or_else(|| said.as_array().map(|_| &said))?;
    let listed: Vec<Offered> = models
        .as_array()?
        .iter()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| {
            let id = m["slug"].as_str().or_else(|| m["id"].as_str())?;
            if !settings::valid_model(id) {
                return None;
            }
            let name = m["display_name"]
                .as_str()
                .or_else(|| m["displayName"].as_str())
                .unwrap_or(id);
            Some(Offered {
                id: id.to_owned(),
                name: name.to_owned(),
            })
        })
        .collect();
    (!listed.is_empty()).then_some(listed)
}

/// App-server `model/list`: `{ "data": [ { "id"|"model", "displayName", "hidden": false }, … ] }`.
pub fn parse_codex_model_list(said: &Value) -> Option<Vec<Offered>> {
    let listed: Vec<Offered> = said["data"]
        .as_array()?
        .iter()
        .filter(|m| m["hidden"] != true)
        .filter_map(|m| {
            let id = m["model"].as_str().or_else(|| m["id"].as_str())?;
            if !settings::valid_model(id) {
                return None;
            }
            let name = m["displayName"]
                .as_str()
                .or_else(|| m["display_name"].as_str())
                .unwrap_or(id);
            Some(Offered {
                id: id.to_owned(),
                name: name.to_owned(),
            })
        })
        .collect();
    (!listed.is_empty()).then_some(listed)
}

/// `cursor-agent models`: lines `id - name` (and a header to skip).
pub fn parse_cursor_lines(text: &str) -> Vec<Offered> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (id, name) = line.split_once(" - ")?;
            let id = id.trim();
            // Cursor pads some names with zero-width spaces and doubled spaces ("Grok 4.7  Low Fast")
            let name = name
                .split(|c: char| {
                    c.is_whitespace() || matches!(c, '\u{200b}'..='\u{200d}' | '\u{feff}')
                })
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if id.is_empty() || name.is_empty() || !settings::valid_model(id) {
                return None;
            }
            Some(Offered {
                id: id.to_owned(),
                name,
            })
        })
        .collect()
}

/// One of Cursor's models with its listed ids by effort and Fast (AGENTS-11): `cursor-agent
/// models` lists one id per combination (`grok-4.7-low`, `grok-4.7-low-fast`, …), so Settings
/// shows the model once and its effort and Fast apart, and still sends a listed id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// What the group is known by: the ids without their effort and Fast (not always an id).
    pub base: String,
    pub name: String,
    /// The id Cursor names with no effort ("Claude Opus 5.5 1M"): where a pick of the model starts.
    pub default: String,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub id: String,
    /// None for a bare id (`gpt-5.3-codex`): the model's own default effort.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    pub fast: bool,
}

/// Efforts in the order variants are listed in (a bare id's default goes just after `medium`);
/// `extra-high` first so it isn't read as `high`, and listed as `xhigh`.
const EFFORTS: &str = "extra-high none minimal low medium high xhigh max ultra";

/// `id` as (base, effort, fast): `-fast` last, an effort before it (or before `-thinking`, as
/// `claude-4.6-opus-high-thinking`), `thinking` staying in the base. `bases`: the bases other ids
/// gave, so an id that is itself one (`gpt-5.1-codex-max` beside `gpt-5.1-codex-max-high`) is a
/// bare model, not `gpt-5.1-codex` at `max`.
fn split_id(id: &str, bases: &std::collections::HashSet<String>) -> (String, Option<String>, bool) {
    let (rest, fast) = id.strip_suffix("-fast").map_or((id, false), |r| (r, true));
    let (rest, thinking) = rest
        .strip_suffix("-thinking")
        .map_or((rest, ""), |r| (r, "-thinking"));
    let effort = EFFORTS
        .split(' ')
        .find_map(|e| Some((rest.strip_suffix(e)?.strip_suffix('-')?, e)));
    match effort {
        Some((base, e)) if !bases.contains(&format!("{rest}{thinking}")) => (
            format!("{base}{thinking}"),
            Some(e.replace("extra-", "x")),
            fast,
        ),
        _ => (format!("{rest}{thinking}"), None, fast),
    }
}

/// Cursor's flat list as models (AGENTS-11), in the list's order: every listed id is in exactly
/// one group, so every choice stays reachable and is one Cursor said exists.
pub fn group_cursor(list: &[Offered]) -> Vec<Group> {
    let none = std::collections::HashSet::new();
    let bases: std::collections::HashSet<String> = list
        .iter()
        .map(|m| split_id(&m.id, &none))
        .filter(|(_, effort, _)| effort.is_some())
        .map(|(base, ..)| base)
        .collect();
    let mut groups: Vec<(Group, Vec<&str>)> = Vec::new();
    for m in list {
        let (mut base, effort, fast) = split_id(&m.id, &bases);
        let taken = |v: &Variant| v.effort == effort && v.fast == fast;
        // ponytail: two ids that read as the same combination: the second stands alone
        if groups
            .iter()
            .any(|(g, _)| g.base == base && g.variants.iter().any(taken))
        {
            base = m.id.clone();
        }
        let variant = Variant {
            id: m.id.clone(),
            effort,
            fast,
        };
        match groups.iter_mut().find(|(g, _)| g.base == base) {
            Some((g, names)) => {
                g.variants.push(variant);
                names.push(&m.name);
            }
            None => groups.push((
                Group {
                    base,
                    name: String::new(),
                    default: String::new(),
                    variants: vec![variant],
                },
                vec![&m.name],
            )),
        }
    }
    groups
        .into_iter()
        .map(|(mut g, names)| {
            // the model's name: the words every one of its names has ("Grok 4.7")
            g.name = names[0]
                .split(' ')
                .filter(|w| names.iter().all(|n| n.split(' ').any(|x| x == *w)))
                .collect::<Vec<_>>()
                .join(" ");
            // the default: the plain id Cursor names with the fewest words, `medium` on a tie
            let words = |v: &Variant| {
                let at = g.variants.iter().position(|x| x == v).unwrap_or(0);
                names[at].split(' ').count()
            };
            g.default = g
                .variants
                .iter()
                .filter(|v| !v.fast)
                .min_by_key(|v| (words(v), v.effort.as_deref() != Some("medium")))
                .unwrap_or(&g.variants[0])
                .id
                .clone();
            let at = |e: &str| EFFORTS.split(' ').position(|x| x == e).unwrap_or(0) * 2;
            g.variants
                .sort_by_key(|v| (v.effort.as_deref().map_or(at("medium") + 1, at), v.fast));
            g
        })
        .collect()
}

/// Scratch folder for one read, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_agent_22_codex_debug_keeps_list_visibility_and_display_names() {
        let text = r#"{
          "models": [
            {"slug": "gpt-6.1-sol", "display_name": "gpt-6.1-sol", "visibility": "list"},
            {"slug": "gpt-6-astra", "display_name": "GPT-6-Astra", "visibility": "list"},
            {"slug": "gpt-hidden", "display_name": "Hidden", "visibility": "hide"},
            {"slug": "bad slug", "display_name": "Bad", "visibility": "list"}
          ]
        }"#;
        assert_eq!(
            parse_codex_debug(text).unwrap(),
            vec![
                Offered {
                    id: "gpt-6.1-sol".into(),
                    name: "gpt-6.1-sol".into()
                },
                Offered {
                    id: "gpt-6-astra".into(),
                    name: "GPT-6-Astra".into()
                },
            ]
        );
    }

    #[test]
    fn conversation_agent_22_codex_model_list_skips_hidden() {
        let said = json!({
            "data": [
                {"id": "gpt-6.1-sol", "model": "gpt-6.1-sol", "displayName": "Sol", "hidden": false},
                {"id": "secret", "model": "secret", "displayName": "Secret", "hidden": true}
            ]
        });
        assert_eq!(
            parse_codex_model_list(&said).unwrap(),
            vec![Offered {
                id: "gpt-6.1-sol".into(),
                name: "Sol".into()
            }]
        );
    }

    #[test]
    fn conversation_agent_22_cursor_lines_are_id_dash_name() {
        let text = "Available models\nauto - Auto (current, default)\ngpt-5.6-sol-high - GPT-5.6 Sol 1M High\nbad line\n";
        assert_eq!(
            parse_cursor_lines(text),
            vec![
                Offered {
                    id: "auto".into(),
                    name: "Auto (current, default)".into()
                },
                Offered {
                    id: "gpt-5.6-sol-high".into(),
                    name: "GPT-5.6 Sol 1M High".into()
                },
            ]
        );
    }

    fn offered(lines: &[(&str, &str)]) -> Vec<Offered> {
        lines
            .iter()
            .map(|(id, name)| Offered {
                id: (*id).into(),
                name: (*name).into(),
            })
            .collect()
    }
    fn variant(id: &str, effort: Option<&str>, fast: bool) -> Variant {
        Variant {
            id: id.into(),
            effort: effort.map(Into::into),
            fast,
        }
    }

    #[test]
    fn agents_11_cursor_ids_group_by_model_with_their_efforts_and_fast() {
        let groups = group_cursor(&offered(&[
            ("auto", "Auto (default)"),
            ("gpt-5.3-codex-low", "Codex 5.3 Low"),
            ("gpt-5.3-codex", "Codex 5.3"),
            ("gpt-5.3-codex-fast", "Codex 5.3 Fast"),
            ("gpt-5.3-codex-high", "Codex 5.3 High"),
            ("claude-opus-5-thinking-high", "Claude Opus 5 1M Thinking"),
            (
                "claude-opus-5-thinking-low-fast",
                "Claude Opus 5 1M Low Thinking Fast",
            ),
            ("claude-opus-5-high", "Claude Opus 5 1M"),
            ("gpt-5.5-extra-high", "GPT-5.5 1M Extra High"),
            ("gpt-5.5-medium", "GPT-5.5 1M"),
            (
                "claude-4.6-opus-high-thinking",
                "Claude Opus 4.6 1M Thinking",
            ),
            (
                "claude-4.6-opus-max-thinking",
                "Claude Opus 4.6 1M Max Thinking",
            ),
            ("kimi-k2.7-code", "Kimi K2.7 Code"),
            ("gpt-5.1-codex-max", "GPT-5.1 Codex Max"),
            ("gpt-5.1-codex-max-high", "GPT-5.1 Codex Max High"),
        ]));
        let at = |base: &str| groups.iter().find(|g| g.base == base).unwrap();
        // auto, and a model with no variants, stand alone
        assert_eq!(at("auto").variants, vec![variant("auto", None, false)]);
        assert_eq!(at("auto").name, "Auto (default)");
        assert_eq!(at("kimi-k2.7-code").variants.len(), 1);
        // the bare id is the model's default effort, listed after medium; Fast beside it
        let codex = at("gpt-5.3-codex");
        assert_eq!(codex.name, "Codex 5.3");
        assert_eq!(codex.default, "gpt-5.3-codex");
        assert_eq!(
            codex.variants,
            vec![
                variant("gpt-5.3-codex-low", Some("low"), false),
                variant("gpt-5.3-codex", None, false),
                variant("gpt-5.3-codex-fast", None, true),
                variant("gpt-5.3-codex-high", Some("high"), false),
            ]
        );
        // thinking is part of the model, wherever Cursor puts it in the id
        let thinking = at("claude-opus-5-thinking");
        assert_eq!(thinking.name, "Claude Opus 5 1M Thinking");
        assert_eq!(thinking.default, "claude-opus-5-thinking-high");
        assert_eq!(at("claude-opus-5").variants.len(), 1);
        assert_eq!(at("claude-4.6-opus-thinking").variants.len(), 2);
        // extra-high is xhigh, not "gpt-5.5-extra" at high
        assert_eq!(
            at("gpt-5.5").variants[1],
            variant("gpt-5.5-extra-high", Some("xhigh"), false)
        );
        assert_eq!(at("gpt-5.5").default, "gpt-5.5-medium");
        // a model named like an effort stays itself
        assert_eq!(
            at("gpt-5.1-codex-max").variants,
            vec![
                variant("gpt-5.1-codex-max", None, false),
                variant("gpt-5.1-codex-max-high", Some("high"), false),
            ]
        );
        assert!(groups.iter().all(|g| g.base != "gpt-5.1-codex"));
    }

    #[test]
    fn agents_11_every_id_cursor_lists_is_in_one_group() {
        let listed = parse_cursor_lines(include_str!("../tests/fixtures/cursor-agent-models.txt"));
        assert_eq!(listed.len(), 276);
        let groups = group_cursor(&listed);
        let mut grouped: Vec<&str> = groups
            .iter()
            .flat_map(|g| g.variants.iter().map(|v| v.id.as_str()))
            .collect();
        grouped.sort_unstable();
        let mut ids: Vec<&str> = listed.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(grouped, ids);
        assert!(groups.len() < 80, "{} models", groups.len());
        let grok = groups.iter().find(|g| g.base == "grok-4.7").unwrap();
        assert_eq!(
            grok.name, "Grok 4.7",
            "zero-width spaces and doubled spaces gone"
        );
        assert_eq!(grok.variants.len(), 8);
        let opus = groups.iter().find(|g| g.base == "claude-opus-5-5").unwrap();
        assert_eq!(opus.name, "Claude Opus 5.5 1M");
        assert_eq!(opus.default, "claude-opus-5-5-medium");
        for g in &groups {
            assert!(g.variants.iter().any(|v| v.id == g.default), "{}", g.base);
            assert!(!g.name.is_empty(), "{}", g.base);
        }
    }

    #[test]
    fn conversation_agent_22_keeps_the_last_good_list_when_a_read_fails() {
        let lists = ModelLists::default();
        let first = vec![Offered {
            id: "gpt-a".into(),
            name: "A".into(),
        }];
        assert!(lists.keep("codex", first.clone()));
        assert!(!lists.keep("codex", first.clone()));
        assert_eq!(lists.for_account(Tool::Codex, "codex"), first);
        assert_eq!(
            lists.for_account(Tool::Codex, "other"),
            fallback(Tool::Codex),
            "no read yet: fixed list"
        );
    }

    #[test]
    fn conversation_agent_22_account_models_only_when_the_list_differs() {
        let lists = ModelLists::default();
        let tool = fallback(Tool::Cursor);
        assert!(lists.when_differs(Tool::Cursor, "cursor", &tool).is_none());
        let long = vec![
            Offered {
                id: "auto".into(),
                name: "Auto".into(),
            },
            Offered {
                id: "gpt-extra".into(),
                name: "Extra".into(),
            },
        ];
        lists.keep("cursor", long.clone());
        assert_eq!(
            lists.when_differs(Tool::Cursor, "cursor", &tool),
            Some(long)
        );
    }
}
