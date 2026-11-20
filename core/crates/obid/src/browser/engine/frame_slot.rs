//! The newest frame a watcher hasn't read yet (BROWSER-14): a slow watcher drops frames rather
//! than read old ones. It owns only its waiting frame and counters; it writes nothing.

use std::collections::VecDeque;

/// The newest frame a watcher hasn't read yet: at most one, so a slow watcher drops frames rather
/// than read old ones.
#[derive(Debug)]
pub struct FrameSlot<T> {
    waiting: VecDeque<T>,
    pub dropped: u64,
    pub most_waiting: usize,
}

impl<T> Default for FrameSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FrameSlot<T> {
    pub fn new() -> Self {
        FrameSlot {
            waiting: VecDeque::new(),
            dropped: 0,
            most_waiting: 0,
        }
    }

    pub fn put(&mut self, frame: T) {
        if !self.waiting.is_empty() {
            self.dropped += self.waiting.len() as u64;
            self.waiting.clear();
        }
        self.waiting.push_back(frame);
        self.most_waiting = self.most_waiting.max(self.waiting.len());
    }

    pub fn take(&mut self) -> Option<T> {
        self.waiting.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_14_a_slow_watcher_reads_the_newest_frame_and_drops_the_rest() {
        let mut slot = FrameSlot::new();
        for seq in 1..=10u64 {
            slot.put(seq);
        }
        assert_eq!(slot.take(), Some(10));
        assert_eq!(slot.take(), None);
        assert_eq!((slot.dropped, slot.most_waiting), (9, 1));
    }
}
