//! The chat finds past conversations before it answers (#593): `oby_conversations` runs the same
//! plain text search as `GET /v1/conversations?q=` and answers what it found, each conversation by
//! its title and id with the turns that said the words. What was said is quoted data in the turn's
//! fence, never instructions. The conversation asking is left out.

use super::briefing::line;
use crate::conversation::{Found, excerpt};

/// The most conversations one search quotes.
pub const CONVERSATIONS: usize = 5;

/// One search's section: each conversation found, by title and id, with the turns that hold the
/// words, the latest first.
pub fn section(said: &str, terms: &[String], found: &[Found]) -> String {
    let mut lines = vec![format!("## Found: {said}")];
    for conversation in found.iter().take(CONVERSATIONS) {
        lines.push(String::new());
        lines.push(format!(
            "### Conversation “{}” (id {})",
            line(&conversation.title, 100),
            conversation.id
        ));
        if conversation.hits.is_empty() {
            lines.push("(Its title says it; no turn kept does.)".into());
        }
        for (at, turn) in &conversation.hits {
            lines.push(format!(
                "Turn {} · {} of {} · {} · {}",
                turn.id,
                at,
                conversation.turns,
                if turn.who == "user" {
                    "the user"
                } else {
                    "Oby"
                },
                turn.at
            ));
            lines.push(excerpt(&turn.text, terms));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::Turn;

    fn found(title: &str, said: &[&str]) -> Found {
        let hits = said
            .iter()
            .enumerate()
            .map(|(n, text)| {
                (
                    n + 1,
                    Turn {
                        id: format!("turn-{n}"),
                        who: "user".into(),
                        text: (*text).into(),
                        at: "2026-10-01T00:00:00Z".into(),
                        jobs: vec![],
                        conversation: "c1".into(),
                        ended: false,
                        artifacts: vec![],
                        routine: None,
                        asked: None,
                        attachments: vec![],
                    },
                )
            })
            .collect();
        Found {
            id: "c1".into(),
            title: title.into(),
            summary: serde_json::Value::Null,
            turns: 41,
            hits,
        }
    }

    #[test]
    fn a_section_names_each_conversation_and_the_turns_that_said_it() {
        let terms = crate::conversation::search_words("Watch target");
        let found = [found("Release planning", &["the Watch target waits"])];
        let text = section("Watch target", &terms, &found);
        assert!(
            text.contains("### Conversation “Release planning” (id c1)"),
            "{text}"
        );
        assert!(
            text.contains(
                "Turn turn-0 · 1 of 41 · the user · 2026-10-01T00:00:00Z\nthe Watch target waits"
            ),
            "{text}"
        );
    }
}
