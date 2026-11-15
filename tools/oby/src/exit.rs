//! How a run ends: one exit code a script can trust (docs/contracts/cli.md, "States and exit codes").

/// A job's status, as obid names it. A status obid adds later is `Unknown`, and still going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Queued,
    Running,
    Done,
    Failed,
    Stopped,
    Interrupted,
    Unknown,
}

impl Status {
    pub fn named(name: &str) -> Status {
        match name {
            "queued" => Status::Queued,
            "running" => Status::Running,
            "done" => Status::Done,
            "failed" => Status::Failed,
            "stopped" => Status::Stopped,
            "interrupted" => Status::Interrupted,
            _ => Status::Unknown,
        }
    }
}

/// Whether the job waits for its owner: an open inbox item of theirs, or a pending limit decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asks {
    Nobody,
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Done = 0,
    Failed = 1,
    Usage = 2,
    Owner = 3,
    TimedOut = 4,
    Unreachable = 5,
}

/// The exit code a wait ends with, or `None` to keep waiting. An ended job wins over an ask and a
/// timeout: a script learns how the job ended whenever it can.
pub fn exit_for(status: Status, asks: Asks, timed_out: bool) -> Option<Exit> {
    match status {
        Status::Done => Some(Exit::Done),
        Status::Failed | Status::Stopped | Status::Interrupted => Some(Exit::Failed),
        Status::Queued | Status::Running | Status::Unknown => match (asks, timed_out) {
            (Asks::Owner, _) => Some(Exit::Owner),
            (Asks::Nobody, true) => Some(Exit::TimedOut),
            (Asks::Nobody, false) => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_for_every_input() {
        use Asks::{Nobody, Owner};
        use Status::*;
        let table = [
            (Queued, Nobody, false, None),
            (Queued, Nobody, true, Some(Exit::TimedOut)),
            (Queued, Owner, false, Some(Exit::Owner)),
            (Queued, Owner, true, Some(Exit::Owner)),
            (Running, Nobody, false, None),
            (Running, Nobody, true, Some(Exit::TimedOut)),
            (Running, Owner, false, Some(Exit::Owner)),
            (Running, Owner, true, Some(Exit::Owner)),
            (Done, Nobody, false, Some(Exit::Done)),
            (Done, Nobody, true, Some(Exit::Done)),
            (Done, Owner, false, Some(Exit::Done)),
            (Done, Owner, true, Some(Exit::Done)),
            (Failed, Nobody, false, Some(Exit::Failed)),
            (Failed, Nobody, true, Some(Exit::Failed)),
            (Failed, Owner, false, Some(Exit::Failed)),
            (Failed, Owner, true, Some(Exit::Failed)),
            (Stopped, Nobody, false, Some(Exit::Failed)),
            (Stopped, Nobody, true, Some(Exit::Failed)),
            (Stopped, Owner, false, Some(Exit::Failed)),
            (Stopped, Owner, true, Some(Exit::Failed)),
            (Interrupted, Nobody, false, Some(Exit::Failed)),
            (Interrupted, Nobody, true, Some(Exit::Failed)),
            (Interrupted, Owner, false, Some(Exit::Failed)),
            (Interrupted, Owner, true, Some(Exit::Failed)),
            (Unknown, Nobody, false, None),
            (Unknown, Nobody, true, Some(Exit::TimedOut)),
            (Unknown, Owner, false, Some(Exit::Owner)),
            (Unknown, Owner, true, Some(Exit::Owner)),
        ];
        assert_eq!(table.len(), 7 * 2 * 2);
        for (status, asks, timed_out, wanted) in table {
            assert_eq!(
                exit_for(status, asks, timed_out),
                wanted,
                "{status:?} {asks:?} {timed_out}"
            );
        }
        let codes = [
            Exit::Done,
            Exit::Failed,
            Exit::Usage,
            Exit::Owner,
            Exit::TimedOut,
            Exit::Unreachable,
        ];
        assert_eq!(codes.map(|e| e as i32), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_status_obid_adds_later_is_still_going() {
        assert_eq!(Status::named("done"), Status::Done);
        assert_eq!(Status::named("interrupted"), Status::Interrupted);
        assert_eq!(Status::named("publishing"), Status::Unknown);
    }
}
