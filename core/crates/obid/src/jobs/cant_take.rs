//! Why an account can't take a task now (docs/contracts/accounts.md, ACCOUNTS-30, 32, 35 and 37):
//! what [`super::Jobs::cant_take`] found, in the words a refusal and a routing line say it with,
//! and the one thing the owner can do about it (Install, Sign in). It owns no state and never
//! writes a job, a login or the configuration.

use serde_json::{Value, json};

use crate::agent::Tool;
use crate::limits::holds::Work;

/// What keeps an account from a task, each in plain words with no path or raw error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CantTake {
    /// Its tool's CLI isn't on this Mac: the tool's name.
    Missing(String),
    /// It isn't signed in: how the owner calls it.
    SignedOut(String),
    /// Its login is there but can't run (macOS refused it, its token ended): the whole clause,
    /// "<account> can't run now: <reason>".
    Login(String),
    /// Its agent couldn't start a moment ago (`cantRun`): the whole clause, "<account> couldn't
    /// start: <reason>".
    Failed(String),
}

impl CantTake {
    /// The clause a line is made of: "Cursor isn't installed on this Mac".
    pub fn clause(&self) -> String {
        match self {
            CantTake::Missing(tool) => format!("{tool} isn't installed on this Mac"),
            CantTake::SignedOut(who) => format!("{who} isn't signed in"),
            CantTake::Login(clause) | CantTake::Failed(clause) => clause.clone(),
        }
    }

    /// Whether a task naming this account is refused before it starts: its CLI isn't on this Mac
    /// (ACCOUNTS-32), but for a follow-up nobody's words asked for now (`asked_now`); or it isn't
    /// signed in, when the owner named it (`owner_named`: in `POST /v1/jobs`, or their words to a
    /// task they named it for, ACCOUNTS-35). One whose agent couldn't start a moment ago still goes
    /// there and says why (ACCOUNTS-20), as a signed-out one a conversation, a routine or a review
    /// names does.
    pub fn refuses(&self, asked_now: bool, follow_up: bool, owner_named: bool) -> bool {
        match self {
            CantTake::Missing(_) => asked_now || !follow_up,
            CantTake::SignedOut(_) | CantTake::Login(_) => owner_named,
            CantTake::Failed(_) => false,
        }
    }

    /// Why a task on an account the owner named didn't start, and what they can do (ACCOUNTS-32).
    pub fn refusal(&self) -> String {
        let fix = match self {
            CantTake::Missing(_) => "install it, or ask for another account",
            CantTake::SignedOut(_) | CantTake::Login(_) => {
                "sign it in from Settings › Agents, or ask for another account"
            }
            CantTake::Failed(_) => "ask for another account, or try again later",
        };
        format!("{}, so this task didn't start: {fix}.", self.clause())
    }

    /// What the owner can do about it, for the app to show under the refusal (ACCOUNTS-37):
    /// `{"action": "install" | "signIn", "agent", "account"}`; none for an agent that couldn't
    /// start a moment ago, which nothing of theirs fixes.
    pub fn fix(&self, tool: Tool, account: &str) -> Option<Value> {
        let action = match self {
            CantTake::Missing(_) => "install",
            CantTake::SignedOut(_) | CantTake::Login(_) => "signIn",
            CantTake::Failed(_) => return None,
        };
        Some(json!({"action": action, "agent": tool.id(), "account": account}))
    }
}

impl super::Jobs {
    /// What the owner can do about a task on the account they `named` (of `agent`'s, when they
    /// named one too) that can't take it (ACCOUNTS-37): none when nothing named one, or it can.
    pub fn fix_for(&self, agent: Option<&str>, named: Option<&str>) -> Option<Value> {
        let named = named
            .map(str::trim)
            .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("auto"))?;
        let work = Work::new(None, None, false);
        let (tool, account) = self.pick_for(agent, Some(named), &work).ok()?;
        self.cant_take(tool, &account)?.fix(tool, &account.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ACCOUNTS-32: a refusal says what keeps it and what to do, in words with no path.
    #[test]
    fn accounts_32_a_refusal_says_why_and_what_to_do() {
        assert_eq!(
            CantTake::SignedOut("Codex · Codex Two".into()).refusal(),
            "Codex · Codex Two isn't signed in, so this task didn't start: sign it in from Settings › Agents, or ask for another account."
        );
        assert_eq!(
            CantTake::Missing("Cursor".into()).refusal(),
            "Cursor isn't installed on this Mac, so this task didn't start: install it, or ask for another account."
        );
    }

    /// ACCOUNTS-32, 35: a signed-out account is refused only when the owner named it (in
    /// `POST /v1/jobs`, or their words to a task they named it for); one a conversation, a routine,
    /// a review or their words to an unnamed task carry goes there. A missing CLI is refused but
    /// for a follow-up nobody asked for now; an agent that couldn't start a moment ago never is
    /// (ACCOUNTS-20).
    #[test]
    fn accounts_35_only_the_owner_naming_it_refuses_a_signed_out_account() {
        for out in [
            CantTake::SignedOut("Claude Code".into()),
            CantTake::Login("Cursor can't run now: run cursor-agent login".into()),
        ] {
            for (asked_now, follow_up) in [(true, false), (true, true), (false, true)] {
                assert!(out.refuses(asked_now, follow_up, true), "{out:?}");
                assert!(!out.refuses(asked_now, follow_up, false), "{out:?}");
            }
        }
        let missing = CantTake::Missing("Cursor".into());
        assert!(missing.refuses(true, true, false) && missing.refuses(false, false, false));
        assert!(!missing.refuses(false, true, true));
        let failed = CantTake::Failed("Codex couldn't start: no binary".into());
        assert!(!failed.refuses(true, false, true) && !failed.refuses(false, false, true));
    }

    /// ACCOUNTS-37: a missing CLI's refusal carries Install, a signed-out account's Sign in, each
    /// with its agent and account; one that couldn't start a moment ago carries none.
    #[test]
    fn accounts_37_a_refusal_carries_install_or_sign_in() {
        let missing = CantTake::Missing("Cursor".into()).fix(Tool::Cursor, "u1");
        assert_eq!(
            missing,
            Some(json!({"action": "install", "agent": "cursor", "account": "u1"}))
        );
        for out in [
            CantTake::SignedOut("Codex".into()),
            CantTake::Login("Codex can't run now: run codex login".into()),
        ] {
            assert_eq!(out.fix(Tool::Codex, "u2").unwrap()["action"], "signIn");
        }
        assert_eq!(CantTake::Failed("x".into()).fix(Tool::Claude, "u3"), None);
    }
}
