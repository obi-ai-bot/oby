//! Words written into a Claude Code job's turn while it runs (docs/contracts/task-conversations.md,
//! TASKCONV-29 to 31): the owner's words to a task at work reach its agent at once, in the same
//! process, never by waiting for its turn to end and starting Claude Code again.
//!
//! Claude Code reads each `user` line obid writes to its stream-json input as it comes, and takes
//! it at its next step: after the tool call under way, in the same turn, or, when the turn ends
//! first, as a turn of its own right after. Run with `--replay-user-messages` it writes each one
//! back (`isReplay`) as it takes it (claude 2.1.325, 2026-11-17), so obid knows which it heard and
//! whether its `result` is the last. Its first line back is the turn's own prompt: until then
//! nothing is written, so a Claude Code that doesn't replay hears no word this way (the words wait
//! for its next turn, as before).
//!
//! It owns only what is written and taken in one turn. The job's own record of the words
//! (`Job::heard`) stays theirs (crate::jobs): what a turn took leaves it once the turn is over,
//! and what it didn't, the next turn hears. It never writes a job.

use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::Notify;

/// One turn's words written into it, from the owner's word to Claude Code's replay.
#[derive(Default)]
pub struct Steer {
    words: Mutex<Words>,
    came: Notify,
}

#[derive(Default)]
struct Words {
    /// The agent replays, so words may go in; false again once the turn is over.
    open: bool,
    /// Its replays so far: the first is its prompt.
    replays: usize,
    /// Offered, not written yet, as the job keeps them and as they're written (the vault's
    /// secrets taken out).
    queued: VecDeque<(String, String)>,
    /// Written, not taken yet.
    written: VecDeque<String>,
    /// Taken, as the job keeps them.
    taken: Vec<String>,
}

impl Steer {
    fn words(&self) -> std::sync::MutexGuard<'_, Words> {
        self.words.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The owner's `text` for the turn: true when it goes in, false when it can't (the agent
    /// doesn't replay, its turn is over, or the vault can't be read: then nothing reaches it).
    pub fn offer(&self, text: &str) -> bool {
        let mut words = self.words();
        if !words.open {
            return false;
        }
        let Ok(clean) = crate::vault::for_agent(text) else {
            return false;
        };
        words.queued.push_back((text.to_owned(), clean));
        self.came.notify_one();
        true
    }

    /// Resolves once words were offered since they were last written ([`Self::to_write`]).
    pub async fn came(&self) {
        self.came.notified().await;
    }

    /// The words to write now, as the agent gets them.
    pub fn to_write(&self) -> Vec<String> {
        let mut words = self.words();
        let queued: Vec<_> = words.queued.drain(..).collect();
        queued
            .into_iter()
            .map(|(kept, clean)| {
                words.written.push_back(kept);
                clean
            })
            .collect()
    }

    /// Claude Code wrote back a line it took: its prompt first, which opens the turn to words,
    /// then each of them in the order written.
    pub fn replayed(&self) {
        let mut words = self.words();
        words.replays += 1;
        if words.replays == 1 {
            words.open = true;
        } else if let Some(heard) = words.written.pop_front() {
            words.taken.push(heard);
        }
    }

    /// At a `result`: the turn is over (true, and no word goes in from now on) unless words it
    /// hasn't taken are on their way, which Claude Code answers in a turn of its own next.
    pub fn over(&self) -> bool {
        let mut words = self.words();
        if !words.queued.is_empty() || !words.written.is_empty() {
            return false;
        }
        words.open = false;
        true
    }

    /// No word goes in any more: the words the agent took, as the job keeps them.
    pub fn close(&self) -> Vec<String> {
        let mut words = self.words();
        words.open = false;
        std::mem::take(&mut words.taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TASKCONV-31: nothing goes in before the agent replays its prompt, nor after its turn is
    /// over; a result with words not taken yet isn't the end; only replayed words count as heard.
    #[test]
    fn taskconv_31_only_replayed_words_are_heard() {
        let steer = Steer::default();
        assert!(!steer.offer("too early"), "before its prompt's replay");
        steer.replayed();
        assert!(steer.offer("use SQLite"));
        assert!(steer.offer("and tests"));
        assert_eq!(steer.to_write(), ["use SQLite", "and tests"]);
        assert!(steer.to_write().is_empty());
        steer.replayed();
        assert!(!steer.over(), "one written word not taken yet");
        steer.replayed();
        assert!(steer.over());
        assert!(!steer.offer("late"), "its turn is over");
        assert_eq!(steer.close(), ["use SQLite", "and tests"]);
        assert!(steer.close().is_empty());
    }

    /// TASKCONV-31: words offered but never taken (the turn was cut) aren't heard.
    #[test]
    fn taskconv_31_words_not_taken_stay_unheard() {
        let steer = Steer::default();
        steer.replayed();
        assert!(steer.offer("one"));
        assert!(!steer.over(), "queued, not written");
        assert_eq!(steer.to_write(), ["one"]);
        assert!(steer.close().is_empty());
        assert!(!steer.offer("two"));
    }
}
