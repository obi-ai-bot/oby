//! Unpacking a checked download with the system's own tools (BROWSER-23): `tar -xzf` for a
//! tarball; `ditto -x -k` on macOS and `unzip -q` on Linux for a zip, refused plainly where
//! `unzip` is missing. It runs only after the download matched its pin, writes only into the folder
//! it is given, and runs nothing it unpacked.

use std::path::{Path, PathBuf};

use super::Archive;

/// Unpacks `file` (packed as `archive`) into the folder `into`.
pub async fn unpack(archive: Archive, file: &Path, into: &Path) -> Result<(), String> {
    let mut command = match archive {
        Archive::TarGz => {
            let tar = system_tool("tar").ok_or("obid can't find tar to unpack agent-browser.")?;
            let mut c = tokio::process::Command::new(tar);
            c.arg("-xzf").arg(file).arg("-C").arg(into);
            c
        }
        Archive::Zip if cfg!(target_os = "macos") => {
            let mut c = tokio::process::Command::new("/usr/bin/ditto");
            c.args(["-x", "-k"]).arg(file).arg(into);
            c
        }
        Archive::Zip => {
            let unzip = system_tool("unzip").ok_or(
                "unzip isn't installed on this computer, so obid can't unpack chrome-headless-shell: install unzip (for example `sudo apt install unzip`) and try again.",
            )?;
            let mut c = tokio::process::Command::new(unzip);
            c.arg("-q").arg(file).arg("-d").arg(into);
            c
        }
    };
    let out = command
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("can't unpack {}: {e}", file.display()))?;
    if !out.status.success() {
        return Err(format!(
            "can't unpack {}: {}",
            file.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// A tool from the system's own folders, never one a person's `PATH` put first.
fn system_tool(name: &str) -> Option<PathBuf> {
    ["/usr/bin", "/bin", "/usr/local/bin"]
        .iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|p| p.is_file())
}
