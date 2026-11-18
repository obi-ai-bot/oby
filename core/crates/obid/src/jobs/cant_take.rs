//! Why an account can't take a task now (docs/contracts/accounts.md, ACCOUNTS-30 and 32): what
//! [`super::Jobs::cant_take`] found, in the words a refusal and a routing line say it with. It owns
//! no state and never writes a job, a login or the configuration.

/// What keeps an account from a task, each in plain words with no path or raw error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CantTake {
    /// Its tool's CLI isn't on this Mac: the tool's name.
    Missing(String),
    /// It isn't signed in: how the owner calls it.
    SignedOut(String),
    /// Its login or its start failed: the whole clause, "<account> couldn't start: <reason>".
    Failed(String),
}

impl CantTake {
    /// The clause a line is made of: "Cursor isn't installed on this Mac".
    pub fn clause(&self) -> String {
        match self {
            CantTake::Missing(tool) => format!("{tool} isn't installed on this Mac"),
            CantTake::SignedOut(who) => format!("{who} isn't signed in"),
            CantTake::Failed(clause) => clause.clone(),
        }
    }

    /// Why a task on an account the owner named didn't start, and what they can do (ACCOUNTS-32).
    pub fn refusal(&self) -> String {
        let fix = match self {
            CantTake::Missing(_) => "install it, or ask for another account",
            CantTake::SignedOut(_) => {
                "sign it in from Settings › Agents, or ask for another account"
            }
            CantTake::Failed(_) => "ask for another account, or try again later",
        };
        format!("{}, so this task didn't start: {fix}.", self.clause())
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
}
