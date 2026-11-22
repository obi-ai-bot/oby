//! macOS's quarantine mark on a download delivered into the person's Downloads (BROWSER-34): the
//! copy gets the one its profile file had, or obid's, so Gatekeeper asks before it first opens.
//! Read and set with macOS's own `/usr/bin/xattr -s`, on the file at the path itself, never through
//! a link (obid's one `unsafe` is UNSAFE-1's, so no `setxattr` of its own). It owns no state and
//! writes only that one attribute.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

const XATTR: &str = "/usr/bin/xattr";
const NAME: &str = "com.apple.quarantine";

/// The file's quarantine mark, if it has one.
pub fn read(path: &Path) -> Option<String> {
    let out = Command::new(XATTR)
        .args(["-s", "-p", NAME])
        .arg(path)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// Marks the copy at `path` with `from`'s non-empty quarantine mark, or else obid's: macOS's
/// `flags;hex-seconds;agent;`, with Oby as the agent.
pub fn ensure(path: &Path, from: &Path) -> io::Result<()> {
    let mark = read(from).filter(|v| !v.is_empty()).unwrap_or_else(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        format!("0081;{secs:x};Oby;")
    });
    let set = Command::new(XATTR)
        .args(["-s", "-w", NAME, &mark])
        .arg(path)
        .stderr(Stdio::null())
        .status()?;
    if set.success() && read(path).is_some_and(|v| !v.is_empty()) {
        Ok(())
    } else {
        Err(io::Error::other(format!("{XATTR} -w {NAME} failed")))
    }
}
