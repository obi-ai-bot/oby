//! One pinned download (BROWSER-23): where it comes from, its SHA-256 and size, how it is packed,
//! the folder it is unpacked into and the executable inside, with that executable's own SHA-256
//! when pinned. It owns no state and writes nothing.

use super::Archive;

/// One pinned download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    /// What it is, in a sentence's words: `agent-browser 0.43.0`.
    pub name: String,
    pub url: String,
    /// The download's SHA-256, lowercase hex.
    pub sha256: String,
    /// The download's size: more than this is refused before it is all read.
    pub bytes: u64,
    pub archive: Archive,
    /// The folder under `<home>/browser/` it is unpacked into.
    pub folder: String,
    /// The executable, relative to that folder.
    pub executable: String,
    /// The executable's own SHA-256, checked once unpacked, when it is pinned.
    pub executable_sha256: Option<String>,
}
