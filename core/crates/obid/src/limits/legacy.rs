//! What an older release left at a limit (LIMITS-13, LIMITS-26): its #1563 decisions in
//! `home/limit-decisions.json` and the jobs they stopped, each settled once when obid loads them
//! (`Jobs::settle_older_limits`). This file reads them and decides; it writes nothing.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// An older release's answer to a #1563 card, as job files and the API's `accountLimit` keep it:
/// pending was its question, the others the owner's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Pending,
    Resume,
    Stop,
    Move,
}

/// What obid reads of one job an older release left (LIMITS-13, LIMITS-26).
#[derive(Clone, Copy, Debug)]
pub struct Left {
    pub interrupted: bool,
    pub blocked: bool,
    pub dismissed: bool,
    pub older_wait: bool,
    pub followed: bool,
    pub choice: Option<Choice>,
}

/// What becomes of it when obid loads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upgrade {
    Untouched,
    ForgetWait,
    KeepStopped,
    Settle,
}

/// A kept Stop stays stopped; a wait the owner dismissed, or a follow-up took over, is forgotten;
/// every other job a limit left moves or waits as LIMITS-15 says (`Settle`).
pub fn upgrade(left: Left) -> Upgrade {
    if !left.interrupted {
        return Upgrade::Untouched;
    }
    if left.blocked && !left.dismissed && left.choice == Some(Choice::Stop) {
        return Upgrade::KeepStopped;
    }
    if left.older_wait && (left.dismissed || (left.followed && !left.blocked)) {
        return Upgrade::ForgetWait;
    }
    if !left.dismissed && (left.blocked || left.older_wait) {
        return Upgrade::Settle;
    }
    Upgrade::Untouched
}

/// One decision of the older file, as much of it as settling reads; serde skips the rest.
#[derive(Deserialize)]
struct Decision {
    id: String,
    jobs: Vec<String>,
    choice: Choice,
    #[serde(default)]
    applied: Vec<String>,
}

/// The older release's decisions, as its file had them.
pub struct Decisions(Vec<Decision>);

impl Decisions {
    /// Where an older release kept them.
    pub fn path(home: &Path) -> PathBuf {
        home.join("limit-decisions.json")
    }

    /// The file at `path`: `None` when there's none, an error when it can't be read or parsed.
    pub fn read(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(|kept| Some(Self(kept)))
                .map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// The decision job `id` still waited on, in its `episode` when it has one: its id and choice.
    pub fn of(&self, id: &str, episode: Option<&str>) -> Option<(&str, Choice)> {
        (self.0.iter())
            .find(|d| {
                d.jobs.iter().any(|j| j == id)
                    && !d.applied.iter().any(|j| j == id)
                    && episode.is_none_or(|e| e == d.id)
            })
            .map(|d| (d.id.as_str(), d.choice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left(blocked: bool, dismissed: bool, older_wait: bool, followed: bool) -> Left {
        Left {
            interrupted: true,
            blocked,
            dismissed,
            older_wait,
            followed,
            choice: None,
        }
    }

    /// LIMITS-26: each case Requirement 5 names, and a job no limit left.
    #[test]
    fn limits_26_each_job_an_older_release_left_gets_its_upgrade() {
        let stop = Left {
            choice: Some(Choice::Stop),
            ..left(true, false, false, false)
        };
        let cases = [
            (left(true, false, false, false), Upgrade::Settle),
            (left(false, false, true, false), Upgrade::Settle),
            (left(true, false, true, true), Upgrade::Settle),
            (stop, Upgrade::KeepStopped),
            (
                Left {
                    dismissed: true,
                    ..stop
                },
                Upgrade::Untouched,
            ),
            (left(false, true, true, false), Upgrade::ForgetWait),
            (left(false, false, true, true), Upgrade::ForgetWait),
            (left(true, true, false, false), Upgrade::Untouched),
            (left(false, false, false, false), Upgrade::Untouched),
            (
                Left {
                    interrupted: false,
                    ..stop
                },
                Upgrade::Untouched,
            ),
        ];
        for (left, want) in cases {
            assert_eq!(upgrade(left), want, "{left:?}");
        }
        for choice in [Choice::Pending, Choice::Resume, Choice::Move] {
            let left = Left {
                choice: Some(choice),
                ..stop
            };
            assert_eq!(upgrade(left), Upgrade::Settle, "{left:?}");
        }
    }

    /// LIMITS-26: a job's decision is the one that names it, not applied to it, in its episode.
    #[test]
    fn limits_26_a_job_reads_the_decision_it_waited_on() {
        let kept: Vec<Decision> = serde_json::from_value(serde_json::json!([
            {"id": "d1", "jobs": ["a", "b"], "choice": "stop", "applied": ["b"], "revision": 3},
            {"id": "d2", "jobs": ["b", "c"], "choice": "resume"},
        ]))
        .unwrap();
        let decisions = Decisions(kept);
        assert_eq!(decisions.of("a", None), Some(("d1", Choice::Stop)));
        assert_eq!(decisions.of("b", None), Some(("d2", Choice::Resume)));
        assert_eq!(decisions.of("c", Some("d1")), None);
        assert_eq!(decisions.of("c", Some("d2")), Some(("d2", Choice::Resume)));
        assert_eq!(decisions.of("e", None), None);
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of LIMITS-26: `kani::any()` stands for every job an older release could leave.

    use super::*;

    /// Any old choice, or none, from a number.
    fn any_choice() -> Option<Choice> {
        match kani::any::<u8>() {
            0 => None,
            1 => Some(Choice::Pending),
            2 => Some(Choice::Resume),
            3 => Some(Choice::Stop),
            _ => Some(Choice::Move),
        }
    }

    fn any_left() -> Left {
        Left {
            interrupted: kani::any(),
            blocked: kani::any(),
            dismissed: kani::any(),
            older_wait: kani::any(),
            followed: kani::any(),
            choice: any_choice(),
        }
    }

    /// Every job a limit left interrupted and not dismissed, blocked or on an older wait no
    /// follow-up took over, settles or stays stopped by its kept Stop: never untouched or only
    /// forgotten.
    #[kani::proof]
    fn limits_26_no_job_a_limit_left_is_dropped() {
        let left = any_left();
        if left.interrupted
            && !left.dismissed
            && (left.blocked || (left.older_wait && !left.followed))
        {
            assert!(matches!(
                upgrade(left),
                Upgrade::Settle | Upgrade::KeepStopped
            ));
        }
    }

    /// `KeepStopped` exactly for an interrupted, blocked job not dismissed whose old choice is Stop.
    #[kani::proof]
    fn limits_26_only_a_kept_stop_stays_stopped() {
        let left = any_left();
        let kept = left.interrupted
            && left.blocked
            && !left.dismissed
            && left.choice == Some(Choice::Stop);
        assert!((upgrade(left) == Upgrade::KeepStopped) == kept);
    }

    /// A job that isn't interrupted is always untouched, whatever its marks and old choice say.
    #[kani::proof]
    fn limits_26_a_job_not_interrupted_is_untouched() {
        let left = Left {
            interrupted: false,
            ..any_left()
        };
        assert!(upgrade(left) == Upgrade::Untouched);
    }
}
