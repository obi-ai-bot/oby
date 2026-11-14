//! The MCP plumbing of obid's servers for agents, written once (docs/contracts/agent-mcp.md): the
//! job's broker (crate::connections), the conversation's tools (crate::brain::tools) and Tetrix's,
//! and the table of every tool they serve with its gates ([`every`]).

use std::sync::LazyLock;

use serde_json::{Value, json};

use crate::{brain, connections, tetrix};

/// Which of obid's servers for agents serves a tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Server {
    Conversation,
    Broker,
    Tetrix,
}

/// What a tool's answer marks where it's read: the conversation, for its tools; a job's are judged
/// by crate::secretspec, which this agrees with (AGENTMCP-7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Marks {
    /// Nothing: obid's own words, or what its handler marks by what it read.
    Nothing,
    With(&'static str),
    /// `<name>'s answer`, as a tool with no entry marks too.
    Answer,
    /// By the job named in the call's `id`: its title, unless it's the owner's own words throughout.
    ByJob,
}

/// One tool an agent can call, with every gate obid holds it to before its handler runs. None has
/// a default: a new tool names each (AGENTMCP-7).
pub(crate) struct Tool {
    pub(crate) name: &'static str,
    pub(crate) server: Server,
    /// Its asks for the owner's OK are capped on a turn that isn't their word (its handler asks).
    pub(crate) confirm_unless_owners_word: bool,
    /// Given only to the owner's own work: never another Oby's.
    pub(crate) owner_only: bool,
    pub(crate) marks_on_result: Marks,
    /// It only reads: another agent given the same words would call it again.
    pub(crate) read_only: bool,
    pub(crate) does: Does,
}

/// What a tool does: its handler, description and input schema.
pub(crate) enum Does {
    Conversation(crate::brain::tools::Handler, &'static str, Value),
    Broker(crate::connections::Handler, &'static str, Value),
    /// Forwarded to Tetrix, whose own listing describes it.
    Forwarded,
}

/// Every tool an agent can call on obid's three servers, with its gates: who may call what.
pub(crate) fn every() -> &'static [Tool] {
    static EVERY: LazyLock<Vec<Tool>> = LazyLock::new(|| {
        let servers = [brain::tools::table(), connections::table(), tetrix::table()];
        servers.into_iter().flatten().collect()
    });
    &EVERY
}

/// `server`'s tool `name`, none when it has no such tool.
pub(crate) fn tool(server: Server, name: &str) -> Option<&'static Tool> {
    every()
        .iter()
        .find(|t| t.server == server && t.name == name)
}

impl Tool {
    /// Its entry in `tools/list`, the broker's saying whether it only reads; none for one
    /// forwarded, which its own server describes.
    pub(crate) fn listed(&self) -> Option<Value> {
        let (d, s) = match &self.does {
            Does::Conversation(_, d, s) | Does::Broker(_, d, s) => (d, s),
            Does::Forwarded => return None,
        };
        let mut listed = json!({"name": self.name, "description": d, "inputSchema": s});
        if self.server == Server::Broker {
            listed["annotations"] = json!({"readOnlyHint": self.read_only});
        }
        Some(listed)
    }
}

impl Marks {
    /// Why `name`'s answer to a call with `args` marks where it's read, or none.
    pub(crate) fn why(self, name: &str, args: &Value, jobs: &crate::jobs::Jobs) -> Option<String> {
        match self {
            Marks::Nothing => None,
            Marks::With(why) => Some(why.into()),
            Marks::Answer => Some(format!("{name}'s answer")),
            Marks::ByJob => {
                let id = args["id"].as_str().unwrap_or_default();
                match jobs.get(id) {
                    Some(job) => job
                        .not_owners_words()
                        .map(|why| format!("the title of job {id}, which {why}")),
                    None => Some(format!("{name} on a job obid doesn't have")),
                }
            }
        }
    }
}

/// The MCP versions obid speaks, newest first (AGENTMCP-1): the only list of them.
pub(crate) const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// A conversation's answer to a tool call while no turn of its is under way (`-32600`).
pub(crate) const NO_TURN: &str = "No turn of Oby's is under way: tools are called only in one.";

/// A message's method, id and params (`{}` when none); none for a notification or no method.
pub(crate) fn read(body: &Value) -> Option<(&str, Value, Value)> {
    let method = body.get("method")?.as_str()?;
    let id = body.get("id").cloned().filter(|id| !id.is_null())?;
    let params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    Some((method, id, params))
}

/// `initialize`'s answer from the server `name`: the version asked for when obid speaks it.
pub(crate) fn hello(params: &Value, name: &str, instructions: &str) -> Value {
    let asked = params["protocolVersion"].as_str().unwrap_or("");
    json!({"protocolVersion": if VERSIONS.contains(&asked) { asked } else { VERSIONS[0] },
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {"name": name, "version": crate::api::API_VERSION},
        "instructions": instructions})
}

/// A `tools/call`'s tool name (empty when none) and arguments (`{}` when none).
pub(crate) fn called(params: &Value) -> (&str, Value) {
    let args = params.get("arguments").cloned();
    (
        params["name"].as_str().unwrap_or(""),
        args.unwrap_or_else(|| json!({})),
    )
}

pub(crate) fn ping() -> Value {
    json!({})
}

/// A method `server` doesn't serve (AGENTMCP-2), named by its first 64 characters.
pub(crate) fn unknown(id: Value, method: &str, server: &str) -> Value {
    let method: String = method.chars().take(64).collect();
    refused(id, -32601, &format!("{server} doesn't answer {method}."))
}

/// A tool call's result that failed, saying why.
pub(crate) fn failure(text: impl Into<String>) -> Value {
    json!({"content": [{"type": "text", "text": text.into()}], "isError": true})
}

/// A result, scrubbed (AGENTMCP-3).
pub(crate) fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": scrub(result)})
}

pub(crate) fn refused(id: Value, code: i64, text: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": text}})
}

/// `value` with every secret obid knows taken out of every string and every key, at any depth.
pub(crate) fn scrub(value: Value) -> Value {
    fn scrubbed(value: Value, secrets: &crate::secrets::Secrets) -> Value {
        match value {
            Value::String(s) => Value::String(secrets.redact(&s)),
            Value::Array(xs) => {
                Value::Array(xs.into_iter().map(|x| scrubbed(x, secrets)).collect())
            }
            Value::Object(xs) => Value::Object(
                xs.into_iter()
                    .map(|(k, v)| (secrets.redact(&k), scrubbed(v, secrets)))
                    .collect(),
            ),
            x => x,
        }
    }
    scrubbed(value, &crate::secrets::known())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// AGENTMCP-3: a vault secret in a result, as a nested string and as a key, leaves as its
    /// marker only.
    #[test]
    fn agentmcp_3_results_are_scrubbed_once() {
        let name = "vault oby.agentmcp-3.test";
        let secret = format!("agentmcp-3-value-{}", "v".repeat(32));
        crate::secrets::known().learn(name, &secret);
        let mut keyed = serde_json::Map::new();
        keyed.insert(secret.clone(), json!(true));
        let sent = ok(
            json!(3),
            json!({"content": [{"type": "text", "text": format!("the key is {secret}")}],
                   "keyed": keyed}),
        );
        crate::secrets::known().forget(name);
        let marker = format!("[redacted: {name}]");
        assert_eq!(
            sent["result"]["content"][0]["text"],
            format!("the key is {marker}"),
            "{sent}"
        );
        assert_eq!(sent["result"]["keyed"][&marker], true, "{sent}");
        assert!(!sent.to_string().contains(&secret), "{sent}");
    }

    /// Every source file of obid's, with their folder (as [`one_protocol_version_list`] walks them).
    fn sources() -> (PathBuf, Vec<PathBuf>) {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        assert!(files.len() > 100, "{}", src.display());
        (src, files)
    }

    /// How many entries (`Tool { …, server: …, … }`) `text` builds, and how many of them take
    /// fields from another (`..`, Rust's struct update) instead of naming each.
    fn entries_in(text: &str) -> (usize, usize) {
        let (mut built, mut updated) = (0, 0);
        for (at, _) in text.match_indices("Tool {") {
            let before = text[..at].trim_end();
            let named = text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == ':');
            if named || before.ends_with("struct") || before.ends_with("impl") {
                continue;
            }
            let body: Vec<char> = text[at + "Tool ".len()..].chars().collect();
            let (mut braces, mut parens, mut quoted, mut escaped) = (0, 0, false, false);
            let (mut end, mut update) = (body.len(), false);
            for (i, &c) in body.iter().enumerate() {
                if quoted {
                    (escaped, quoted) = (!escaped && c == '\\', escaped || c != '"');
                    continue;
                }
                match c {
                    '"' => quoted = true,
                    '{' => braces += 1,
                    '}' if braces == 1 => {
                        end = i;
                        break;
                    }
                    '}' => braces -= 1,
                    '(' | '[' => parens += 1,
                    ')' | ']' => parens -= 1,
                    '.' if braces == 1 && parens == 0 && body.get(i + 1) == Some(&'.') => {
                        update = true;
                    }
                    _ => {}
                }
            }
            if body[..end]
                .iter()
                .collect::<String>()
                .contains("server: Server::")
            {
                built += 1;
                updated += usize::from(update);
            }
        }
        (built, updated)
    }

    /// AGENTMCP-7: every tool of the three servers is an entry of [`every`] that names each of its
    /// gates: no `Default` for the entry or its gates, no entry built from another (`..`). The
    /// conversation's are in `tools/list`'s order, the broker's is `oby_ask_owner`, Tetrix's are
    /// `tetrix::TOOLS`, and what a job's tool marks agrees with crate::secretspec.
    #[test]
    fn agentmcp_7_every_entry_names_every_gate() {
        let names = |server| -> Vec<&str> {
            let ours = every().iter().filter(|t| t.server == server);
            ours.map(|t| t.name).collect()
        };
        let conversation = [
            "oby_jobs",
            "oby_job",
            "oby_dispatch",
            "oby_offer",
            "oby_tell_job",
            "oby_stop_job",
            "oby_continue_job",
            "oby_archive_job",
            "oby_delete_job",
            "oby_change_job",
            "oby_tasks",
            "oby_inbox",
            "oby_github",
            "oby_answer",
            "oby_merge",
            "oby_github_write",
            "oby_settings",
            "oby_routines",
            "oby_routine",
            "oby_change_routine",
            "oby_check_in",
            "oby_room_read",
            "oby_room_post",
            "oby_conversations",
            "oby_delete_conversations",
            "oby_members",
            "oby_ask",
            "oby_asks",
            "oby_artifact",
            "oby_read",
            "oby_search",
            "oby_list",
        ];
        assert_eq!(names(Server::Conversation), conversation);
        assert_eq!(names(Server::Broker), [crate::permissions::ASK_OWNER]);
        assert_eq!(names(Server::Tetrix), crate::tetrix::TOOLS);
        for server in [Server::Conversation, Server::Broker, Server::Tetrix] {
            let mut unique = names(server);
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), names(server).len(), "{server:?}");
        }
        for tool in every() {
            let served = match (&tool.does, tool.server) {
                (Does::Conversation(..), Server::Conversation) => "conversation",
                (Does::Broker(..), Server::Broker) => "oby",
                (Does::Forwarded, Server::Tetrix) => "tetrix",
                _ => panic!("{} does what another server does", tool.name),
            };
            if served != "conversation" {
                let safe =
                    crate::secretspec::tool_is_known_safe(&format!("mcp__{served}__{}", tool.name));
                assert_eq!(
                    tool.marks_on_result != Marks::Nothing,
                    !safe,
                    "{}",
                    tool.name
                );
            }
        }

        let (src, files) = sources();
        let mcp = std::fs::read_to_string(src.join("mcp.rs")).unwrap();
        let production = |text: &str| text.split("#[cfg(test)]").next().unwrap().to_owned();
        for kind in ["struct Tool", "enum Server", "enum Marks", "enum Does"] {
            let at = mcp.find(&format!("pub(crate) {kind} {{")).unwrap();
            let above = mcp[..at].lines().rev();
            let mut attributes = above.take_while(|l| l.starts_with("#[") || l.starts_with("///"));
            assert!(
                !attributes.any(|l| l.contains("Default")),
                "{kind} has a Default"
            );
        }
        let (mut built, mut updated) = (0, Vec::new());
        for file in &files {
            let text = production(&std::fs::read_to_string(file).unwrap());
            let name = file.strip_prefix(&src).unwrap().display().to_string();
            for kind in ["Tool", "Server", "Marks", "Does"] {
                let defaults = [
                    format!("Default for {kind} "),
                    format!("Default for mcp::{kind} "),
                ];
                let imports = name == "mcp.rs" || text.contains("mcp::");
                assert!(
                    !(imports && defaults.iter().any(|d| text.contains(d.as_str()))),
                    "{name}: a Default for {kind}"
                );
            }
            let (here, from_another) = entries_in(&text);
            built += here;
            if from_another > 0 {
                updated.push(name);
            }
        }
        // The conversation's 32, the broker's one and Tetrix's, built from its list in one place.
        assert!(built >= 34, "{built} entries found");
        assert!(
            updated.is_empty(),
            "an entry built with `..` in {updated:?}: name every gate"
        );
        let mine = r#"Tool { name: "x", server: Server::Broker, ..OTHER }"#;
        assert_eq!(entries_in(mine), (1, 1), "the check sees `..`");
        let named = r#"Tool { name: "x", server: Server::Broker, does: f(a..b), x: "..", y: z }"#;
        assert_eq!(entries_in(named), (1, 0), "nor a range, nor a string");
    }

    /// One protocol-version list: no source file but this one holds two of its versions.
    #[test]
    fn one_protocol_version_list() {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        assert!(files.len() > 100, "{}", src.display());
        let lists: Vec<String> = files
            .iter()
            .filter(|f| **f != src.join("mcp.rs"))
            .filter(|f| {
                let text = std::fs::read_to_string(f).unwrap();
                VERSIONS
                    .iter()
                    .filter(|v| text.contains(&format!("\"{v}\"")))
                    .count()
                    >= 2
            })
            .map(|f| f.strip_prefix(&src).unwrap().display().to_string())
            .collect();
        assert!(
            lists.is_empty(),
            "a second protocol-version list, in {lists:?}: use crate::mcp::VERSIONS"
        );
    }
}
