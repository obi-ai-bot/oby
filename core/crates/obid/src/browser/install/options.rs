//! What an [`super::Installer`] installs and where (BROWSER-23): obid's home and the two pins.
//! [`super::pinned_options`] gives this computer's; a test gives pins of its own, served from a
//! local listener. It owns no state and writes nothing.

use std::path::PathBuf;

use super::Pin;

/// What an installer installs and where.
#[derive(Clone, Debug)]
pub struct Options {
    /// obid's home: the engine goes under `<home>/browser/`.
    pub home: PathBuf,
    /// agent-browser's pin, or why this computer has none.
    pub agent_browser: Result<Pin, String>,
    /// chrome-headless-shell's pin, or why this computer has none (linux-arm64).
    pub chrome: Result<Pin, String>,
}

impl Options {
    pub fn new(
        home: PathBuf,
        agent_browser: Result<Pin, String>,
        chrome: Result<Pin, String>,
    ) -> Options {
        Options {
            home,
            agent_browser,
            chrome,
        }
    }
}
