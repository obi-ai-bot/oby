//! What a [`super::Driver`] needs (BROWSER-24): obid's home, where the engine comes from, and its
//! limits: four live browsers, 10 minutes idle, and how long its own calls may take. Its
//! constructor only assigns. It owns no state and writes nothing.

use std::path::PathBuf;
use std::time::Duration;

use super::Binaries;

/// At most this many browsers live at once (the owner's "4 at once").
pub const MAX_LIVE: usize = 4;
/// A browser idle this long closes itself (agent-browser's `--idle-timeout`).
pub const IDLE_TIMEOUT: &str = "10m";
/// The first call that starts a job's daemon and Chrome, within `browser_up`'s 60 s.
pub const START_LIMIT: Duration = Duration::from_secs(50);
/// obid's own reads around an agent's command (`tab list`, `get url`).
pub const SYSTEM_LIMIT: Duration = Duration::from_secs(15);
/// One try at reading a timed-out tab's URL while it settles; tried again until it answers.
pub const SETTLE_TRY: Duration = Duration::from_secs(5);
/// From SIGTERM to SIGKILL when a job's browser is stopped.
pub const END_GRACE: Duration = Duration::from_secs(3);

/// What a driver needs.
#[derive(Clone, Debug)]
pub struct Options {
    pub home: PathBuf,
    pub binaries: Binaries,
    pub max_live: usize,
    pub idle_timeout: String,
    pub start_limit: Duration,
    pub system_limit: Duration,
    pub settle_try: Duration,
    pub end_grace: Duration,
}

impl Options {
    pub fn new(home: PathBuf, binaries: Binaries) -> Options {
        Options {
            home,
            binaries,
            max_live: MAX_LIVE,
            idle_timeout: IDLE_TIMEOUT.into(),
            start_limit: START_LIMIT,
            system_limit: SYSTEM_LIMIT,
            settle_try: SETTLE_TRY,
            end_grace: END_GRACE,
        }
    }
}
