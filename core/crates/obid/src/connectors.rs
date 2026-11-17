//! The owner's own Claude Code's connectors, as Settings › Connectors shows them
//! (docs/contracts/connectors.md, CONNECTORS-1 to 3): the MCP servers and claude.ai connectors an own
//! conversation's session named in its `init` line, by name and status only, kept in
//! `home/connectors.json` (0600), and `GET /v1/connectors`.
//!
//! State: that file, replaced whole each time an own session names its servers. It never holds an
//! address, a command, an argument or an environment (the `init` line has none), and nothing here
//! adds, signs in to or removes a connector: they're the owner's, in Claude Code and on claude.ai.
//! A boxed session's list never reaches it (`brain::harness::one` passes only an own one's).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::Shared;

/// The servers obid itself gives a session, which are Oby's, not the owner's.
const OBIDS: [&str; 2] = ["oby", "tetrix"];

/// One connector, as Claude Code named it: `status` is its own word (`connected`, `needs-auth`,
/// `pending`, `failed`, `disabled`), passed on as an open string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seen {
    seen_at: String,
    servers: Vec<Server>,
}

/// The servers an `init` line names (`mcp_servers`), but obid's own, in its order.
pub fn named(init: &Value) -> Vec<Server> {
    init["mcp_servers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let name = s["name"].as_str()?.trim();
            let status = s["status"].as_str().unwrap_or("").trim();
            (!name.is_empty() && !OBIDS.contains(&name)).then(|| Server {
                name: name.to_owned(),
                status: status.to_owned(),
            })
        })
        .collect()
}

fn file(home: &Path) -> PathBuf {
    home.join("connectors.json")
}

/// An own session named `servers`: they replace what was kept. A failed write is logged, never
/// the turn's.
pub fn saw(home: &Path, servers: Vec<Server>) {
    let seen = Seen {
        seen_at: obi_protocol::time::format(jiff::Timestamp::now().as_second()),
        servers,
    };
    let bytes = serde_json::to_vec_pretty(&seen).unwrap_or_default();
    if let Err(e) = crate::store::write_private(&file(home), &bytes) {
        tracing::warn!("obid couldn't keep the owner's connectors: {e}");
    }
}

/// What was kept, or none before any own session named its servers (or a file that doesn't read).
fn kept(home: &Path) -> Option<Seen> {
    serde_json::from_slice(&std::fs::read(file(home)).ok()?).ok()
}

pub fn router() -> Router<Arc<Shared>> {
    Router::new().route("/v1/connectors", get(list))
}

async fn list(State(shared): State<Arc<Shared>>) -> Json<Value> {
    Json(json!({"claude": kept(&shared.config.home)}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keeps_the_owners_by_name_and_status_but_obids_own() {
        let init = json!({"type": "system", "subtype": "init", "mcp_servers": [
            {"name": "oby", "status": "connected"},
            {"name": "claude.ai Slack", "status": "connected", "url": "https://example.com"},
            {"name": "tetrix", "status": "connected"},
            {"name": "rea", "status": "failed"},
            {"status": "connected"},
        ]});
        assert_eq!(
            named(&init),
            vec![
                Server {
                    name: "claude.ai Slack".into(),
                    status: "connected".into()
                },
                Server {
                    name: "rea".into(),
                    status: "failed".into()
                },
            ]
        );
        assert!(named(&json!({"type": "system"})).is_empty());
    }

    #[test]
    fn saw_replaces_what_was_kept() {
        let home = std::env::temp_dir().join(format!("obid-connectors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(kept(&home), None);
        let one = |name: &str| Server {
            name: name.into(),
            status: "connected".into(),
        };
        saw(&home, vec![one("a"), one("b")]);
        saw(&home, vec![one("c")]);
        let seen = kept(&home).unwrap();
        assert_eq!(seen.servers, vec![one("c")]);
        assert!(seen.seen_at.ends_with('Z'));
        std::fs::remove_dir_all(&home).unwrap();
    }
}
