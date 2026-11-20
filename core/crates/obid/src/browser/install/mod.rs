//! The browser's engine reaching this computer (BROWSER-23): obid downloads agent-browser and
//! chrome-headless-shell itself on the first browser call, into its home (`<home>/browser/`,
//! private 0700 folders), each checked against a SHA-256 pinned here per platform before anything
//! of it is unpacked, and agent-browser's binary checked again once unpacked. A file that doesn't
//! match is deleted and never run. One install at a time: two jobs asking at once download once.
//!
//! It owns `<home>/browser/agent-browser-<version>/`,
//! `<home>/browser/chrome-headless-shell-<version>/` and the `.partial-*` folders it downloads
//! into; it never writes anywhere else, never runs what it downloaded, and never touches the
//! person's own Chrome or its profile.

mod archive;
mod fetch;
mod installed;
mod installer;
mod options;
mod pin;
mod pins;
mod unpack;

pub use archive::Archive;
pub use fetch::{fetch_verified, sha256_file};
pub use installed::Installed;
pub use installer::Installer;
pub use options::Options;
pub use pin::Pin;
pub use pins::{
    AGENT_BROWSER_BASE, AGENT_BROWSER_VERSION, CHROME_BASE, CHROME_VERSION, agent_browser, chrome,
    pinned_options, this_platform,
};
pub use unpack::unpack;
