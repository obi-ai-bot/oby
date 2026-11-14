//! Where the owner's words in a conversation stand (docs/contracts/deliveries.md), and the only
//! moves between those states.

use serde::{Deserialize, Serialize};

/// `status` is an open string: obid writes `queued`, `starting`, `running`, `answered`, `failed`,
/// `interrupted` and `cancelled`, and one it doesn't know (a newer obid's) never moves. `by`: the
/// user's turn whose turn carries the words; `why`: why they went back to `queued`; `error`: why
/// they failed, with every secret obid knows taken out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The moves: start (`queued` → `starting`), confirm (`starting` → `running`, `by`), restore
/// (`starting` → `queued`, `why`), finish (`running` → `answered`, `failed` with its `error`, or
/// `interrupted`) and cancel (`queued` or `running` → `cancelled`).
#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    Start,
    Confirm(String),
    Restore(String),
    Answered,
    Failed(String),
    Interrupted,
    Cancel,
}

/// What queued words keep to be sent again, on the conversation's file only: their attachments'
/// paths, and whether an app of the owner's signed them (none reads as unsigned: TASKCONV-6).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Replay {
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default)]
    pub signed: bool,
}

impl Delivery {
    /// Makes `change` when it's a move from this status: whether it did. A refused one changes
    /// nothing.
    pub fn apply(&mut self, change: &Move) -> bool {
        let (status, set) = match (self.status.as_str(), change) {
            ("queued", Move::Start) => ("starting", None),
            ("starting", Move::Confirm(by)) => ("running", Some((&mut self.by, by))),
            ("starting", Move::Restore(why)) => ("queued", Some((&mut self.why, why))),
            ("running", Move::Answered) => ("answered", None),
            ("running", Move::Failed(error)) => ("failed", Some((&mut self.error, error))),
            ("running", Move::Interrupted) => ("interrupted", None),
            ("queued" | "running", Move::Cancel) => ("cancelled", None),
            _ => return false,
        };
        if let Some((field, value)) = set {
            *field = Some(value.clone());
        }
        self.status = status.into();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moves() -> Vec<Move> {
        vec![
            Move::Start,
            Move::Confirm("t2".into()),
            Move::Restore("obid restarted".into()),
            Move::Answered,
            Move::Failed("no session".into()),
            Move::Interrupted,
            Move::Cancel,
        ]
    }

    fn at(status: &str) -> Delivery {
        Delivery {
            status: status.into(),
            ..Delivery::default()
        }
    }

    /// DELIVERY-6: a finished delivery, or one whose status obid doesn't know, never moves; each
    /// allowed move gives its status and what it sets, and nothing else moves.
    #[test]
    fn delivery_6_a_terminal_delivery_never_moves() {
        for status in [
            "answered",
            "cancelled",
            "failed",
            "interrupted",
            "delivered-later",
        ] {
            for change in moves() {
                let mut delivery = at(status);
                assert!(!delivery.apply(&change), "{status}: {change:?}");
                assert_eq!(delivery, at(status), "{status}: {change:?}");
            }
        }
        let allowed = |status: &str, change: &Move| -> Option<Delivery> {
            let mut delivery = at(status);
            let moved = delivery.apply(change);
            assert_eq!(moved, delivery != at(status), "{status}: {change:?}");
            moved.then_some(delivery)
        };
        for status in ["queued", "starting", "running"] {
            for change in moves() {
                let moved = allowed(status, &change);
                let expected = match (status, &change) {
                    ("queued", Move::Start) => Some(at("starting")),
                    ("starting", Move::Confirm(by)) => Some(Delivery {
                        by: Some(by.clone()),
                        ..at("running")
                    }),
                    ("starting", Move::Restore(why)) => Some(Delivery {
                        why: Some(why.clone()),
                        ..at("queued")
                    }),
                    ("running", Move::Answered) => Some(at("answered")),
                    ("running", Move::Failed(error)) => Some(Delivery {
                        error: Some(error.clone()),
                        ..at("failed")
                    }),
                    ("running", Move::Interrupted) => Some(at("interrupted")),
                    ("queued" | "running", Move::Cancel) => Some(at("cancelled")),
                    _ => None,
                };
                assert_eq!(moved, expected, "{status}: {change:?}");
            }
        }
        let unknown = serde_json::json!({"status": "delivered-later", "by": "t1"});
        let read: Delivery = serde_json::from_value(unknown.clone()).unwrap();
        assert_eq!(serde_json::to_value(&read).unwrap(), unknown);
        assert_eq!(
            serde_json::to_value(at("queued")).unwrap(),
            serde_json::json!({"status": "queued"})
        );
    }
}
