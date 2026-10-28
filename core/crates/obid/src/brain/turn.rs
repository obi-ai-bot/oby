//! The turns under way, as Stop and cancel reach them: one answer at a time in a conversation
//! (words said meanwhile wait for it, then are answered together), a user's words preempting
//! obid's own event turn (#590), and why each turn ended (#538). The turn itself runs on the
//! conversation harness (super::harness).

use std::collections::HashMap;
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
}

/// What the owner's next turn in a conversation answers ([`Turns::wait_turn`]): every word said
/// there since the turn before, as one. It holds the conversation until it's dropped, so words
/// said meanwhile wait for it.
pub struct Next {
    /// The latest words' turn, the words one after another, every attachment, and the latest
    /// Mac picked to run on.
    pub words: Words,
    /// The user's turns it answers, oldest first.
    pub turns: Vec<String>,
    _held: tokio::sync::OwnedMutexGuard<()>,
}

/// The turns under way.
#[derive(Debug, Default)]
pub struct Turns {
    underway: Mutex<Vec<Arc<Control>>>,
    /// The owner's words no turn has taken yet, each with its conversation, in the order they came.
    waiting: Mutex<Vec<(String, Words)>>,
    /// One lock per conversation, held by the owner's turn there until it ends: one answer at a
    /// time in a conversation (docs/mac-design.md 1.6).
    owners: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl Turns {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Arc<Control>>> {
        self.underway.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn waiting(&self) -> std::sync::MutexGuard<'_, Vec<(String, Words)>> {
        self.waiting.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Waits for the owner's turn under way in `conversation` to end, then takes every word said
    /// there that no turn took yet, `words` last: the next turn answers them all at once. None
    /// when later words came meanwhile (their turn answers these with theirs) or a cancel dropped
    /// them: nothing answers them on their own.
    pub async fn wait_turn(&self, conversation: &str, words: Words) -> Option<Next> {
        let turn = words.turn.clone();
        self.waiting().push((conversation.to_owned(), words));
        let lock = self
            .owners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(conversation.to_owned())
            .or_default()
            .clone();
        // Fair: the words take the conversation in the order they came.
        let held = lock.lock_owned().await;
        let mut waiting = self.waiting();
        if waiting
            .iter()
            .rev()
            .find(|(c, _)| c == conversation)
            .is_none_or(|(_, w)| w.turn != turn)
        {
            return None;
        }
        let (taken, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut *waiting)
            .into_iter()
            .partition(|(c, _)| c == conversation);
        *waiting = kept;
        let taken: Vec<Words> = taken.into_iter().map(|(_, w)| w).collect();
        Some(Next {
            words: Words {
                turn,
                said: taken
                    .iter()
                    .map(|w| w.said.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                attachments: taken.iter().flat_map(|w| w.attachments.clone()).collect(),
                jobs: taken.iter().flat_map(|w| w.jobs.clone()).collect(),
            },
            turns: taken.into_iter().map(|w| w.turn).collect(),
            _held: held,
        })
    }

    /// A new turn in `conversation`: one obid started there by itself (a supervisor's) is
    /// cancelled, and its plan, when it comes, does nothing. The owner's turn under way there is
    /// never one: their next words wait for it ([`Turns::wait_turn`]).
    pub fn begin(&self, turn: &str, conversation: &str) -> Arc<Control> {
        let control = Arc::new(Control {
            turn: turn.into(),
            conversation: conversation.into(),
            stopped: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        });
        let mut underway = self.lock();
        for earlier in underway.iter().filter(|c| c.conversation == conversation) {
            earlier.cancelled.store(true, Ordering::SeqCst);
        }
        // A user's words preempt the event orchestrator's turn: it stops after the action under
        // way (#590).
        if conversation != super::orchestrator::CONVERSATION {
            for event in underway
                .iter()
                .filter(|c| c.conversation == super::orchestrator::CONVERSATION)
            {
                event.stopped.store(true, Ordering::SeqCst);
            }
        }
        underway.push(control.clone());
        control
    }

    /// A turn in `conversation` only when none is under way there, checked as it begins: a
    /// supervisor turn (super::supervisor) never replaces the owner's.
    pub fn begin_if_idle(&self, turn: &str, conversation: &str) -> Option<Arc<Control>> {
        let mut underway = self.lock();
        if underway.iter().any(|c| c.conversation == conversation) {
            return None;
        }
        let control = Arc::new(Control {
            turn: turn.into(),
            conversation: conversation.into(),
            stopped: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        });
        underway.push(control.clone());
        Some(control)
    }

    pub fn end(&self, control: &Arc<Control>) {
        self.lock().retain(|c| !Arc::ptr_eq(c, control));
    }

    /// Asks the turns under way (in `conversation`, or all) to stop after the action under way.
    /// True when there was one.
    pub fn stop(&self, conversation: Option<&str>) -> bool {
        self.mark(conversation, |c| c.stopped.store(true, Ordering::SeqCst))
    }

    /// Drops the turns under way (in `conversation`, or all): their plans do nothing; and the
    /// owner's words waiting there: no turn answers them.
    pub fn cancel(&self, conversation: Option<&str>) -> bool {
        self.waiting()
            .retain(|(c, _)| conversation.is_some_and(|id| c != id));
        self.mark(conversation, |c| c.cancelled.store(true, Ordering::SeqCst))
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

    /// Whether a user's turn is under way: the event orchestrator waits for it (#590).
    pub fn user_turn_underway(&self) -> bool {
        self.lock()
            .iter()
            .any(|c| c.conversation != super::orchestrator::CONVERSATION)
    }

    /// Whether a turn is under way in `conversation`: a supervisor turn waits for it to end.
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

    /// A supervisor turn begins only where no turn is under way, and never cancels the owner's.
    #[test]
    fn a_supervisor_turn_never_replaces_the_owners() {
        let turns = Turns::default();
        let owner = turns.begin("t1", "c1");
        assert!(turns.begin_if_idle("s1", "c1").is_none());
        assert!(owner.live());
        let elsewhere = turns.begin_if_idle("s2", "c2").unwrap();
        assert!(turns.underway_in("c2"));
        turns.end(&elsewhere);
        turns.end(&owner);
        assert!(turns.begin_if_idle("s3", "c1").is_some());
    }

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

    /// Words said while the owner's turn is under way wait for it to end; the next turn answers
    /// every one of them at once, as the latest's. Cancel drops what waits, and another
    /// conversation never waits.
    #[tokio::test]
    async fn words_wait_for_the_turn_under_way_and_are_answered_together() {
        let turns = Arc::new(Turns::default());
        let words = |turn: &str| Words {
            turn: turn.into(),
            said: format!("said {turn}"),
            attachments: vec![format!("{turn}.png")],
            jobs: vec![],
        };
        let wait = |turn: &'static str| {
            let turns = turns.clone();
            tokio::spawn(async move {
                turns
                    .wait_turn("c1", words(turn))
                    .await
                    .map(|next| (next.turns, next.words))
            })
        };
        let waiting = |n: usize| {
            let turns = turns.clone();
            async move {
                while turns.waiting().len() < n {
                    tokio::task::yield_now().await;
                }
            }
        };
        let first = turns.wait_turn("c1", words("t1")).await.unwrap();
        assert_eq!(first.turns, ["t1"]);
        assert_eq!(first.words.said, "said t1");
        drop(turns.wait_turn("c2", words("t9")).await.unwrap());
        let second = wait("t2");
        waiting(1).await;
        let third = wait("t3");
        waiting(2).await;
        drop(first);
        assert!(
            second.await.unwrap().is_none(),
            "the latest words answer it"
        );
        let (answered, next) = third.await.unwrap().unwrap();
        assert_eq!(answered, ["t2", "t3"]);
        assert_eq!(next.turn, "t3");
        assert_eq!(next.said, "said t2\n\nsaid t3");
        assert_eq!(next.attachments, ["t2.png", "t3.png"]);
        let held = turns.wait_turn("c1", words("t4")).await.unwrap();
        let dropped = wait("t5");
        waiting(1).await;
        turns.cancel(Some("c2"));
        assert_eq!(turns.waiting().len(), 1, "another conversation's cancel");
        turns.cancel(Some("c1"));
        drop(held);
        assert!(dropped.await.unwrap().is_none(), "cancelled");
        assert!(turns.waiting().is_empty());
    }

    /// A turn begun cancels the one under way in its conversation (obid's own: the owner's words
    /// wait for theirs), never another conversation's; Stop reaches only its own.
    #[test]
    fn a_turn_begun_cancels_the_one_under_way_in_its_conversation_only() {
        let turns = Turns::default();
        let first = turns.begin("t1", "c1");
        let other = turns.begin("t2", "c2");
        let second = turns.begin("t3", "c1");
        assert!(first.cancelled() && !first.stopped());
        assert!(other.live() && second.live());
        assert!(turns.stop(Some("c2")));
        assert!(other.stopped() && second.live());
        turns.end(&second);
        assert!(!turns.cancel(Some("c3")));
        assert_eq!(turns.latest_conversation(), Some("c2".into()));
    }
}
