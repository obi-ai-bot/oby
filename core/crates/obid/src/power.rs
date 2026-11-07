//! Keeps the Mac from idle sleep while any job's worker runs (RECOVERY-10): an idle sleep would
//! freeze every agent, check and retry clock. `/usr/bin/caffeinate -i -w <obid's pid>` while the
//! count of workers is above 0, ended when it's back to 0; `-w` ends it with obid too. Idle sleep
//! only: a closed lid still sleeps. `keepAwake: false` turns it off.

use std::process::{Child, Command, Stdio};

/// The sleep assertion, held while [`Awake::keep`] says so.
pub struct Awake {
    on: bool,
    child: Option<Child>,
}

impl Awake {
    /// `on` is `keepAwake`; off, [`Awake::keep`] does nothing.
    pub fn new(on: bool) -> Self {
        Awake { on, child: None }
    }

    /// Holds the assertion when `wanted` (a worker runs), lets it go when not.
    pub fn keep(&mut self, wanted: bool) {
        if !wanted {
            self.release();
            return;
        }
        // One that ended by itself (killed from outside) is started again.
        if let Some(child) = &mut self.child
            && matches!(child.try_wait(), Ok(None))
        {
            return;
        }
        self.release();
        if !(self.on && cfg!(target_os = "macos")) {
            return;
        }
        match Command::new("/usr/bin/caffeinate")
            .args(["-i", "-w", &std::process::id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => self.child = Some(child),
            Err(e) => tracing::warn!("couldn't keep the Mac awake: caffeinate: {e}"),
        }
    }

    fn release(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        self.release();
    }
}
