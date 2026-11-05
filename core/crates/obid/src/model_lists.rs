//! Each signed-in account's model list, read from its own CLI (CONVERSATION-AGENT-22).
//!
//! Codex: `codex debug models` (visibility `"list"`, display names) or app-server `model/list`
//! (not hidden). Cursor: `cursor-agent models` (`id - name` lines). Claude Code stays on the fixed
//! list unless its CLI lists them. The last good list is kept; a failed read leaves it. Refreshed
//! with the Usage panel's account reads ([`crate::jobs::Jobs::want_usage`]), never on the turn or
//! job path, and never blocking those.

use std::collections::HashMap;
use std::ffi::OsString;
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
    let mut argv: Vec<OsString> = vec![bin.into()];
    if dir.is_some() {
        argv.extend(["-c".into(), r#"cli_auth_credentials_store="file""#.into()]);
    }
    argv.extend([
        "-c".into(),
        "features.plugins=false".into(),
        "app-server".into(),
    ]);
    let mut env = crate::process::kept(|name| std::env::var_os(name));
    env.push(("PATH".into(), crate::accounts::login_path(bin).into()));
    env.push(("TMPDIR".into(), scratch.into()));
    env.extend(
        crate::accounts::account_env(Tool::Codex, dir, scratch)
            .ok()?
            .into_iter()
            .map(|(k, v)| (k.into(), v)),
    );
    let launch = crate::agent::Launch {
        argv,
        env,
        cwd: scratch.to_path_buf(),
        stderr: scratch.join("stderr.log"),
        own_sandbox: false,
        group: None,
    };
    let params = json!({"limit": 100, "includeHidden": false});
    let said = crate::codex::ask_once(&launch, READ_WITHIN, "model/list", params)
        .await
        .ok()?;
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
    let mut command = tokio::process::Command::new(bin);
    command
        .args(args)
        .env_clear()
        .envs(crate::process::kept(|name| std::env::var_os(name)))
        .env("PATH", crate::accounts::login_path(bin))
        .env("TMPDIR", scratch)
        .env("BROWSER", "true")
        .envs(crate::accounts::account_env(tool, dir, scratch).ok()?)
        .current_dir(scratch)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .process_group(0);
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
            let name = name.trim();
            if id.is_empty() || name.is_empty() || !settings::valid_model(id) {
                return None;
            }
            Some(Offered {
                id: id.to_owned(),
                name: name.to_owned(),
            })
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
