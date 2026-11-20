//! A file an agent downloaded, still in its profile (BROWSER-13): its name (the page's words) and
//! size. It owns no state and writes nothing.

/// A file an agent downloaded, still in its profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadFile {
    pub name: String,
    pub bytes: u64,
}
