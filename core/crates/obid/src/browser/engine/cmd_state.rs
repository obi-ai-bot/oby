//! A command's fate in the engine's ledger (BROWSER-11): running, done, failed or timed out. It
//! owns no state and writes nothing.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmdState {
    Running,
    Done,
    Failed,
    TimedOut,
}
