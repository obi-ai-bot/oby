//! The owner's words to a job's agent between and during its turns (#1661, TASKCONV-29 to 31):
//! what its next turn hears (`Job::heard`, `Job::owners`), and the words a Claude Code turn under
//! way takes as they come (crate::steer), which leave `heard` once that turn is over. It owns the
//! jobs' `steers` while a turn runs; it never starts, stops or publishes a job.

use super::*;

impl Jobs {
    /// A Claude Code turn of job `id` starts: the owner's words from now on go into it as they
    /// come, once its agent replays (crate::steer).
    pub(super) fn steer_open(&self, id: &str) -> Arc<crate::steer::Steer> {
        let steer = Arc::new(crate::steer::Steer::default());
        self.lock().steers.insert(id.to_owned(), steer.clone());
        steer
    }

    /// Job `id`'s turn is over, however it ended: no more words go into it, and those it took
    /// leave what its next turn hears and the owner's words it never heard, each once. Those it
    /// didn't take stay for that turn.
    pub(super) fn steer_close(&self, id: &str, steer: &crate::steer::Steer) {
        let taken = {
            let mut state = self.lock();
            if state.steers.get(id).is_some_and(|s| std::ptr::eq(&**s, steer)) {
                state.steers.remove(id);
            }
            steer.close()
        };
        if taken.is_empty() {
            return;
        }
        self.update(id, |job| {
            for heard in &taken {
                if let Some(at) = job.heard.iter().position(|h| h == heard) {
                    job.heard.remove(at);
                }
                if let Some(at) = job.owners.iter().position(|w| w.text == *heard) {
                    job.owners.remove(at);
                }
            }
        });
    }

    /// The owner's words the agent hasn't heard yet, taken: what its next turn hears.
    pub(super) fn take_heard(&self, id: &str) -> Vec<String> {
        self.take_words(id).0
    }

    /// [`Self::take_heard`], and whether the owner's own words were among them (#1661).
    pub(super) fn take_words(&self, id: &str) -> (Vec<String>, bool) {
        let mut taken = (Vec::new(), false);
        self.update(id, |job| {
            taken = (
                std::mem::take(&mut job.heard),
                !std::mem::take(&mut job.owners).is_empty(),
            );
        });
        taken
    }

    /// The agent's last turn is over: the owner's words that came meanwhile, taken (its next
    /// turn), and whether the owner's own were among them, or none, and from now on the job
    /// takes no more until it ends.
    pub(super) fn last_turn(&self, id: &str) -> (Vec<String>, bool) {
        let (words, owners) = {
            let mut state = self.lock();
            let (words, owners) = state
                .jobs
                .get_mut(id)
                .map(|job| {
                    (
                        std::mem::take(&mut job.heard),
                        !std::mem::take(&mut job.owners).is_empty(),
                    )
                })
                .unwrap_or_default();
            if words.is_empty() {
                state.wrapping.insert(id.to_owned());
            }
            (words, owners)
        };
        if !words.is_empty() {
            self.update(id, |_| {});
        }
        (words, owners)
    }
}
