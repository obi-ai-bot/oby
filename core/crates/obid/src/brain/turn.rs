//! The turns under way, as Stop and cancel reach them: one answer at a time in a conversation, the
//! words said meanwhile answered together by the next (crate::brain::drain, CONVERSATION-AGENT-49),
//! and why each turn ended (#538). The turn itself runs on the conversation harness
//! (super::harness).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::actions::Talk;

/// What a turn says when it ran out of time.
pub const AS_FAR_AS: &str = "That's as far as Oby got in one go: say “continue” for the rest.";

/// A turn under way, as Stop and cancel reach it.
#[derive(Debug)]
pub struct Control {
    /// The user's turn: what its step events name.
    pub turn: String,
    pub conversation: String,
    /// Stop (`POST /v1/conversation/stop`, or a bare "stop"): finish the action under way, then
    /// answer with what was done.
    stopped: AtomicBool,
    /// Cancel (`POST /v1/plans/cancel`, or the owner's words where obid's own turn is under
    /// way): nothing more happens for it, and its answer is `none`.
    cancelled: AtomicBool,
    /// The owner's words this turn answers were signed by an app of theirs ([`Words::signed`]).
    signed: AtomicBool,
    /// When it began: an inbox item made since is one the owner hasn't seen yet.
    pub begun: i64,
}

impl Control {
    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Whether what the brain plans may still happen.
    pub fn live(&self) -> bool {
        !self.stopped() && !self.cancelled()
    }

    /// Whether the owner's words this turn answers were signed by an app of theirs.
    pub fn signed(&self) -> bool {
        self.signed.load(Ordering::SeqCst)
    }

    pub fn set_signed(&self, signed: bool) {
        self.signed.store(signed, Ordering::SeqCst);
    }
}

/// The owner's words in a conversation, kept until a turn answers them.
#[derive(Debug, Clone)]
pub struct Words {
    /// The user's turn they are.
    pub turn: String,
    pub said: String,
    pub attachments: Vec<String>,
    /// The jobs the owner attached (`POST /v1/plans`'s `jobs`).
    pub jobs: Vec<String>,
    /// An app of the owner's signed them (docs/contracts/signed-words.md): only then are they the
    /// owner's own word for what Oby does at once (TASKCONV-6).
    pub signed: bool,
    /// The message box's chips sent with them (brain::defaults, COMPOSER-3).
    pub defaults: Option<super::defaults::Defaults>,
}

impl Words {
    /// The owner's words `taken` (oldest first, each with what the file keeps to answer it) as
    /// one turn answers them: their words one after another, every attachment and job, the
    /// latest's user turn and chips (COMPOSER-7), and the owner's own only when each was signed.
    pub fn together(
        taken: &[(crate::conversation::Turn, crate::delivery::Replay)],
        defaults: Option<super::defaults::Defaults>,
    ) -> Option<Words> {
        let (last, _) = taken.last()?;
        Some(Words {
            turn: last.id.clone(),
            said: (taken.iter())
                .map(|(t, _)| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
            attachments: (taken.iter())
                .flat_map(|(_, r)| r.attachments.clone())
                .collect(),
            jobs: taken.iter().flat_map(|(t, _)| t.jobs.clone()).collect(),
            signed: taken.iter().all(|(_, r)| r.signed),
            defaults,
        })
    }
}

/// What a request for the owner's words asked with that obid doesn't keep on the conversation's
/// file (CONVERSATION-AGENT-49): the message box's chips, the turns its client sent, since when the
/// owner was away (a recap, #594) and when. The turn that takes the words takes it; words a
/// restart left queued have none.
#[derive(Debug, Clone, Default)]
pub struct Asking {
    pub defaults: Option<super::defaults::Defaults>,
    pub turns: Option<Vec<crate::conversation::Turn>>,
    pub back: Option<i64>,
    pub now: i64,
}

/// What a request for the owner's words hears ([`Turns::ask`]): the answer's body, or why none
/// came.
#[derive(Debug, Clone)]
pub enum Heard {
    Answer(serde_json::Value),
    Failed(axum::http::StatusCode, String),
}

/// The turns under way, and each conversation's one drain (CONVERSATION-AGENT-49, R05-04): the
/// owner's waiting words are the conversation's `queued` deliveries on disk, and nowhere else.
#[derive(Debug, Default)]
pub struct Turns {
    underway: Mutex<Vec<Arc<Control>>>,
    /// The conversations whose drain runs (crate::brain::drain): one at a time in each.
    draining: Mutex<HashSet<String>>,
    /// What each request asked with, by the user's turn its words are, until a turn takes them.
    asking: Mutex<HashMap<String, Asking>>,
    /// Where each request waiting for its words' answer hears it, by their user's turn.
    heard: Mutex<HashMap<String, tokio::sync::oneshot::Sender<Heard>>>,
}

fn held<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Turns {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Arc<Control>>> {
        held(&self.underway)
    }

    /// The request for `turn`'s words, which asked with `asking`, waits for their answer: it hears
    /// it, or `none` when a later turn's words carried them or a cancel dropped them.
    pub fn ask(&self, turn: &str, asking: Asking) -> tokio::sync::oneshot::Receiver<Heard> {
        let (tell, hear) = tokio::sync::oneshot::channel();
        held(&self.asking).insert(turn.to_owned(), asking);
        held(&self.heard).insert(turn.to_owned(), tell);
        hear
    }

    /// What the request for `turn`'s words asked with, taken once.
    pub fn take_asking(&self, turn: &str) -> Option<Asking> {
        held(&self.asking).remove(turn)
    }

    /// The request for `turn`'s words, if one waits, hears `heard`.
    pub fn tell(&self, turn: &str, heard: Heard) {
        if let Some(tell) = held(&self.heard).remove(turn) {
            let _ = tell.send(heard);
        }
    }

    /// `conversation`'s drain begins, unless one runs there already: whether it does.
    pub fn start_draining(&self, conversation: &str) -> bool {
        held(&self.draining).insert(conversation.to_owned())
    }

    /// `conversation`'s drain ends when `empty` says nothing waits there, judged under the lock
    /// [`Turns::start_draining`] takes: words kept before a drain was asked for are always taken.
    /// Whether it ended.
    pub fn done_draining(&self, conversation: &str, empty: impl FnOnce() -> bool) -> bool {
        let mut draining = held(&self.draining);
        let done = empty();
        if done {
            draining.remove(conversation);
        }
        done
    }

    /// A drain that ended without [`Turns::done_draining`] (it panicked): `conversation` may have
    /// another.
    pub fn abandon_draining(&self, conversation: &str) {
        held(&self.draining).remove(conversation);
    }

    /// A turn begins in `conversation`: its drain runs one at a time there.
    pub fn begin(&self, turn: &str, conversation: &str) -> Arc<Control> {
        let control = Arc::new(Control {
            turn: turn.into(),
            conversation: conversation.into(),
            stopped: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            signed: AtomicBool::new(false),
            begun: obi_protocol::time::now(),
        });
        self.lock().push(control.clone());
        control
    }

    pub fn end(&self, control: &Arc<Control>) {
        self.lock().retain(|c| !Arc::ptr_eq(c, control));
    }

    /// Asks the turns under way (in `conversation`, or all) to stop after the action under way.
    /// True when there was one.
    pub fn stop(&self, conversation: Option<&str>) -> bool {
        self.mark(conversation, |c| c.stopped.store(true, Ordering::SeqCst))
    }

    /// Drops the turns under way (in `conversation`, or all): their plans do nothing. The words
    /// waiting there are the conversation's to cancel (crate::brain::cancel_waiting).
    pub fn cancel(&self, conversation: Option<&str>) {
        self.mark(conversation, |c| c.cancelled.store(true, Ordering::SeqCst));
    }

    fn mark(&self, conversation: Option<&str>, mark: impl Fn(&Control)) -> bool {
        let underway = self.lock();
        let mut any = false;
        for control in underway
            .iter()
            .filter(|c| conversation.is_none_or(|id| c.conversation == id))
        {
            mark(control);
            any = true;
        }
        any
    }

    /// Whether a turn is under way in `conversation`.
    pub fn underway_in(&self, conversation: &str) -> bool {
        self.lock().iter().any(|c| c.conversation == conversation)
    }

    /// The conversation of the latest turn under way.
    pub fn latest_conversation(&self) -> Option<String> {
        self.lock().last().map(|c| c.conversation.clone())
    }
}

/// Words that end the turn under way instead of starting one: "never mind", "stop", "cancel",
/// "forget it", with any punctuation after them (`Conversation.cancels`). They never reach the
/// brain, so no job stops on a bare word: "stop the vault job" or "stop everything" do that.
pub fn cancels(said: &str) -> bool {
    [
        "never mind",
        "nevermind",
        "stop",
        "cancel",
        "cancel that",
        "forget it",
        "no wait",
        "wait",
        "nothing",
        "leave it",
    ]
    .contains(&plain(said).as_str())
}

/// `said` in lower-case words, with no punctuation.
fn plain(said: &str) -> String {
    let cleaned: String = said
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c == '\'' || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Why a turn ended (#538), as Claude Code's loop records why it stopped: the turn and its tests
/// go by this, never by the words the user hears, which a clearer message or a translation can
/// change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    /// Oby answered.
    Answered,
    /// It started these jobs, by id.
    StartedJobs(Vec<String>),
    /// The turn's budget ran out.
    BudgetSpent,
    /// Stop: the action under way finished, and nothing more started.
    Stopped,
    /// Cancelled: nothing more happens, and the answer is none.
    Cancelled,
    /// No agent session could answer.
    BrainFailed,
}

impl End {
    pub fn as_str(&self) -> &'static str {
        match self {
            End::Answered => "answered",
            End::StartedJobs(_) => "started_jobs",
            End::BudgetSpent => "budget_spent",
            End::Stopped => "stopped",
            End::Cancelled => "cancelled",
            End::BrainFailed => "brain_failed",
        }
    }
}

/// A step's state, as its `step` event says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    Failed,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Running => "running",
            State::Done => "done",
            State::Failed => "failed",
        }
    }
}

/// What a turn answers, the longer Markdown under it when the brain gave some, and why it ended.
pub struct Answered {
    pub talk: Talk,
    pub details: Option<String>,
    pub end: End,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ConversationTests.wordsThatCancelAreKnown
    #[test]
    fn words_that_cancel_are_known() {
        for words in [
            "never mind",
            "Never mind!",
            "nevermind",
            "stop",
            "STOP.",
            "cancel",
            "cancel that",
            "forget it",
            "wait",
            "no, wait",
        ] {
            assert!(cancels(words), "{words}");
        }
        for words in [
            "stop everything",
            "stop the vault job",
            "cancel the review",
            "never mind that, review PR 12",
            "how is it going?",
        ] {
            assert!(!cancels(words), "{words}");
        }
    }

    fn waiting(turn: &str, signed: bool) -> (crate::conversation::Turn, crate::delivery::Replay) {
        let mut said = crate::conversation::Turn::of("user", format!("said {turn}"));
        said.id = turn.into();
        let replay = crate::delivery::Replay {
            attachments: vec![format!("{turn}.png")],
            signed,
        };
        (said, replay)
    }

    /// Words that waited are answered together as the latest's: their words one after another,
    /// every attachment, signed only when each was (CONVERSATION-AGENT-49).
    #[test]
    fn words_that_waited_are_answered_together() {
        let words = Words::together(&[waiting("t2", true), waiting("t3", true)], None).unwrap();
        assert_eq!(words.turn, "t3");
        assert_eq!(words.said, "said t2\n\nsaid t3");
        assert_eq!(words.attachments, ["t2.png", "t3.png"]);
        assert!(words.signed);
        let mixed = Words::together(&[waiting("t2", true), waiting("t3", false)], None).unwrap();
        assert!(!mixed.signed);
        assert!(Words::together(&[], None).is_none());
    }

    /// COMPOSER-7: words answered together take the latest words' chips.
    #[test]
    fn composer_7_words_answered_together_take_the_latest_chips() {
        let chips = super::super::defaults::Defaults {
            mode: Some("ask".into()),
            ..Default::default()
        };
        let words = Words::together(
            &[waiting("t2", true), waiting("t3", true)],
            Some(chips.clone()),
        );
        assert_eq!(words.unwrap().defaults, Some(chips));
    }

    /// CONVERSATION-AGENT-49: the request for some words hears their answer once, and what it
    /// asked with is taken once; a drain runs one at a time in a conversation, ends only when
    /// nothing waits, and one that panicked lets the next start.
    #[tokio::test]
    async fn conversation_agent_49_a_request_hears_once_and_one_drain_runs() {
        let turns = Turns::default();
        let asking = Asking {
            back: Some(7),
            ..Default::default()
        };
        let heard = turns.ask("t1", asking);
        assert_eq!(turns.take_asking("t1").and_then(|a| a.back), Some(7));
        assert!(turns.take_asking("t1").is_none());
        turns.tell("t1", Heard::Answer(serde_json::json!({"kind": "none"})));
        turns.tell("t1", Heard::Answer(serde_json::json!({"kind": "reply"})));
        match heard.await.unwrap() {
            Heard::Answer(body) => assert_eq!(body["kind"], "none"),
            other => panic!("{other:?}"),
        }
        assert!(turns.start_draining("c1"));
        assert!(!turns.start_draining("c1"), "one drain at a time");
        assert!(turns.start_draining("c2"), "another conversation's own");
        assert!(!turns.done_draining("c1", || false), "words wait still");
        assert!(!turns.start_draining("c1"));
        assert!(turns.done_draining("c1", || true));
        assert!(turns.start_draining("c1"));
        turns.abandon_draining("c1");
        assert!(turns.start_draining("c1"));
    }

    /// A turn begun cancels nothing (its conversation's drain runs one at a time); Stop and
    /// cancel reach only their own conversation's.
    #[test]
    fn stop_and_cancel_reach_only_their_conversation() {
        let turns = Turns::default();
        let first = turns.begin("t1", "c1");
        let other = turns.begin("t2", "c2");
        assert!(first.live() && other.live());
        assert!(turns.stop(Some("c2")));
        assert!(other.stopped() && first.live());
        turns.cancel(Some("c1"));
        assert!(first.cancelled());
        turns.end(&first);
        assert!(!turns.stop(Some("c3")));
        assert_eq!(turns.latest_conversation(), Some("c2".into()));
    }
}
