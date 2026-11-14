//! The MCP plumbing of obid's servers for agents, written once (docs/contracts/agent-mcp.md): the
//! job's broker (crate::connections), the conversation's tools (crate::brain::tools) and Tetrix's.

use serde_json::{Value, json};

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

pub(crate) fn ping() -> Value {
    json!({})
}

/// A method `server` doesn't serve (AGENTMCP-2), named by its first 64 characters.
pub(crate) fn unknown(id: Value, method: &str, server: &str) -> Value {
    let method: String = method.chars().take(64).collect();
    refused(id, -32601, &format!("{server} doesn't answer {method}."))
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
