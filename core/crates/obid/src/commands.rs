//! The owner's own Claude Code's commands and skills, as the message box's `/` lists them
//! (docs/contracts/composer.md, COMPOSER-16 to 21): the names an own conversation's session gave in
//! its `init` line, kept in `home/commands.json` (0600), `GET /v1/commands`, and which words run as
//! one ([`runs`]) and which only look like one ([`guarded`]).
//!
//! State: that file, replaced whole each time an own session names its commands. It holds names
//! only, never what a command or skill says or does: running one is Claude Code's, which gets the
//! owner's words unchanged (`brain::harness`). A boxed session's list and a job's never reach it
//! (`brain::harness::one` passes only an own one's): neither is where the owner's words go.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::Shared;

/// One command, as Claude Code named it; `skill` when its `skills` names it too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub skill: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seen {
    seen_at: String,
    commands: Vec<Command>,
}

/// The names a list of the `init` line holds, trimmed, empty ones left out.
fn names(init: &Value, key: &str) -> Vec<String> {
    init[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n.as_str().map(str::trim))
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The commands an `init` line names (`slash_commands`), in its order, but those that run only in a
/// terminal (`terminal_slash_commands`) and Claude Code's own `_` ones; `None` from a Claude Code
/// whose `init` names none (before 2.1.317), so an older one leaves the last list as it was.
pub fn named(init: &Value) -> Option<Vec<Command>> {
    init.get("slash_commands")?.as_array()?;
    let terminal = names(init, "terminal_slash_commands");
    let skills = names(init, "skills");
    let mut commands: Vec<Command> = Vec::new();
    for name in names(init, "slash_commands") {
        if name.starts_with('_')
            || terminal.contains(&name)
            || commands.iter().any(|c| c.name == name)
        {
            continue;
        }
        commands.push(Command {
            skill: skills.contains(&name),
            name,
        });
    }
    Some(commands)
}

fn file(home: &Path) -> PathBuf {
    home.join("commands.json")
}

/// An own session named `commands`: they replace what was kept. A failed write is logged, never
/// the turn's.
pub fn saw(home: &Path, commands: Vec<Command>) {
    let seen = Seen {
        seen_at: obi_protocol::time::format(jiff::Timestamp::now().as_second()),
        commands,
    };
    let bytes = serde_json::to_vec_pretty(&seen).unwrap_or_default();
    if let Err(e) = crate::store::write_private(&file(home), &bytes) {
        tracing::warn!("obid couldn't keep the owner's commands: {e}");
    }
}

/// What was kept, or none before any own session named its commands (or a file that doesn't read).
fn kept(home: &Path) -> Option<Seen> {
    serde_json::from_slice(&std::fs::read(file(home)).ok()?).ok()
}

/// The commands kept, none before any was seen.
pub fn listed(home: &Path) -> Vec<Command> {
    kept(home).map(|s| s.commands).unwrap_or_default()
}

/// The command `words` start with, when their first word is `/` and a name `listed` holds.
fn named_first<'a>(words: &str, listed: &'a [Command]) -> Option<&'a Command> {
    let name = words
        .trim_start()
        .strip_prefix('/')?
        .split(char::is_whitespace)
        .next()?;
    listed.iter().find(|c| !name.is_empty() && c.name == name)
}

/// Whether the owner's `said` runs as a command (COMPOSER-19): their first word is `/` and the name
/// of one obid keeps. The rest of what makes it one (signed, alone, nothing attached, an own Claude
/// Code session) is the turn's, `brain::harness`'s.
pub fn runs(said: &str, listed: &[Command]) -> bool {
    named_first(said, listed).is_some()
}

/// What goes before a prompt that isn't a command but would start like one (COMPOSER-20).
pub const NOT_A_COMMAND: &str = "These words aren't a command:";

/// `prompt` as a session is given it when its words aren't a command: one that starts with the
/// name of a command obid keeps gets [`NOT_A_COMMAND`] before it, so Claude Code takes it as words;
/// any other is unchanged (an unknown `/name`, a path, is words to Claude Code already).
pub fn guarded(prompt: String, listed: &[Command]) -> String {
    match named_first(&prompt, listed) {
        Some(_) => format!("{NOT_A_COMMAND}\n{prompt}"),
        None => prompt,
    }
}

pub fn router() -> Router<Arc<Shared>> {
    Router::new().route("/v1/commands", get(list))
}

async fn list(State(shared): State<Arc<Shared>>) -> Json<Value> {
    Json(json!({"claude": kept(&shared.config.home)}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str, skill: bool) -> Command {
        Command {
            name: name.into(),
            skill,
        }
    }

    /// COMPOSER-18: what an own session's `init` names, terminal-only and `_` ones left out,
    /// skills marked, in its order.
    #[test]
    fn composer_18_commands_are_read_from_init() {
        let init = json!({"type": "system", "subtype": "init",
            "slash_commands": ["simplify", "compact", "doctor", "__remote-workflow", "vercel:deploy", " ", "compact"],
            "terminal_slash_commands": ["doctor"],
            "skills": ["simplify", "vercel:deploy", "doctor"]});
        assert_eq!(
            named(&init),
            Some(vec![
                command("simplify", true),
                command("compact", false),
                command("vercel:deploy", true),
            ])
        );
        assert_eq!(named(&json!({"type": "system", "subtype": "init"})), None);
        assert_eq!(named(&json!({"slash_commands": []})), Some(vec![]));
    }

    /// COMPOSER-18: the next own session's list replaces the last, in a private file.
    #[test]
    fn composer_18_the_next_list_replaces_the_last() {
        let home = std::env::temp_dir().join(format!("obid-commands-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(kept(&home), None);
        assert!(listed(&home).is_empty());
        saw(&home, vec![command("a", false), command("b", true)]);
        saw(&home, vec![command("c", true)]);
        let seen = kept(&home).unwrap();
        assert_eq!(seen.commands, vec![command("c", true)]);
        assert!(seen.seen_at.ends_with('Z'));
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(file(&home)).unwrap().permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// COMPOSER-19: words whose first word is `/` and a kept name run as that command, with what
    /// follows as its arguments.
    #[test]
    fn composer_19_words_alone_run_as_a_command() {
        let listed = [command("simplify", true), command("vercel:deploy", true)];
        assert!(runs("/simplify", &listed));
        assert!(runs("/simplify the diff\nand the tests", &listed));
        assert!(runs("  /vercel:deploy prod", &listed));
    }

    /// COMPOSER-20: anything else starting with `/` is words; one that would start like a kept
    /// command is guarded, any other left as it is.
    #[test]
    fn composer_20_other_slash_words_are_words() {
        let listed = [command("simplify", true)];
        for words in [
            "/Users/me/notes.txt what's in it?",
            "/simplifyit",
            "/ simplify",
            "simplify /simplify",
            "/",
            "",
        ] {
            assert!(!runs(words, &listed), "{words:?}");
            assert_eq!(guarded(words.to_owned(), &listed), words, "{words:?}");
        }
        assert!(!runs("/simplify", &[]));
        assert_eq!(
            guarded("/simplify the diff".into(), &listed),
            format!("{NOT_A_COMMAND}\n/simplify the diff")
        );
    }
}
