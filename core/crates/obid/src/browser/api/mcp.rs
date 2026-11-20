//! The agents' tool results (BROWSER-1, 9, 15): an engine answer or an API refusal as an MCP tool
//! result, `isError` and the reason's code kept. It owns no state and writes nothing.

use serde_json::{Value, json};

use super::Refused;

/// An MCP tool result from an engine answer (`{ok, result}` or `{ok: false, error}`).
pub fn mcp_result(structured: Value) -> Value {
    let is_error = structured["ok"] == false;
    let text = if is_error {
        format!(
            "{} ({})",
            structured["error"]["message"].as_str().unwrap_or("refused"),
            structured["error"]["code"].as_str().unwrap_or("browser")
        )
    } else {
        structured["result"].to_string()
    };
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
        "isError": is_error,
    })
}

/// A refusal as an MCP tool result.
pub fn mcp_refusal(r: &Refused) -> Value {
    mcp_result(json!({"ok": false, "error": {"code": r.reason, "message": r.message}}))
}
