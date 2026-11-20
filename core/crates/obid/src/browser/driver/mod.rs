//! The browser's engine driver (BROWSER-23 to 25, and the engine side of BROWSER-10 to 13): one
//! agent-browser daemon and its chrome-headless-shell per job, started on the job's first browser
//! call once [`crate::browser::install`] has put the engine in place, at most four live at once,
//! each closing itself after 10 minutes idle, killed when its job ends and reaped when obid starts.
//! It runs what the core's [`crate::browser::engine::Ticket`] says, right before checking it may
//! start, under its deadline, never replaying one that timed out, and tells the core what the page
//! is now.
//!
//! It owns, under obid's home, `browser/jobs/<job>/` (each job's `HOME` and profile, its downloads
//! in `profile/Downloads`), `browser/live/<job>.json` (the PIDs it started) and each job's socket
//! folder (`b/<id>`, or a private one in `/tmp` when that path would pass a socket's 103 bytes). It
//! never writes outside them, never touches the person's own Chrome or its profile, and never
//! signals a process whose command isn't under `<home>/browser/`.

mod binaries;
mod call;
mod certificate;
mod chrome_said;
mod download_file;
mod downloads;
mod drive;
mod frame;
mod live;
mod live_file;
mod options;
mod person;
mod person_page;
mod person_proceed;
mod procs;
mod reap;
mod run;
mod session;
/// The tests' stand-in agent-browser: obid's unit tests', and a debug build's integration tests'
/// (`OBID_TEST_BROWSER_STAND_IN`); never in a release build.
#[cfg(any(test, debug_assertions))]
#[doc(hidden)]
pub mod stand_in;
mod stream;
mod stream_reader;
mod waiting;

pub use binaries::Binaries;
pub use call::Call;
pub use download_file::DownloadFile;
pub use drive::Driver;
pub use frame::Frame;
pub use live_file::LiveFile;
pub use options::{
    END_GRACE, IDLE_TIMEOUT, MAX_LIVE, Options, SETTLE_TRY, START_LIMIT, STOP_GRACE, SYSTEM_LIMIT,
};
pub use reap::reap_at_start;
pub use session::{SOCKET_PATH_MAX, Session};
pub use stream::Stream;
pub use stream_reader::StreamReader;
