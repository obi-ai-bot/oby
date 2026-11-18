//! AUTOFIX (docs/contracts/autofix.md, [why](../../../../docs/decisions/2026-11-17-red-ci-fixes-itself.md)):
//! what a red read of a pull request the owner's own task pushed leads to: a rerun on GitHub with
//! no agent turn, one more round of that same task, the merge queue again, or giving up. It owns
//! the job's `autofix` state (counted in its file, never in the API) and decides from typed facts
//! only (each check's conclusion, the head, its own counts), never from a log's words. It does
//! nothing itself: `jobs::autofix` reads GitHub, starts the round and tells the conversation.

use serde::{Deserialize, Serialize};

/// Fix rounds per pull request, every head counted, before it gives up (AUTOFIX-9); the owner's
/// `One more round` adds as many again (AUTOFIX-10).
pub const FIX_ROUNDS: u32 = 3;
/// Reruns on GitHub per head, with no agent turn (AUTOFIX-6, 7).
pub const RERUNS_PER_HEAD: u32 = 2;
/// Times one head goes back into the merge queue after another entry's failure (AUTOFIX-12).
pub const REQUEUES_PER_HEAD: u32 = 3;
/// How long red may last with nothing green before it gives up (AUTOFIX-9).
pub const GIVE_UP_AFTER: i64 = 24 * 3600;

/// How long after a rerun the same head's red is GitHub's old answer, not the rerun's: 90
/// seconds, or `OBID_RERUN_SETTLE_SECONDS` (obid's tests set 0).
pub fn rerun_settle() -> i64 {
    std::env::var("OBID_RERUN_SETTLE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(90)
}

/// The conclusions that say GitHub's runner, not the code, stopped a check (AUTOFIX-6).
pub const INFRA: [&str; 4] = ["CANCELLED", "TIMED_OUT", "STARTUP_FAILURE", "STALE"];

/// What obid did by itself about the job's own pull request's red checks, kept in its file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFix {
    /// Fix rounds started.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fixes: u32,
    /// Rounds the owner's `One more round` added.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more: u32,
    /// Each rerun: the head and when.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reruns: Vec<(String, i64)>,
    /// The heads put back into the merge queue, one entry each time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requeues: Vec<String>,
    /// The last fix round: the head it started at and the checks that had failed there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<(String, Vec<String>)>,
    /// When this run of red reads began; cleared by a green one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub red_since: Option<i64>,
    /// The head it gave up at, and why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gave_up: Option<(String, GaveUp)>,
    /// What it tells the job's conversation, in obid's words, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<String>,
    /// How many of `lines` the conversation was told.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub told: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Why it gave up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GaveUp {
    /// Its fix rounds ran out.
    Rounds,
    /// A fix round's head failed the same checks as the head before it.
    Same,
    /// A fix round left the head as it was.
    Unchanged,
    /// Red for a day with nothing green.
    Day,
    /// The merge queue took it out again and again for another entry's failure.
    Queue,
    /// The failure couldn't be quoted for its agent (the vault unreadable, or no round could start).
    NoRound,
}

/// What a red read leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing now: a rerun is still settling.
    Wait,
    /// Its failed runs again on GitHub, with no agent turn.
    Rerun,
    /// One more round of its agent's.
    Fix,
    /// Over to the owner: one line, and its card.
    GiveUp(GaveUp),
}

/// What [`decide`] weighs, counted from the job's [`AutoFix`] and the read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// Why it gave up at this head already, if it did.
    pub gave_up_here: Option<GaveUp>,
    /// A rerun of this head is younger than [`rerun_settle`].
    pub settling: bool,
    /// Red for at least [`GIVE_UP_AFTER`].
    pub a_day: bool,
    /// The last fix round started at this very head and left it so.
    pub fixed_here: bool,
    /// The last fix round's head failed exactly the checks this one does.
    pub same_failure: bool,
    /// Every failed check stopped for its runner ([`INFRA`]).
    pub infra: bool,
    /// The job's own check passed on this head here.
    pub checked_here: bool,
    /// Reruns of this head so far.
    pub reruns_here: u32,
    /// Fix rounds so far, and how many it may have.
    pub fixes: u32,
    pub allowed: u32,
}

/// What a red read leads to (AUTOFIX-6 to 9): a rerun while one settles waits; a runner's failure
/// reruns ([`RERUNS_PER_HEAD`]); a first failure on a head its own check passed on reruns once;
/// then a fix round, unless it ran out, failed the same way twice, left the head as it was or
/// stayed red a day.
pub fn decide(f: Facts) -> Step {
    if let Some(why) = f.gave_up_here {
        return Step::GiveUp(why);
    }
    if f.settling {
        return Step::Wait;
    }
    if f.a_day {
        return Step::GiveUp(GaveUp::Day);
    }
    if f.fixed_here {
        return Step::GiveUp(GaveUp::Unchanged);
    }
    if f.reruns_here < RERUNS_PER_HEAD && (f.infra || (f.checked_here && f.reruns_here == 0)) {
        return Step::Rerun;
    }
    if f.same_failure {
        return Step::GiveUp(GaveUp::Same);
    }
    if f.fixes >= f.allowed {
        return Step::GiveUp(GaveUp::Rounds);
    }
    Step::Fix
}

/// A red read of `head`: the names of the checks that failed (`failed`, as GitHub names them),
/// whether every one stopped for its runner (`infra`), and whether the job's own check passed on
/// it here (`checked_here`).
#[derive(Clone, Copy)]
pub struct Red<'a> {
    pub head: &'a str,
    pub failed: &'a [String],
    pub infra: bool,
    pub checked_here: bool,
    pub now: i64,
}

impl AutoFix {
    /// Whether nothing was ever done: not kept in the job's file.
    pub fn is_empty(&self) -> bool {
        *self == AutoFix::default()
    }

    /// The fix rounds it may have: [`FIX_ROUNDS`] and what the owner added.
    pub fn allowed(&self) -> u32 {
        FIX_ROUNDS.saturating_add(self.more)
    }

    fn reruns_of(&self, head: &str) -> u32 {
        self.reruns.iter().filter(|(h, _)| h == head).count() as u32
    }

    /// The [`Facts`] of `red` against what it did so far, `red_since` noted.
    pub fn facts(&mut self, red: &Red) -> Facts {
        let since = *self.red_since.get_or_insert(red.now);
        let last_rerun = (self.reruns.iter().rev())
            .find(|(h, _)| h == red.head)
            .map(|(_, at)| *at);
        let failed = signature(red.failed);
        Facts {
            gave_up_here: (self.gave_up.as_ref())
                .filter(|(h, _)| h == red.head)
                .map(|(_, why)| *why),
            settling: last_rerun.is_some_and(|at| red.now - at < rerun_settle()),
            a_day: red.now - since >= GIVE_UP_AFTER,
            fixed_here: self.fixed.as_ref().is_some_and(|(h, _)| h == red.head),
            same_failure: self
                .fixed
                .as_ref()
                .is_some_and(|(h, was)| h != red.head && *was == failed),
            infra: red.infra,
            checked_here: red.checked_here,
            reruns_here: self.reruns_of(red.head),
            fixes: self.fixes,
            allowed: self.allowed(),
        }
    }

    /// What `red` leads to now ([`decide`]).
    pub fn next(&mut self, red: &Red) -> Step {
        let facts = self.facts(red);
        decide(facts)
    }

    /// A green read: red reads count from the next one again.
    pub fn green(&mut self) {
        self.red_since = None;
    }

    /// A rerun of `head` went out at `now`.
    pub fn reran(&mut self, head: &str, now: i64) {
        self.reruns.push((head.to_owned(), now));
    }

    /// A fix round started at `head`, which failed `failed`.
    pub fn fixing(&mut self, head: &str, failed: &[String]) {
        self.fixes = self.fixes.saturating_add(1);
        self.fixed = Some((head.to_owned(), signature(failed)));
    }

    /// It gives up at `head` for `why`: true the first time, when its line is said.
    pub fn give_up(&mut self, head: &str, why: GaveUp) -> bool {
        let first = self.gave_up.as_ref().is_none_or(|(h, _)| h != head);
        self.gave_up = Some((head.to_owned(), why));
        first
    }

    /// The owner's `One more round`: [`FIX_ROUNDS`] more, the stop it gave up at lifted, and a red
    /// day counted from now.
    pub fn more(&mut self) {
        self.more = self.more.saturating_add(FIX_ROUNDS);
        self.gave_up = None;
        self.red_since = None;
        // On the owner's word, the head a round left as it was and the same failure get a round.
        self.fixed = None;
    }

    /// The merge queue took `head` out for another entry's failure: whether it goes back in
    /// ([`REQUEUES_PER_HEAD`]), counted.
    pub fn requeue(&mut self, head: &str) -> bool {
        let times = self.requeues.iter().filter(|h| *h == head).count() as u32;
        if times >= REQUEUES_PER_HEAD {
            return false;
        }
        self.requeues.push(head.to_owned());
        true
    }

    /// Says `line` in the job's conversation (crate::conversation, AUTOFIX-1).
    pub fn say(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    /// The lines its conversation wasn't told yet.
    pub fn untold(&self) -> &[String] {
        self.lines.get(self.told as usize..).unwrap_or_default()
    }
}

/// The failed checks as compared across heads: their names, sorted, once each.
fn signature(failed: &[String]) -> Vec<String> {
    let mut names = failed.to_vec();
    names.sort();
    names.dedup();
    names
}

/// What tells one failure from another across heads (AUTOFIX-9): the names of the checks that
/// failed and a digest of what GitHub logged of them, its last 50 lines with every digit taken out,
/// so run ids, times and durations don't make the same failure look new. Never read for its words.
pub fn failure(names: &[String], log: &str) -> Vec<String> {
    let lines: Vec<String> = (log.lines())
        .map(|l| {
            l.chars()
                .filter(|c| !c.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|l| !l.trim().is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(50)..].join("\n");
    let digest = hex::encode(obi_protocol::keys::sha256(tail.as_bytes()));
    let mut failed = names.to_vec();
    failed.push(format!("log {}", &digest[..16]));
    failed
}

/// The line a give-up says, in obid's words (AUTOFIX-10).
pub fn give_up_line(why: GaveUp, allowed: u32) -> String {
    let why = match why {
        GaveUp::Rounds => format!("Its checks on GitHub still fail after {allowed} fixes."),
        GaveUp::Same => "Its checks on GitHub failed the same way after a fix.".into(),
        GaveUp::Unchanged => "Its last fix round changed nothing.".into(),
        GaveUp::Day => "Its checks on GitHub have been failing for a day.".into(),
        GaveUp::Queue => {
            "GitHub's merge queue keeps taking it out for other changes' failures.".into()
        }
        GaveUp::NoRound => "Its failed checks couldn't be handed to its agent.".into(),
    };
    format!("{why} Over to you.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            gave_up_here: None,
            settling: false,
            a_day: false,
            fixed_here: false,
            same_failure: false,
            infra: false,
            checked_here: false,
            reruns_here: 0,
            fixes: 0,
            allowed: FIX_ROUNDS,
        }
    }

    fn red<'a>(head: &'a str, failed: &'a [String], now: i64) -> Red<'a> {
        Red {
            head,
            failed,
            infra: false,
            checked_here: false,
            now,
        }
    }

    #[test]
    fn autofix_1_a_first_code_failure_gets_a_fix_round() {
        assert_eq!(decide(facts()), Step::Fix);
    }

    #[test]
    fn autofix_6_a_runners_failure_reruns_twice_then_fixes() {
        let mut f = facts();
        f.infra = true;
        assert_eq!(decide(f), Step::Rerun);
        f.reruns_here = 1;
        assert_eq!(decide(f), Step::Rerun);
        f.reruns_here = 2;
        assert_eq!(decide(f), Step::Fix);
    }

    #[test]
    fn autofix_7_a_head_its_check_passed_on_reruns_once_first() {
        let mut f = facts();
        f.checked_here = true;
        assert_eq!(decide(f), Step::Rerun);
        f.reruns_here = 1;
        assert_eq!(decide(f), Step::Fix);
        f.settling = true;
        assert_eq!(decide(f), Step::Wait, "a rerun still settling");
    }

    #[test]
    fn autofix_9_three_rounds_and_no_loop_on_the_same_failure() {
        let test = vec!["test".to_owned()];
        let mut state = AutoFix::default();
        for (n, head) in ["a", "b", "c"].into_iter().enumerate() {
            let failed = vec![format!("test-{n}")];
            assert_eq!(state.next(&red(head, &failed, 0)), Step::Fix, "{head}");
            state.fixing(head, &failed);
        }
        assert_eq!(
            state.next(&red("d", &test, 0)),
            Step::GiveUp(GaveUp::Rounds)
        );
        let mut state = AutoFix::default();
        state.fixing("a", &test);
        assert_eq!(state.next(&red("b", &test, 0)), Step::GiveUp(GaveUp::Same));
        assert_eq!(
            state.next(&red("a", &test, 0)),
            Step::GiveUp(GaveUp::Unchanged)
        );
        let mut state = AutoFix::default();
        assert_eq!(state.next(&red("a", &test, 100)), Step::Fix);
        assert_eq!(
            state.next(&red("a", &test, 100 + GIVE_UP_AFTER)),
            Step::GiveUp(GaveUp::Day)
        );
        state.green();
        assert_eq!(state.next(&red("a", &test, 200 + GIVE_UP_AFTER)), Step::Fix);
    }

    #[test]
    fn autofix_9_the_same_failure_is_its_names_and_its_log_but_digits() {
        let test = ["Check".to_owned()];
        let a = failure(&test, "Run 41 at 10:02\nerror: x is 1\n");
        let b = failure(&test, "Run 77 at 11:59\nerror: x is 2\n");
        assert_eq!(a, b, "only digits differ");
        assert_ne!(a, failure(&test, "error: y is missing\n"));
        assert_ne!(
            a,
            failure(&["Lint".to_owned()], "Run 41 at 10:02\nerror: x is 1\n")
        );
    }

    #[test]
    fn autofix_10_one_more_round_gives_three_more() {
        let test = vec!["test".to_owned()];
        let mut state = AutoFix::default();
        state.fixes = FIX_ROUNDS;
        assert!(state.give_up("a", GaveUp::Rounds));
        assert!(!state.give_up("a", GaveUp::Rounds), "said once");
        assert_eq!(
            state.next(&red("a", &test, 0)),
            Step::GiveUp(GaveUp::Rounds)
        );
        state.more();
        assert_eq!(state.allowed(), 2 * FIX_ROUNDS);
        assert_eq!(state.next(&red("a", &test, 0)), Step::Fix);
    }

    #[test]
    fn autofix_12_a_head_goes_back_into_the_queue_three_times() {
        let mut state = AutoFix::default();
        assert!((0..REQUEUES_PER_HEAD).all(|_| state.requeue("a")));
        assert!(!state.requeue("a"));
        assert!(state.requeue("b"), "a new head counts anew");
    }

    #[test]
    fn untold_lines_are_the_ones_after_the_mark() {
        let mut state = AutoFix::default();
        state.say("one");
        state.say("two");
        state.told = 1;
        assert_eq!(state.untold(), ["two"]);
        state.told = 9;
        assert!(state.untold().is_empty());
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of AUTOFIX's bounds: `kani::any()` stands for every read and every count.

    use super::*;

    fn any_facts() -> Facts {
        Facts {
            gave_up_here: None,
            settling: kani::any(),
            a_day: kani::any(),
            fixed_here: kani::any(),
            same_failure: kani::any(),
            infra: kani::any(),
            checked_here: kani::any(),
            reruns_here: kani::any(),
            fixes: kani::any(),
            allowed: kani::any(),
        }
    }

    /// AUTOFIX-9: a fix round starts only within its allowance, never on the head a round left as
    /// it was, after the same failure, or past a red day.
    #[kani::proof]
    fn autofix_9_a_fix_round_only_within_its_bounds() {
        let f = any_facts();
        if decide(f) == Step::Fix {
            assert!(f.fixes < f.allowed);
            assert!(!f.fixed_here && !f.same_failure && !f.a_day && !f.settling);
        }
    }

    /// AUTOFIX-6, 7: a rerun only below its count per head, and a code failure's only once, on a
    /// head the job's own check passed on.
    #[kani::proof]
    fn autofix_6_reruns_stay_within_their_count() {
        let f = any_facts();
        if decide(f) == Step::Rerun {
            assert!(f.reruns_here < RERUNS_PER_HEAD);
            assert!(f.infra || (f.checked_here && f.reruns_here == 0));
        }
    }
}
