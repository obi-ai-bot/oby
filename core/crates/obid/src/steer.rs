//! Words written into a Claude Code job's turn while it runs (docs/contracts/task-conversations.md,
//! TASKCONV-33 to 35): the owner's words to a task at work reach its agent at once, in the same
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
//! Codex (TASKCONV-36 and 37) takes them through `turn/steer` on its turn under way, as T3 Code
//! does: Codex answers each at once, and an answer that isn't an error is the word taken, which it
//! hears at its next step (codex-cli 0.163, 2026-11-18). Its turn opens to words once `turn/start`
//! answers; words that come as a turn ends start the thread's next turn in the same process
//! ([`Steer::next_turn`]), never a Codex started again.
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

    /// Codex's turn started: words go in from now on (TASKCONV-36).
    pub fn opened(&self) {
        let mut words = self.words();
        words.open = true;
        // Words that waited while no turn was under way go in now.
        if !words.queued.is_empty() {
            self.came.notify_one();
        }
    }

    /// Codex answered the first word written and not answered yet: `took` it, or refused it (its
    /// turn was over), and then it waits for the thread's next turn ([`Self::next_turn`]).
    pub fn answered(&self, took: bool) {
        let mut words = self.words();
        let Some(heard) = words.written.pop_front() else {
            return;
        };
        if took {
            words.taken.push(heard);
        } else if let Ok(clean) = crate::vault::for_agent(&heard) {
            words.queued.push_front((heard, clean));
        }
    }

    /// Codex's turn is over and every word written was answered: those it didn't take, as it gets
    /// them, are the thread's next turn (written now: [`Self::answered`] says whether it took
    /// them); none, and no word goes in from now on (TASKCONV-37).
    pub fn next_turn(&self) -> Vec<String> {
        let mut words = self.words();
        if words.queued.is_empty() {
            words.open = false;
            return Vec::new();
        }
        drop(words);
        self.to_write()
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

    /// TASKCONV-35: nothing goes in before the agent replays its prompt, nor after its turn is
    /// over; a result with words not taken yet isn't the end; only replayed words count as heard.
    #[test]
    fn taskconv_35_only_replayed_words_are_heard() {
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

    /// TASKCONV-36: Codex's turn takes words once it started; one it refused waits for the next
    /// turn, which takes them all, and with nothing left no word goes in.
    #[test]
    fn taskconv_36_codex_takes_words_as_steer_answers() {
        let steer = Steer::default();
        assert!(!steer.offer("too early"), "before its turn started");
        steer.opened();
        assert!(steer.offer("use SQLite"));
        assert!(steer.offer("and tests"));
        assert_eq!(steer.to_write(), ["use SQLite", "and tests"]);
        steer.answered(true);
        steer.answered(false);
        assert!(steer.offer("late"));
        assert_eq!(steer.next_turn(), ["and tests", "late"]);
        steer.answered(true);
        steer.answered(true);
        assert!(steer.next_turn().is_empty());
        assert!(!steer.offer("after"), "its last turn is over");
        assert_eq!(steer.close(), ["use SQLite", "and tests", "late"]);
    }

    /// TASKCONV-35: words offered but never taken (the turn was cut) aren't heard.
    #[test]
    fn taskconv_35_words_not_taken_stay_unheard() {
        let steer = Steer::default();
        steer.replayed();
        assert!(steer.offer("one"));
        assert!(!steer.over(), "queued, not written");
        assert_eq!(steer.to_write(), ["one"]);
        assert!(steer.close().is_empty());
        assert!(!steer.offer("two"));
    }
}
