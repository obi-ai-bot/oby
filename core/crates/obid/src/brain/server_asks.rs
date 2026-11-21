//! What an own Codex conversation does when its agent calls a tool of one of the owner's own MCP
//! servers, not Oby's (docs/contracts/conversation-agent.md, CONVERSATION-AGENT-18), as T3 Code
//! does: in Bypass (T3's full access) Codex's approval of it is accepted; in every other mode the
//! owner is asked. Conversations have no card that waits mid-turn, so the asking is the question
//! card a turn already ends on (CONVERSATION-AGENT-47): the call is declined, the turn told why,
//! and the owner's signed "Allow once" lets that server's tool run on their next turn only. It
//! keeps no state: the grant is read from the question Oby's turn kept (`Turn::asked`) and the
//! owner's words after it, and goes with that turn.

use serde_json::json;

use crate::agent::Answer;
use crate::api::Shared;

use super::harness::Session;

/// The question's options: the first lets the tool run on the owner's next turn, the second leaves
/// it declined. Proposed words, Sangam's to change.
pub(crate) const ALLOW: &str = "Allow once";
pub(crate) const DONT: &str = "Don't allow";

/// What the owner is asked about `server`'s `tool`. Proposed words, Sangam's to change.
pub(crate) fn question(server: &str, tool: &str) -> String {
    format!(
        "Codex wants to run {tool} from your MCP server {server}. Allow it for your next message?"
    )
}

/// The server and tool of `name` (`mcp__<server>__<tool>`) when the server isn't Oby's own (`oby`,
/// or obid's `tetrix`).
fn of(name: &str) -> Option<(&str, &str)> {
    let (server, tool) = name.strip_prefix("mcp__")?.split_once("__")?;
    (!["oby", "tetrix"].contains(&server) && !tool.is_empty()).then_some((server, tool))
}

/// The question the owner's words this turn allow: their words were signed by an app of theirs,
/// are [`ALLOW`] exactly, and answer Oby's turn just before them, which asked it with [`ALLOW`] and
/// [`DONT`]. None otherwise: their own words, an older question or another answer allow nothing.
pub(crate) fn granted(
    shared: &Shared,
    conversation: &str,
    turn: &str,
    signed: bool,
) -> Option<String> {
    if !signed {
        return None;
    }
    let turns = shared.conversations.turns_but(conversation, &[]);
    let at = turns.iter().position(|t| t.id == turn)?;
    if turns[at].text.trim() != ALLOW {
        return None;
    }
    let before = turns[..at].iter().rfind(|t| t.who == "oby")?;
    let asked = before.asked.as_ref()?;
    (asked.options == [ALLOW, DONT]).then(|| asked.question.clone())
}

/// obid's answer when an own Codex session's call `name` is a tool of another MCP server than
/// Oby's; none for every other call. In `bypass`, yes. With the owner's `granted` question naming
/// it, yes. Otherwise the question is put to the owner as the turn's answer and the call declined,
/// in words that tell the agent to end its turn rather than try again.
pub(crate) fn rule(
    name: &str,
    bypass: bool,
    granted: Option<&str>,
    session: &Session,
) -> Option<Answer> {
    let (server, tool) = of(name)?;
    let asked = question(server, tool);
    if bypass || granted == Some(asked.as_str()) {
        return Some(Answer::Allow);
    }
    session.put_question(&json!({"questions": [{
        "question": asked,
        "header": "",
        "options": [{"label": ALLOW, "description": ""}, {"label": DONT, "description": ""}],
    }]}));
    Some(Answer::Deny(format!(
        "the owner hasn't allowed {tool} from their MCP server {server}, so it didn't run. Oby has asked them, over the message box. Don't call it again: end your turn now. Their answer comes as their next words."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CONVERSATION-AGENT-18: only another server's tool is asked about; Oby's and Tetrix's are
    /// obid's own rules'.
    #[test]
    fn conversation_agent_18_only_another_servers_tool_is_asked_about() {
        assert_eq!(of("mcp__elsewhere__run"), Some(("elsewhere", "run")));
        assert_eq!(of("mcp__oby__oby_jobs"), None);
        assert_eq!(of("mcp__tetrix__search_entities"), None);
        assert_eq!(of("Bash"), None);
        assert_eq!(of("mcp__elsewhere__"), None);
    }
}
