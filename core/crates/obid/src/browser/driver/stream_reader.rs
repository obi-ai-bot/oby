//! One reader of a job's live stream (BROWSER-14): the newest frame it hasn't read (the core's
//! [`FrameSlot`]: a slow reader drops frames, never queues them), woken when one arrives, and told
//! when the stream closed. It owns its slot and writes nothing.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use super::Frame;
use crate::browser::engine::FrameSlot;

/// One reader of a job's live stream.
#[derive(Debug, Default)]
pub struct StreamReader {
    slot: Mutex<FrameSlot<Arc<Frame>>>,
    wake: Notify,
    closed: AtomicBool,
}

impl StreamReader {
    pub fn new() -> StreamReader {
        StreamReader::default()
    }

    /// A frame arrived: it replaces any this reader hasn't read.
    pub fn put(&self, frame: Arc<Frame>) {
        self.slot.lock().unwrap().put(frame);
        self.wake.notify_one();
    }

    /// How many frames this reader dropped so far.
    pub fn dropped(&self) -> u64 {
        self.slot.lock().unwrap().dropped
    }

    /// The stream closed: a reader waiting is woken with nothing.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    /// The newest frame not read yet, with how many this reader dropped so far, or None once the
    /// stream closed.
    pub async fn next(&self) -> Option<(Arc<Frame>, u64)> {
        loop {
            {
                let mut slot = self.slot.lock().unwrap();
                if let Some(frame) = slot.take() {
                    return Some((frame, slot.dropped));
                }
            }
            if self.closed.load(Ordering::SeqCst) {
                return None;
            }
            self.wake.notified().await;
        }
    }
}
