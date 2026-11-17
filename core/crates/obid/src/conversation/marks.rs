//! A conversation's two marks for the owner's list (docs/contracts/conversation-states.md,
//! CHATSTATE-6 to 8 and 10): when the owner last looked at it or spoke in it (`seen`), so a turn
//! since reads as unread, and whether they put it away (`archived`).
//!
//! It owns only those two fields of a [`super::Thread`]: it never writes a turn, the title, the
//! order (`sequence`) or anything an agent reads. Unread is a rule of obid's, not the app's: the
//! app shows `unread` as obid sends it.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// When the owner last looked: the time, for clients to show, and the conversation's `sequence`
/// then, which says exactly whether a turn came after (two turns can share a second).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Seen {
    pub at: String,
    pub sequence: u64,
}

/// What `PATCH /v1/conversations/{id}` asks of the marks: `seen` true (looked at it now) or false
/// (Mark unread), `archived` true or false. `None` leaves a mark as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Marks {
    pub seen: Option<bool>,
    pub archived: Option<bool>,
}

impl Marks {
    pub fn any(self) -> bool {
        self.seen.is_some() || self.archived.is_some()
    }
}

impl super::Thread {
    /// Seen as it is now (`at`): every turn it has so far is read.
    pub(super) fn seen_at(&mut self, at: String) {
        self.seen = Some(Seen {
            at,
            sequence: self.sequence,
        });
    }

    /// The owner's own words in it: they've seen it, and it's in use again, out of Archived.
    pub(super) fn spoke(&mut self, at: String) {
        self.seen_at(at);
        self.archived = false;
    }

    /// The marks asked (`now`: the time to keep for seen). Mark unread keeps when they last looked
    /// and reads its latest turn as new again; with no turn after its making, there's nothing to be
    /// unread, and it stays read.
    pub(super) fn mark(&mut self, marks: Marks, now: String) {
        match marks.seen {
            Some(true) => self.seen_at(now),
            Some(false) => {
                let at = self
                    .seen
                    .as_ref()
                    .map_or_else(|| self.created.clone(), |s| s.at.clone());
                self.seen = Some(Seen {
                    at,
                    sequence: self.sequence.saturating_sub(1),
                });
            }
            None => {}
        }
        if let Some(archived) = marks.archived {
            self.archived = archived;
        }
    }

    /// A turn came after the owner last looked. One kept before obid had marks (no `seen`) reads
    /// as read until it's looked at once.
    pub(super) fn unread(&self) -> bool {
        self.seen
            .as_ref()
            .is_some_and(|s| s.sequence < self.sequence)
    }

    /// Its marks in its summary, each only once there's something to say (add-only keys).
    pub(super) fn show_marks(&self, summary: &mut Value) {
        if let Some(seen) = &self.seen {
            summary["seen"] = json!(seen.at);
        }
        if self.unread() {
            summary["unread"] = json!(true);
        }
        if self.archived {
            summary["archived"] = json!(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Marks;
    use crate::conversation::Conversations;
    use crate::events::Events;
    use serde_json::Value;

    fn conversations(name: &str) -> (Conversations, std::path::PathBuf) {
        let home = std::env::temp_dir().join(format!("obid-marks-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        (Conversations::open(&home, Events::new()), home)
    }

    fn marked(marks: Marks) -> crate::conversation::Edit {
        crate::conversation::Edit {
            marks,
            ..Default::default()
        }
    }

    fn summary(c: &Conversations, id: &str) -> Value {
        c.lock()
            .iter()
            .find(|t| t.id == id)
            .map(super::super::Thread::summary)
            .unwrap()
    }

    const SEEN: Marks = Marks {
        seen: Some(true),
        archived: None,
    };
    const UNSEEN: Marks = Marks {
        seen: Some(false),
        archived: None,
    };

    /// CHATSTATE-6: a new conversation starts seen; Oby's turn after it is unread; the owner's own
    /// words, or looking at it (`seen` true), read it all.
    #[test]
    fn chatstate_6_a_turn_after_the_owner_looked_is_unread() {
        let (c, home) = conversations("6");
        let id = c.start_for("Fix the build").unwrap();
        let fresh = summary(&c, &id);
        assert!(fresh["seen"].is_string(), "made seen: {fresh}");
        assert_eq!(fresh["unread"], Value::Null);
        c.append(&id, "oby", "Done.".into(), vec![], None).unwrap();
        assert_eq!(summary(&c, &id)["unread"], true);
        c.append(&id, "user", "Thanks".into(), vec![], None)
            .unwrap();
        assert_eq!(
            summary(&c, &id)["unread"],
            Value::Null,
            "their own words read it"
        );
        c.append(&id, "oby", "Anything else?".into(), vec![], None)
            .unwrap();
        assert_eq!(summary(&c, &id)["unread"], true);
        let looked = c.change(&id, marked(SEEN)).unwrap();
        assert_eq!(looked["unread"], Value::Null);
        assert_eq!(summary(&c, &id)["unread"], Value::Null, "kept");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// CHATSTATE-6: one kept before obid had marks has no `seen` and is never unread until looked at.
    #[test]
    fn chatstate_6_one_kept_before_marks_reads_as_read() {
        let (c, home) = conversations("6-kept");
        let id = c.start_for("Old").unwrap();
        c.append(&id, "oby", "Answer".into(), vec![], None).unwrap();
        drop(c);
        let file = home.join("conversations").join(format!("{id}.json"));
        let mut kept: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        kept.as_object_mut().unwrap().remove("seen");
        std::fs::write(&file, kept.to_string()).unwrap();
        let c = Conversations::open(&home, Events::new());
        let old = summary(&c, &id);
        assert_eq!(
            (old["seen"].clone(), old["unread"].clone()),
            (Value::Null, Value::Null)
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// CHATSTATE-7: Mark unread (`seen` false) makes its latest turn new again, until looked at;
    /// the order of the list doesn't change.
    #[test]
    fn chatstate_7_mark_unread_until_seen_again() {
        let (c, home) = conversations("7");
        let id = c.start_for("Plan").unwrap();
        c.append(&id, "user", "Plan it".into(), vec![], None)
            .unwrap();
        let before = summary(&c, &id);
        let unseen = c.change(&id, marked(UNSEEN)).unwrap();
        assert_eq!(unseen["unread"], true);
        assert_eq!(unseen["cursor"], before["cursor"], "same place in the list");
        assert_eq!(
            unseen["seen"], before["seen"],
            "when they last looked stays"
        );
        assert_eq!(c.change(&id, marked(SEEN)).unwrap()["unread"], Value::Null);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// CHATSTATE-10: archived and restored by the owner; their own words in it bring it back.
    #[test]
    fn chatstate_10_archived_until_restored_or_spoken_in() {
        let (c, home) = conversations("10");
        let id = c.start_for("Old work").unwrap();
        let archive = Marks {
            seen: None,
            archived: Some(true),
        };
        assert_eq!(c.change(&id, marked(archive)).unwrap()["archived"], true);
        assert_eq!(summary(&c, &id)["archived"], true);
        c.append(&id, "oby", "A task ended.".into(), vec![], None)
            .unwrap();
        assert_eq!(
            summary(&c, &id)["archived"],
            true,
            "Oby's turn leaves it archived"
        );
        c.append(&id, "user", "Again".into(), vec![], None).unwrap();
        assert_eq!(
            summary(&c, &id)["archived"],
            Value::Null,
            "their words bring it back"
        );
        c.change(&id, marked(archive)).unwrap();
        let restore = Marks {
            seen: None,
            archived: Some(false),
        };
        assert_eq!(
            c.change(&id, marked(restore)).unwrap()["archived"],
            Value::Null
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
