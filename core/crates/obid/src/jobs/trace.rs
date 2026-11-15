//! Each job's trace of its dispatcher steps (docs/contracts/failures.md): one JSON line per step
//! in `home/jobs/<id>/trace.ndjson`, holding only a step name, its round, a start, a duration and
//! an outcome code, never any text of the job's. Most lines are worked out at [`Jobs::keep`] from
//! the job before and after a change; at most [`CAP`] are kept.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::job::{Job, Phase, Status};

/// The most lines a trace keeps: the oldest go first.
pub const CAP: usize = 2000;

/// Every write to a trace file, from setup and from `keep`, one at a time.
static WRITING: Mutex<()> = Mutex::new(());

/// A dispatcher step, as the trace names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Admit,
    Prepare,
    Setup,
    Round,
    Check,
    Publish,
    Rebase,
    Stall,
    Retry,
    AccountMove,
    Park,
    RestartRecovery,
    Crash,
}

impl Step {
    /// The steps that last from where the last of them ended: the rest are marks or setup.
    fn spans(self) -> bool {
        matches!(
            self,
            Step::Admit | Step::Prepare | Step::Round | Step::Check | Step::Publish
        )
    }
}

/// How the job stood once the step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Queued,
    Failed,
    Limit,
    Interrupted,
    Stopped,
}

/// One line of a trace: no text of any kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub step: Step,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    #[serde(with = "iso")]
    pub start: i64,
    pub seconds: i64,
    pub outcome: Outcome,
}

/// `start` as an ISO date.
mod iso {
    pub fn serialize<S: serde::Serializer>(at: &i64, to: S) -> Result<S::Ok, S::Error> {
        to.serialize_str(&obi_protocol::time::format(*at))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(from: D) -> Result<i64, D::Error> {
        let text = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(from)?;
        obi_protocol::time::parse(&text).ok_or_else(|| serde::de::Error::custom("not a date"))
    }
}

impl Line {
    /// The line as the file stores it.
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// A stored line, or none for one that doesn't decode.
    pub fn decode(text: &str) -> Option<Line> {
        serde_json::from_str(text).ok()
    }
}

/// The job as the trace sees it.
#[derive(Debug, Clone, Copy)]
pub struct Seen {
    pub status: Status,
    pub phase: Option<Phase>,
    pub round: u32,
    pub errors: usize,
    pub stalls: usize,
    pub moves: usize,
    pub parked: bool,
    pub crashed: bool,
    pub limited: bool,
}

impl Seen {
    pub fn of(job: &Job) -> Seen {
        Seen {
            status: job.status,
            phase: job.phase,
            round: job.current_round(),
            errors: job.recovery.errors,
            stalls: job.recovery.stalls,
            moves: job.moves.len(),
            parked: job.parked.is_some(),
            crashed: job.reason.as_deref().is_some_and(crate::job::obids_doing),
            limited: job.limit.is_some(),
        }
    }
}

/// How many of the oldest lines go before one is added to a file of `lines` lines.
pub fn to_drop(lines: usize) -> usize {
    lines.saturating_sub(CAP - 1)
}

/// The step a phase is.
pub fn phase_step(phase: Phase) -> Step {
    match phase {
        Phase::Prepare => Step::Prepare,
        Phase::Work => Step::Round,
        Phase::Check => Step::Check,
        Phase::Publish => Step::Publish,
    }
}

/// The outcome code of the job as it is after a change.
pub fn outcome(after: Seen) -> Outcome {
    match after.status {
        Status::Running | Status::Done => Outcome::Ok,
        Status::Queued => Outcome::Queued,
        Status::Failed if after.limited => Outcome::Limit,
        Status::Failed => Outcome::Failed,
        Status::Interrupted => Outcome::Interrupted,
        Status::Stopped => Outcome::Stopped,
    }
}

/// The steps a change from `before` to `after` ended, in the order they're written.
pub fn ended(before: Seen, after: Seen) -> [Option<Step>; 7] {
    let left = after.phase != before.phase || after.status != Status::Running;
    [
        before
            .phase
            .filter(|_| before.status == Status::Running && left)
            .map(phase_step),
        (after.stalls > before.stalls).then_some(Step::Stall),
        (after.errors > before.errors).then_some(Step::Retry),
        (after.moves > before.moves).then_some(Step::AccountMove),
        (!before.parked && after.parked).then_some(Step::Park),
        (!before.crashed && after.crashed).then_some(Step::Crash),
        (before.status == Status::Queued && after.status == Status::Running).then_some(Step::Admit),
    ]
}

/// A job changed from `before` (none: a new job) to `after`: the steps it ended are written.
pub fn observe(home: &Path, before: Option<&Job>, after: &Job) {
    let Some(before) = before else {
        return;
    };
    let (before, seen) = (Seen::of(before), Seen::of(after));
    let steps = ended(before, seen);
    if steps.iter().all(Option::is_none) {
        return;
    }
    let (outcome, now) = (outcome(seen), obi_protocol::time::now());
    write(home, &after.id, |kept| {
        let mut from = kept
            .iter()
            .rev()
            .find(|line| line.step.spans())
            .map_or(after.created, |line| line.start + line.seconds);
        let mut lines = Vec::new();
        for step in steps.into_iter().flatten() {
            let line = match step.spans() {
                true => Line {
                    step,
                    round: (step == Step::Round).then_some(before.round),
                    start: from,
                    seconds: (now - from).max(0),
                    outcome,
                },
                false => mark(step, now, outcome),
            };
            if step.spans() {
                from = line.start + line.seconds;
            }
            lines.push(line);
        }
        lines
    });
}

/// A step that happened at `now` and took no time.
fn mark(step: Step, now: i64, outcome: Outcome) -> Line {
    Line {
        step,
        round: None,
        start: now,
        seconds: 0,
        outcome,
    }
}

/// obid started again: a job it was at work on gets its restart recovery, left here or queued.
pub fn restarted(home: &Path, job: &Job, at_work: bool, left_here: bool) {
    if !at_work {
        return;
    }
    let outcome = match left_here {
        true => Outcome::Interrupted,
        false => Outcome::Queued,
    };
    let now = obi_protocol::time::now();
    write(home, &job.id, |_| {
        vec![mark(Step::RestartRecovery, now, outcome)]
    });
}

/// Job `id`'s repository's setup ran from `start` to `end`, well or not.
pub fn setup(home: &Path, id: &str, start: i64, end: i64, ok: bool) {
    let line = Line {
        step: Step::Setup,
        round: None,
        start,
        seconds: (end - start).max(0),
        outcome: if ok { Outcome::Ok } else { Outcome::Failed },
    };
    write(home, id, |_| vec![line]);
}

fn folder(home: &Path, id: &str) -> PathBuf {
    home.join("jobs").join(id)
}

fn path(home: &Path, id: &str) -> PathBuf {
    folder(home, id).join("trace.ndjson")
}

/// The stored lines of job `id`'s trace, none when it has no trace.
fn stored(home: &Path, id: &str) -> std::io::Result<Vec<String>> {
    match std::fs::read_to_string(path(home, id)) {
        Ok(text) => Ok(text.lines().map(str::to_owned).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Adds the lines `make` gives from the trace as it is, keeping at most [`CAP`]. A write that
/// fails is logged and nothing else: the job never hears of it.
fn write(home: &Path, id: &str, make: impl FnOnce(&[Line]) -> Vec<Line>) {
    let _writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = append(home, id, make) {
        tracing::warn!("job {id}: can't write its trace: {e}");
    }
}

fn append(home: &Path, id: &str, make: impl FnOnce(&[Line]) -> Vec<Line>) -> std::io::Result<()> {
    let mut kept = stored(home, id)?;
    let decoded: Vec<Line> = kept.iter().filter_map(|l| Line::decode(l)).collect();
    let added: Vec<String> = make(&decoded).iter().map(Line::encode).collect();
    let mut rewrite = false;
    for line in &added {
        let drop = to_drop(kept.len());
        if drop > 0 {
            kept.drain(..drop);
            rewrite = true;
        }
        kept.push(line.clone());
    }
    crate::store::make_private_dir(&folder(home, id))?;
    if rewrite {
        let text: String = kept.iter().map(|l| format!("{l}\n")).collect();
        return crate::store::write_private(&path(home, id), text.as_bytes());
    }
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path(home, id))?;
    let text: String = added.iter().map(|l| format!("{l}\n")).collect();
    file.write_all(text.as_bytes())
}

/// Job `id`'s trace, in order, without a line that doesn't decode.
pub fn lines(home: &Path, id: &str) -> Vec<Line> {
    stored(home, id)
        .unwrap_or_default()
        .iter()
        .filter_map(|l| Line::decode(l))
        .collect()
}

/// The last `n` lines of job `id`'s trace, as stored.
pub fn last(home: &Path, id: &str, n: usize) -> Vec<String> {
    let mut kept = stored(home, id).unwrap_or_default();
    kept.drain(..kept.len().saturating_sub(n));
    kept
}

/// A deleted job's trace goes with its folder.
pub fn remove(home: &Path, id: &str) {
    let _writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    match std::fs::remove_dir_all(folder(home, id)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            tracing::warn!("job {id}: can't remove its trace: {e}")
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FAILURES-2: past its cap, a trace drops its oldest lines and keeps [`CAP`].
    #[test]
    fn failures_2_the_trace_stays_under_its_cap() {
        let home = std::env::temp_dir().join(format!("obid-trace-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for n in 0..CAP + 5 {
            let line = mark(Step::Stall, n as i64, Outcome::Ok);
            write(&home, "ab12cd34", |_| vec![line]);
        }
        let kept = std::fs::read_to_string(path(&home, "ab12cd34")).unwrap();
        assert_eq!(kept.lines().count(), CAP);
        let first = Line::decode(kept.lines().next().unwrap()).unwrap();
        assert_eq!(first.start, 5, "the 6th line written");
        assert_eq!(lines(&home, "ab12cd34").len(), CAP);
        assert_eq!(last(&home, "ab12cd34", 3).len(), 3);
        remove(&home, "ab12cd34");
        assert!(!folder(&home, "ab12cd34").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_line_holds_its_keys_in_order_and_leaves_out_no_round() {
        let line = Line {
            step: Step::Round,
            round: Some(1),
            start: 0,
            seconds: 12,
            outcome: Outcome::Ok,
        };
        assert_eq!(
            line.encode(),
            r#"{"step":"round","round":1,"start":"1970-01-01T00:00:00Z","seconds":12,"outcome":"ok"}"#
        );
        assert_eq!(Line::decode(&line.encode()), Some(line));
        let mark = mark(Step::AccountMove, 0, Outcome::Queued);
        assert!(!mark.encode().contains("round"), "{}", mark.encode());
        assert!(mark.encode().contains(r#""step":"account_move""#));
        assert_eq!(Line::decode("not json"), None);
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of the trace's cap (FAILURES-2) and of the step a change ends (FAILURES-1):
    //! `kani::any()` stands for every count, and for every value of `Status` and `Phase`.

    use super::*;

    fn any_status() -> Status {
        match kani::any::<u8>() {
            0 => Status::Queued,
            1 => Status::Running,
            2 => Status::Done,
            3 => Status::Failed,
            4 => Status::Stopped,
            _ => Status::Interrupted,
        }
    }

    fn any_phase() -> Option<Phase> {
        match kani::any::<u8>() {
            0 => None,
            1 => Some(Phase::Prepare),
            2 => Some(Phase::Work),
            3 => Some(Phase::Check),
            _ => Some(Phase::Publish),
        }
    }

    fn any_seen() -> Seen {
        Seen {
            status: any_status(),
            phase: any_phase(),
            round: kani::any(),
            errors: kani::any(),
            stalls: kani::any(),
            moves: kani::any(),
            parked: kani::any(),
            crashed: kani::any(),
            limited: kani::any(),
        }
    }

    /// FAILURES-2: dropping `to_drop(lines)` leaves room for one more line within the cap, drops
    /// no more lines than there are, and drops nothing under the cap.
    #[kani::proof]
    fn failures_2_the_trace_never_passes_its_cap() {
        let lines: usize = kani::any();
        let drop = to_drop(lines);
        assert!(drop <= lines);
        assert!(lines - drop < CAP);
        if lines < CAP {
            assert_eq!(drop, 0);
        }
    }

    /// FAILURES-1: a job that left its work phase ends a round first; a queued job that starts
    /// running is admitted last, with no phase line.
    #[kani::proof]
    fn failures_1_a_phase_left_writes_its_own_step() {
        let (before, after) = (any_seen(), any_seen());
        let steps = ended(before, after);
        if before.status == Status::Running
            && before.phase == Some(Phase::Work)
            && (after.phase != before.phase || after.status != Status::Running)
        {
            assert_eq!(steps[0], Some(Step::Round));
        }
        if before.status == Status::Queued && after.status == Status::Running {
            assert_eq!(steps[6], Some(Step::Admit));
            assert_eq!(steps[0], None);
        }
    }
}
