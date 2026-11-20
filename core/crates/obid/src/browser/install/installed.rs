//! Where the browser's engine is once in place (BROWSER-23): agent-browser's binary and Chrome for
//! Testing's executable, both under `<home>/browser/`. It owns no state and writes nothing.

use std::path::PathBuf;

/// Where the browser's engine is once in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub agent_browser: PathBuf,
    pub chrome: PathBuf,
}
