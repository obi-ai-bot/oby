//! Signing an account out, the account staying (docs/contracts/accounts.md, ACCOUNTS-23 to 25):
//! `POST /v1/accounts/{id}/sign-out` runs the tool's own logout in the account's own folder, as its
//! sign-in runs ([`command`]'s environment, no box), or for a Continue with ChatGPT login ends obid's
//! own ([`crate::chatgpt::ChatGpt::sign_out`]). The row, its name, model and settings stay; who it
//! was and its usage go, since the next sign-in may be someone else.
//!
//! It writes nothing of the login itself: the tool removes what it keeps. A tool's own login (no
//! folder of obid's) is the person's, signed out with the tool itself, as it is signed in.

use super::*;

/// How long a tool's logout may take.
const LOGOUT_WITHIN: Duration = Duration::from_secs(15);

/// The tool's own logout for the account in `dir`, run as its sign-in is ([`tool_command`],
/// [`account_env`]), nothing read from its input.
pub(super) fn logout_command(
    tool: Tool,
    bin: &Path,
    dir: &Path,
    scratch: &Path,
) -> Result<tokio::process::Command, String> {
    let mut command = tool_command(bin, scratch);
    command
        .stdin(std::process::Stdio::null())
        .envs(account_env(tool, Some(dir), scratch)?);
    match tool {
        Tool::Claude => command.args(["auth", "logout"]),
        Tool::Codex => command.args(["logout", "-c", CODEX_FILE_STORE]),
        Tool::Cursor => command.arg("logout"),
    };
    Ok(command)
}

/// Runs it: what it printed is never kept or logged, only whether it ended well.
async fn log_out(tool: Tool, bin: &Path, dir: &Path, scratch: &Path) -> Result<(), String> {
    let mut command = logout_command(tool, bin, dir, scratch)?;
    let ran = tokio::time::timeout(LOGOUT_WITHIN, command.output())
        .await
        .map_err(|_| format!("{}'s sign-out didn't end within 15 seconds.", tool.name()))?
        .map_err(|e| format!("obid couldn't start {}'s sign-out: {e}", tool.name()))?;
    if ran.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} didn't sign out (it ended with {}).",
            tool.name(),
            ran.status
                .code()
                .map_or_else(|| "a signal".to_owned(), |c| format!("status {c}"))
        ))
    }
}

/// `POST /v1/accounts/{id}/sign-out`: `200` with the Account, now signed out. `404` for no such
/// account, `409` while a job runs on it (`busy`), for the tool's own login (`ownLogin`) or a folder
/// obid won't run a login through (`folderUnsafe`), `503` with no binary to run (`notInstalled`),
/// `502` when the tool's logout failed (`didNotSignOut`), the account as it was.
pub async fn sign_out(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
) -> Result<Response, ApiError> {
    // One change of the account at a time: a sign-in under way ends first.
    let (_held, _) = sign_in_ended(&shared, &id).await?;
    let agents = shared.config.agents.get();
    let (tool, account) = find(&agents, &id).ok_or_else(|| no_account(&id))?;
    let conflict = |reason: &'static str, message: String| {
        ApiError::new(StatusCode::CONFLICT, message).because(reason)
    };
    let chatgpt = tool == Tool::Codex && crate::chatgpt::marker_of(&account).is_some();
    let dir = match (&account.config_dir, chatgpt) {
        (_, true) => None,
        (Some(dir), false) => Some(dir.clone()),
        (None, false) => {
            return Err(conflict(
                "ownLogin",
                format!(
                    "{} is {}'s own login, the person's: sign out with {} itself on this machine.",
                    account.name,
                    tool.name(),
                    tool.name()
                ),
            ));
        }
    };
    let bin = match &dir {
        None => None,
        Some(dir) => {
            login_paths_safe(tool, dir).map_err(|why| {
                conflict(
                    "folderUnsafe",
                    format!(
                        "{}'s folder has {why}: obid never runs a login through it.",
                        account.name
                    ),
                )
            })?;
            Some(
                agent_of(&agents, tool)
                    .as_ref()
                    .and_then(|agent| crate::process::resolve_bin(&agent.bin))
                    .filter(|bin| bin.is_absolute())
                    .ok_or_else(|| {
                        ApiError::new(
                            StatusCode::SERVICE_UNAVAILABLE,
                            format!("obid can't find {}'s binary by an absolute path, which is how it runs a sign-out.", tool.name()),
                        )
                        .because("notInstalled")
                    })?,
            )
        }
    };
    let _changing = shared
        .jobs
        .begin_change(&id)
        .ok_or_else(|| busy(&account))?;
    // A conversation's Claude Code waiting on the login ends with it.
    shared.harness.end_warm_on(&id);
    match (&dir, &bin) {
        (Some(dir), Some(bin)) => {
            let scratch = scratch_folder(&shared)
                .map_err(|why| ApiError::new(StatusCode::BAD_GATEWAY, why))?;
            let ran = log_out(tool, bin, dir, &scratch).await;
            let _ = std::fs::remove_dir_all(&scratch);
            ran.map_err(|why| {
                ApiError::new(StatusCode::BAD_GATEWAY, why).because("didNotSignOut")
            })?;
        }
        _ => {
            if !shared.chatgpt.sign_out(&id).await {
                tracing::warn!("account {id}: ChatGPT didn't confirm ending its session");
            }
        }
    }
    // Who it was and what it used go with the login; its row, name and settings stay.
    if let Err(e) = shared.config.agents.change(|_, saved| {
        saved.identities.remove(&id);
        Ok(())
    }) {
        tracing::warn!(
            "account {id}: who it was stays in accounts.json: {}",
            e.message
        );
    }
    if let Err(e) = shared.jobs.usage.forget(&id) {
        tracing::warn!("account {id}: its usage stayed in usage.json: {e}");
    }
    tracing::info!("account {id}: signed out");
    send_accounts(&shared).await;
    let now = json_now(&shared, tool, account).await;
    Ok(answer(StatusCode::OK, json!(now)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(tool: Tool) -> Vec<String> {
        let command = logout_command(
            tool,
            Path::new("/bin/echo"),
            Path::new("/tmp/acct"),
            Path::new("/tmp"),
        )
        .unwrap();
        command
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn accounts_23_each_tool_signs_out_with_its_own_logout() {
        assert_eq!(argv(Tool::Claude), ["auth", "logout"]);
        assert_eq!(argv(Tool::Codex), ["logout", "-c", CODEX_FILE_STORE]);
        assert_eq!(argv(Tool::Cursor), ["logout"]);
    }

    #[test]
    fn accounts_23_the_logout_runs_in_the_accounts_own_folder() {
        let command = logout_command(
            Tool::Codex,
            Path::new("/bin/echo"),
            Path::new("/tmp/acct"),
            Path::new("/tmp"),
        )
        .unwrap();
        let env: Vec<(String, String)> = command
            .as_std()
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().into_owned(),
                    v?.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        assert!(env.contains(&("CODEX_HOME".into(), "/tmp/acct".into())));
        assert!(
            !env.iter()
                .any(|(k, _)| k.starts_with("ANTHROPIC_") || k == "GH_TOKEN")
        );
    }
}
