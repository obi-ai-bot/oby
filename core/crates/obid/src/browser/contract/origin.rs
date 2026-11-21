//! Who a job's browser works for (BROWSER-18), as obid's own records say: the owner's own work, a
//! job marked `untrusted`, or another Oby's task, named by the Obi that asked and its owner (the
//! exchange that brought it, `Job::obi`), never by anything its request says. A wire type: it owns
//! no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Who a job's browser works for. `kind` only grows (`own`, `untrusted`, `obi`): a client treats
/// one it doesn't know as `obi`, the farthest from the owner.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Origin {
    /// The owner's own work.
    #[default]
    Own,
    /// The owner's job marked `untrusted` (it read something from outside), or Oby's own review.
    Untrusted,
    /// Another Oby's task: the Obi that asked (`peer`) and its owner (`peerOwner`).
    Obi {
        peer: String,
        #[serde(rename = "peerOwner")]
        peer_owner: String,
    },
}

impl Origin {
    /// Whether its browser is in the foreign lane: Oby's box, apart from the owner's own.
    pub fn foreign(&self) -> bool {
        *self != Origin::Own
    }

    /// The webview partition a job's tabs open in (the window takes no other shape): the owner's
    /// own work in its agent's, kept, so a site signed in to for that agent stays signed in for its
    /// next job; anything else (another Oby's task, an `untrusted` job, Oby's own review) in a box
    /// partition of the job's own, not kept, dropped with the job.
    pub fn partition(&self, job: &str, agent: &str) -> String {
        if self.foreign() {
            format!("oby-box-{job}")
        } else {
            format!("persist:oby-agent-{agent}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_18_another_obys_task_is_named_by_its_peer_and_peer_owner_on_the_wire() {
        let o = Origin::Obi {
            peer: "obi-b".into(),
            peer_owner: "ben".into(),
        };
        assert_eq!(
            serde_json::to_value(&o).unwrap(),
            serde_json::json!({"kind": "obi", "peer": "obi-b", "peerOwner": "ben"})
        );
        assert_eq!(
            serde_json::to_value(Origin::Own).unwrap(),
            serde_json::json!({"kind": "own"})
        );
        assert!(o.foreign() && Origin::Untrusted.foreign() && !Origin::Own.foreign());
        assert_eq!(o.partition("j1", "codex"), "oby-box-j1");
        assert_eq!(Origin::Untrusted.partition("j1", "codex"), "oby-box-j1");
        assert_eq!(
            Origin::Own.partition("j1", "codex"),
            "persist:oby-agent-codex"
        );
    }
}
