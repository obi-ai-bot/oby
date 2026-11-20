//! Putting the browser's engine in place, once (BROWSER-23): agent-browser first, then Chrome for
//! Testing, each downloaded into a `.partial-*` folder of `<home>/browser/`, checked against its
//! pin before it is unpacked, unpacked there, its executable checked (agent-browser's against its
//! own pin), then renamed into place so a half-done install is never seen. One install at a time
//! (an async mutex), in a task of its own: a caller that gives up (`browser_up`'s allowance) leaves
//! it running, and the next call finds it done or still going.
//!
//! It owns `<home>/browser/` but `jobs/`, `live/` and the sockets, which are the driver's; it never
//! runs what it downloads and writes nothing outside `<home>/browser/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{Installed, Options, Pin, fetch_verified, sha256_file, unpack};
use crate::store::{keep_private, make_private_dir};

/// Puts the browser's engine in place, once.
#[derive(Debug)]
pub struct Installer {
    options: Options,
    done: tokio::sync::Mutex<Option<Installed>>,
}

impl Installer {
    pub fn new(options: Options) -> Installer {
        Installer {
            options,
            done: tokio::sync::Mutex::new(None),
        }
    }

    /// The engine, downloaded and checked first when it isn't in place. The work runs in a task of
    /// its own, so a caller that stops waiting doesn't stop it.
    pub async fn ensure(self: &Arc<Self>) -> Result<Installed, String> {
        let me = self.clone();
        tokio::spawn(async move { me.ensure_here().await })
            .await
            .map_err(|e| format!("The browser's install stopped: {e}"))?
    }

    async fn ensure_here(&self) -> Result<Installed, String> {
        let mut done = self.done.lock().await;
        if let Some(installed) = done.as_ref() {
            return Ok(installed.clone());
        }
        let agent_browser = self.options.agent_browser.clone()?;
        let chrome = self.options.chrome.clone()?;
        let root = self.options.home.join("browser");
        make_private_dir(&root)
            .and_then(|()| keep_private(&root, 0o700))
            .map_err(|e| format!("can't make {}: {e}", root.display()))?;
        remove_partials(&root);
        let client = reqwest::Client::new();
        // agent-browser first: it is small, and one that doesn't match stops the install before
        // Chrome's 200 MB.
        let installed = Installed {
            agent_browser: one(&client, &root, &agent_browser).await?,
            chrome: one(&client, &root, &chrome).await?,
        };
        *done = Some(installed.clone());
        Ok(installed)
    }
}

/// Half-done installs a stopped obid left behind.
fn remove_partials(root: &Path) {
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with(".partial-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Whether the executable in place is the pinned one (or, with no pin of its own, is there).
async fn in_place(exe: &Path, pin: &Pin) -> bool {
    let exe = exe.to_path_buf();
    let want = pin.executable_sha256.clone();
    tokio::task::spawn_blocking(move || match want {
        Some(sha) => sha256_file(&exe).is_ok_and(|got| got == sha),
        None => exe.is_file(),
    })
    .await
    .unwrap_or(false)
}

/// One pin, in place under `root`: its executable's path.
async fn one(client: &reqwest::Client, root: &Path, pin: &Pin) -> Result<PathBuf, String> {
    let folder = root.join(&pin.folder);
    let exe = folder.join(&pin.executable);
    if folder.is_dir() {
        if in_place(&exe, pin).await {
            return Ok(exe);
        }
        // An executable missing or not the pinned one: the folder goes and is downloaded again.
        std::fs::remove_dir_all(&folder)
            .map_err(|e| format!("can't remove {}: {e}", folder.display()))?;
    }
    let partial = root.join(format!(".partial-{}-{}", pin.folder, std::process::id()));
    let _ = std::fs::remove_dir_all(&partial);
    make_private_dir(&partial).map_err(|e| format!("can't make {}: {e}", partial.display()))?;
    let outcome = put_in_place(client, pin, &partial, &folder).await;
    let _ = std::fs::remove_dir_all(&partial);
    outcome.map(|()| exe)
}

async fn put_in_place(
    client: &reqwest::Client,
    pin: &Pin,
    partial: &Path,
    folder: &Path,
) -> Result<(), String> {
    let download = partial.join("download");
    fetch_verified(client, pin, &download).await?;
    let unpacked = partial.join("unpacked");
    make_private_dir(&unpacked).map_err(|e| format!("can't make {}: {e}", unpacked.display()))?;
    unpack(pin.archive, &download, &unpacked).await?;
    let exe = unpacked.join(&pin.executable);
    if !exe.is_file() {
        return Err(format!(
            "{}'s download has no {}: nothing of it ran.",
            pin.name, pin.executable
        ));
    }
    if let Some(want) = &pin.executable_sha256 {
        let got = sha256_file(&exe).map_err(|e| format!("can't read {}: {e}", exe.display()))?;
        if &got != want {
            return Err(format!(
                "{}'s {} doesn't match its pin: its SHA-256 is {got}, the pinned one is {want}. It was deleted and never ran.",
                pin.name, pin.executable
            ));
        }
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&exe)
            .map_err(|e| format!("can't read {}: {e}", exe.display()))?
            .permissions()
            .mode();
        if mode & 0o100 == 0 {
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("can't make {} runnable: {e}", exe.display()))?;
        }
    }
    std::fs::rename(&unpacked, folder).map_err(|e| {
        format!(
            "can't move {} into {}: {e}",
            unpacked.display(),
            folder.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::super::{Archive, sha256_file};
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("obid-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A gzipped tarball holding `inside` (a path) with `body`, made with the system's tar.
    fn tarball(dir: &Path, inside: &str, body: &str) -> Vec<u8> {
        let src = dir.join("src");
        let _ = std::fs::remove_dir_all(&src);
        let file = src.join(inside);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, body).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = dir.join("out.tgz");
        let top = inside.split('/').next().unwrap();
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&out)
            .arg("-C")
            .arg(&src)
            .arg(top)
            .status()
            .unwrap();
        assert!(status.success());
        let bytes = std::fs::read(&out).unwrap();
        std::fs::remove_file(out).unwrap();
        std::fs::remove_dir_all(src).unwrap();
        bytes
    }

    fn sha256(bytes: &[u8]) -> String {
        hex::encode(obi_protocol::keys::sha256(bytes))
    }

    /// A local server answering each path with its bytes, counting the requests.
    async fn serve(files: Vec<(&'static str, Vec<u8>)>) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut conn, _)) = listener.accept().await else {
                    return;
                };
                counted.fetch_add(1, Ordering::SeqCst);
                let files = files.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        if conn.read(&mut byte).await.unwrap_or(0) == 0 {
                            return;
                        }
                        head.push(byte[0]);
                    }
                    let line = String::from_utf8_lossy(&head);
                    let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                    let reply = match files.iter().find(|(p, _)| *p == path) {
                        Some((_, body)) => {
                            let mut r = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            )
                            .into_bytes();
                            r.extend_from_slice(body);
                            r
                        }
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                    };
                    let _ = conn.write_all(&reply).await;
                });
            }
        });
        (base, hits)
    }

    fn pin(name: &str, url: String, bytes: &[u8], folder: &str, exe: &str) -> Pin {
        Pin {
            name: name.into(),
            url,
            sha256: sha256(bytes),
            bytes: bytes.len() as u64,
            archive: Archive::TarGz,
            folder: folder.into(),
            executable: exe.into(),
            executable_sha256: None,
        }
    }

    const BINARY: &str = "#!/bin/sh\necho stand-in agent-browser\n";
    const CHROME: &str = "#!/bin/sh\necho stand-in chrome\n";

    /// The two pins, served: agent-browser's tarball (its binary pinned too) and a Chrome tarball.
    struct Served {
        options: Options,
        hits: Arc<AtomicUsize>,
        agent_browser: Vec<u8>,
    }

    async fn served(home: &Path, corrupt: Option<&str>) -> Served {
        let ab = tarball(home, "package/bin/agent-browser", BINARY);
        let chrome = tarball(home, "chrome-test/chrome", CHROME);
        let flip = |mut b: Vec<u8>| {
            let i = b.len() / 2;
            b[i] ^= 0xff;
            b
        };
        let (base, hits) = serve(vec![
            (
                "/ab.tgz",
                if corrupt == Some("ab") {
                    flip(ab.clone())
                } else {
                    ab.clone()
                },
            ),
            (
                "/chrome.tgz",
                if corrupt == Some("chrome") {
                    flip(chrome.clone())
                } else {
                    chrome.clone()
                },
            ),
        ])
        .await;
        let mut ab_pin = pin(
            "agent-browser 0.43.0",
            format!("{base}/ab.tgz"),
            &ab,
            "agent-browser-0.43.0",
            "package/bin/agent-browser",
        );
        ab_pin.executable_sha256 = Some(sha256(BINARY.as_bytes()));
        let chrome_pin = pin(
            "Chrome for Testing 158.0.8127.36",
            format!("{base}/chrome.tgz"),
            &chrome,
            "chrome-158.0.8127.36",
            "chrome-test/chrome",
        );
        Served {
            options: Options::new(home.join("obid"), Ok(ab_pin), Ok(chrome_pin)),
            hits,
            agent_browser: ab,
        }
    }

    /// Every file under `dir`.
    fn files_under(dir: &Path) -> Vec<PathBuf> {
        let mut found = vec![];
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                found.extend(files_under(&p));
            } else {
                found.push(p);
            }
        }
        found
    }

    #[tokio::test]
    async fn browser_23_a_corrupted_download_is_refused_and_nothing_runs() {
        let home = scratch("corrupt");
        let s = served(&home, Some("ab")).await;
        let installer = Arc::new(Installer::new(s.options.clone()));
        let why = installer.ensure().await.unwrap_err();
        assert!(why.contains("doesn't match its pin"), "{why}");
        assert!(
            why.contains(&sha256(&s.agent_browser)),
            "names the pinned hash: {why}"
        );
        assert!(why.contains("nothing of it ran"), "{why}");
        let root = s.options.home.join("browser");
        // Nothing of it kept: no file at all under the browser's folder, so nothing to run.
        assert_eq!(files_under(&root), Vec::<PathBuf>::new());
        assert!(!root.join("agent-browser-0.43.0").exists());
        // Chrome wasn't even asked for.
        assert_eq!(s.hits.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_23_a_corrupted_chrome_is_refused_and_not_kept() {
        let home = scratch("corrupt-chrome");
        let s = served(&home, Some("chrome")).await;
        let installer = Arc::new(Installer::new(s.options.clone()));
        let why = installer.ensure().await.unwrap_err();
        assert!(
            why.starts_with("Chrome for Testing 158.0.8127.36's download"),
            "{why}"
        );
        let root = s.options.home.join("browser");
        assert!(!root.join("chrome-158.0.8127.36").exists());
        assert!(
            files_under(&root)
                .iter()
                .all(|f| f.starts_with(root.join("agent-browser-0.43.0"))),
            "{:?}",
            files_under(&root)
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_23_the_right_bytes_are_installed_once_for_two_callers_and_kept_private() {
        let home = scratch("right");
        let s = served(&home, None).await;
        let installer = Arc::new(Installer::new(s.options.clone()));
        let (a, b) = tokio::join!(installer.ensure(), installer.ensure());
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(a, b);
        // One download each, for both callers.
        assert_eq!(s.hits.load(Ordering::SeqCst), 2);
        let root = s.options.home.join("browser");
        assert_eq!(
            a.agent_browser,
            root.join("agent-browser-0.43.0/package/bin/agent-browser")
        );
        assert_eq!(
            a.chrome,
            root.join("chrome-158.0.8127.36/chrome-test/chrome")
        );
        assert_eq!(
            sha256_file(&a.agent_browser).unwrap(),
            sha256(BINARY.as_bytes())
        );
        use std::os::unix::fs::PermissionsExt;
        for dir in [
            root.clone(),
            root.join("agent-browser-0.43.0"),
            root.join("chrome-158.0.8127.36"),
        ] {
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }
        // No half-done folder left.
        assert!(
            std::fs::read_dir(&root)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with('.')),
        );
        // A later obid finds it in place and downloads nothing.
        let again = Arc::new(Installer::new(s.options.clone()));
        assert_eq!(again.ensure().await.unwrap(), a);
        assert_eq!(s.hits.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_23_an_agent_browser_changed_in_place_is_downloaded_again() {
        let home = scratch("changed");
        let s = served(&home, None).await;
        let first = Arc::new(Installer::new(s.options.clone()))
            .ensure()
            .await
            .unwrap();
        std::fs::write(&first.agent_browser, "#!/bin/sh\necho swapped\n").unwrap();
        let again = Arc::new(Installer::new(s.options.clone()));
        let second = again.ensure().await.unwrap();
        assert_eq!(
            sha256_file(&second.agent_browser).unwrap(),
            sha256(BINARY.as_bytes())
        );
        assert_eq!(s.hits.load(Ordering::SeqCst), 3);
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_23_a_computer_with_no_build_is_refused_plainly() {
        let home = scratch("no-build");
        let options = Options::new(
            home.clone(),
            super::super::agent_browser("linux-arm64", "http://127.0.0.1:1"),
            super::super::chrome("linux-arm64", "http://127.0.0.1:1"),
        );
        let why = Arc::new(Installer::new(options))
            .ensure()
            .await
            .unwrap_err();
        assert!(
            why.contains("Chrome for Testing has no build for linux-arm64"),
            "{why}"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn browser_23_a_zip_unpacks_with_ditto() {
        let home = scratch("zip");
        let src = home.join("src/chrome-mac-arm64");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("chrome"), CHROME).unwrap();
        let zip = home.join("c.zip");
        let status = std::process::Command::new("/usr/bin/ditto")
            .args(["-c", "-k"])
            .arg(home.join("src"))
            .arg(&zip)
            .status()
            .unwrap();
        assert!(status.success());
        let into = home.join("into");
        std::fs::create_dir_all(&into).unwrap();
        unpack(Archive::Zip, &zip, &into).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(into.join("chrome-mac-arm64/chrome")).unwrap(),
            CHROME
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// The real pinned agent-browser for this computer, from npm: its tarball and binary match.
    #[tokio::test]
    #[ignore = "downloads 8 MB from npm"]
    async fn browser_23_the_real_agent_browser_matches_its_pin() {
        let home = scratch("real");
        let platform = super::super::this_platform().expect("a platform with a build");
        let ab = super::super::agent_browser(platform, super::super::AGENT_BROWSER_BASE).unwrap();
        let root = home.join("browser");
        make_private_dir(&root).unwrap();
        let exe = one(&reqwest::Client::new(), &root, &ab).await.unwrap();
        assert_eq!(
            sha256_file(&exe).unwrap(),
            ab.executable_sha256.clone().unwrap()
        );
        let _ = std::fs::remove_dir_all(home);
    }
}
