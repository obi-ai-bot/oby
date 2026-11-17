//! Why an account's agent couldn't start (docs/contracts/accounts.md, ACCOUNTS-19 to 22): the reason
//! its last start failed with, in plain words, kept in memory per account until a start on it goes
//! through, it's signed in again, or [`KEPT_FOR`] has passed since it last failed. It owns
//! `Jobs::cant_run`; it never writes a job, the configuration or a login. A restart forgets it, as
//! the next start finds the reason again.

use super::*;

/// How long a reason holds routing off its account after it last failed (ACCOUNTS-20): long enough
/// that the next tasks don't each fail on it, short enough that a reason the owner fixed (a setting,
/// a login refreshed outside Oby) is tried again by itself.
const KEPT_FOR: i64 = 10 * 60;

/// Each account's reason, and when it was last seen.
#[derive(Default)]
pub struct CantRun {
    kept: Mutex<HashMap<String, (String, i64)>>,
}

impl CantRun {
    fn kept(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, i64)>> {
        self.kept.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `account` couldn't start at `now`, for `why` (already plain): whether what it says changed.
    pub fn set(&self, account: &str, why: &str, now: i64) -> bool {
        let before = self
            .kept()
            .insert(account.to_owned(), (why.to_owned(), now));
        before.is_none_or(|(was, at)| was != why || now - at >= KEPT_FOR)
    }

    /// A start on `account` went through, or it was signed in again: whether it had a reason.
    pub fn clear(&self, account: &str) -> bool {
        self.kept().remove(account).is_some()
    }

    /// Why `account` can't run, while that's kept at `now`.
    pub fn why(&self, account: &str, now: i64) -> Option<String> {
        let kept = self.kept();
        let (why, at) = kept.get(account)?;
        (now - at < KEPT_FOR).then(|| why.clone())
    }
}

impl Jobs {
    /// `account`'s agent couldn't start, for `why`: kept on the account in plain words, and the
    /// apps told (the `accounts` event). A sign-in or removal under way is the owner's own doing,
    /// not a reason. What it keeps, for the caller to say.
    pub fn cant_start(&self, account: &crate::config::Account, why: &str) -> String {
        let plain = plain(why);
        if !self.being_changed(&account.id)
            && self
                .cant_run
                .set(&account.id, &plain, obi_protocol::time::now())
        {
            self.usage_changed.notify_one();
        }
        plain
    }

    /// A start on `account` went through: it has no reason any more.
    pub fn started_on(&self, account: &str) {
        if self.cant_run.clear(account) {
            self.usage_changed.notify_one();
        }
    }

    /// Why `account` can't run now, while kept ([`CantRun::why`]).
    pub fn cant_run_why(&self, account: &str) -> Option<String> {
        self.cant_run.why(account, obi_protocol::time::now())
    }
}

/// Why `account`'s login, there, can't run ([`login_problem`]), for the conversation's line.
pub(crate) fn login_problem_of(
    tool: Tool,
    account: &crate::config::Account,
    home: Option<&Path>,
) -> Option<String> {
    login_problem(tool, account, home)
}

/// Whether a run ended because its login wasn't taken: the tool said so with a 401 or 403
/// ([`crate::agent::Cause::Login`]), or in its own words with none, as Claude Code 2.1 does for a
/// folder whose login it can't use (`Not logged in · Please run /login`).
pub(crate) fn login_refused(cause: Option<crate::agent::Cause>, words: &str) -> bool {
    let words = words.to_lowercase();
    cause == Some(crate::agent::Cause::Login)
        || ["not logged in", "please run /login", "invalid api key"]
            .iter()
            .any(|w| words.contains(w))
}

/// `why` in plain words for the owner (ACCOUNTS-19): its first line, the secrets obid knows taken
/// out, each path down to its last name (no home folder, `~/…` or `[oby]` in it), and no closing
/// full stop, to go inside a sentence.
pub fn plain(why: &str) -> String {
    let line = crate::secrets::known().redact(why.lines().next().unwrap_or_default());
    let words: Vec<String> = line.split(' ').map(unpathed).collect();
    words.join(" ").trim().trim_end_matches('.').to_owned()
}

/// One word with a path in it down to its last name: `(~/.claude/settings.json),` is
/// `(settings.json),`.
fn unpathed(word: &str) -> String {
    // `[oby]` and `[home]` open a path: a bracket is trimmed off its end only.
    let core = word
        .trim_start_matches(|c: char| "({'\"`".contains(c))
        .trim_end_matches(|c: char| ")]},;:'\"`".contains(c));
    let core = core.strip_suffix('.').unwrap_or(core);
    // A bare `/login` is a command, not a path: a path has a folder in it.
    let path = (core.starts_with('/') && core[1..].contains('/'))
        || core.starts_with("~/")
        || core.starts_with("[oby]/")
        || core.starts_with("[home]/");
    if !path || core.len() < 2 {
        return word.to_owned();
    }
    let name = core
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    word.replacen(core, name, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_19_a_reason_is_plain_words_with_no_path() {
        assert_eq!(
            plain("~/.claude/settings.json sets CLAUDE_X in its env.\nmore"),
            "settings.json sets CLAUDE_X in its env"
        );
        assert_eq!(
            plain("obid can't find Claude Code's binary (/Users/someone/bin/claude)."),
            "obid can't find Claude Code's binary (claude)"
        );
        assert_eq!(
            plain("no login in [oby]/accounts/a1/, so it didn't start"),
            "no login in a1, so it didn't start"
        );
        assert_eq!(plain("hit 50/50 of it / now"), "hit 50/50 of it / now");
        assert_eq!(
            plain("To be asked again, run claude, then /login."),
            "To be asked again, run claude, then /login"
        );
    }

    #[test]
    fn accounts_19_a_login_refused_in_words_counts() {
        use crate::agent::Cause;
        assert!(login_refused(None, "Not logged in · Please run /login"));
        assert!(login_refused(Some(Cause::Login), "API Error: 401"));
        assert!(!login_refused(Some(Cause::Other), "Two tests still fail"));
    }

    #[test]
    fn accounts_20_a_reason_holds_for_ten_minutes_and_a_start_clears_it() {
        let kept = CantRun::default();
        assert!(kept.set("a", "no login", 100));
        assert!(
            !kept.set("a", "no login", 160),
            "the same words are no news"
        );
        assert_eq!(
            kept.why("a", 160 + KEPT_FOR - 1).as_deref(),
            Some("no login")
        );
        assert_eq!(kept.why("a", 160 + KEPT_FOR), None);
        assert!(
            kept.set("a", "no login", 160 + KEPT_FOR),
            "seen again: news again"
        );
        assert!(kept.clear("a"));
        assert_eq!(kept.why("a", 200), None);
        assert!(!kept.clear("a"));
    }
}
