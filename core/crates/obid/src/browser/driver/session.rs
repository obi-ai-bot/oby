//! One job's agent-browser session (BROWSER-24, 25): its `--session` name, `HOME`, profile,
//! downloads folder and socket folder, and one call of agent-browser in it with the lab's exact
//! flags (`--json --session --profile --download-path --no-auto-dialog --idle-timeout`,
//! `--executable-path`), an environment made from scratch, killed at its limit. It owns no state;
//! [`Session::make_folders`] writes its folders and nothing else.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::Call;
use crate::browser::install::Installed;
use crate::store::{keep_private, make_private_dir};

/// A Unix socket's path is at most this many bytes on macOS (104 with its terminating zero).
pub const SOCKET_PATH_MAX: usize = 103;

/// The longest path Chrome makes under its `TMPDIR` for its single-instance socket on Linux
/// (`mkdtemp`'s six characters stand for themselves): too long, and Chrome stops at once with
/// "Socket path too long".
pub const CHROME_SINGLETON: &str = "org.chromium.Chromium.XXXXXX/SingletonSocket";

/// The system's folders only: agent-browser and Chrome need nothing a person's `PATH` adds.
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// One job's agent-browser session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub job: String,
    /// agent-browser's `--session`.
    pub name: String,
    pub bin: PathBuf,
    pub chrome: PathBuf,
    /// The job's own `HOME` for agent-browser and Chrome.
    pub home: PathBuf,
    pub profile: PathBuf,
    pub downloads: PathBuf,
    pub socket_dir: PathBuf,
    pub idle_timeout: String,
    /// The keys the person's Go on anyway trusted (BROWSER-29), each with the host it was for:
    /// the browser runs with them in `--ignore-certificate-errors-spki-list`, and agent-browser
    /// starts it again by itself when they change. Empty for a browser started afresh.
    pub trusted: Vec<(String, String)>,
}

/// A job's id becomes a folder's name: letters, digits, `-` and `_` only.
fn valid_job(job: &str) -> Result<(), String> {
    let ok = !job.is_empty()
        && job.len() <= 64
        && job
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then_some(()).ok_or_else(|| {
        format!("{job:?} can't name a browser: a job's id is letters, digits, - and _")
    })
}

/// A short id for the job under this home: its socket folder's and session's name.
fn short_id(home: &Path, job: &str) -> String {
    let digest = obi_protocol::keys::sha256(format!("{}\n{job}", home.display()).as_bytes());
    hex::encode(&digest[..6])
}

impl Session {
    /// The job's folders under `<home>/browser/jobs/<job>/`.
    pub fn job_dir(home: &Path, job: &str) -> PathBuf {
        home.join("browser").join("jobs").join(job)
    }

    /// The job's downloads, in its profile.
    pub fn downloads_of(home: &Path, job: &str) -> PathBuf {
        Self::job_dir(home, job).join("profile").join("Downloads")
    }

    /// The job's session name.
    pub fn name_of(home: &Path, job: &str) -> String {
        format!("oby-{}", short_id(home, job))
    }

    /// The job's socket folder, which is also Chrome's `TMPDIR` (on Linux Chrome puts its
    /// single-instance socket there): `<home>/b/<id>` when the longest socket path either makes
    /// fits in [`SOCKET_PATH_MAX`], else a private one in `/tmp` named for this user and job.
    pub fn socket_dir_of(home: &Path, job: &str) -> PathBuf {
        let id = short_id(home, job);
        let near = home.join("b").join(&id);
        let agent_browser = near.join(format!("{}.stream", Self::name_of(home, job)));
        let chrome = near.join(CHROME_SINGLETON);
        let longest = agent_browser
            .as_os_str()
            .len()
            .max(chrome.as_os_str().len());
        if longest <= SOCKET_PATH_MAX {
            near
        } else {
            PathBuf::from(format!("/tmp/oby-{}-{id}", nix::unistd::getuid().as_raw()))
        }
    }

    /// The job's session with this engine: paths only, nothing made yet.
    pub fn for_job(
        home: &Path,
        job: &str,
        installed: &Installed,
        idle_timeout: &str,
    ) -> Result<Session, String> {
        valid_job(job)?;
        let dir = Self::job_dir(home, job);
        let profile = dir.join("profile");
        Ok(Session {
            job: job.into(),
            name: Self::name_of(home, job),
            bin: installed.agent_browser.clone(),
            chrome: installed.chrome.clone(),
            home: dir.join("home"),
            downloads: profile.join("Downloads"),
            profile,
            socket_dir: Self::socket_dir_of(home, job),
            idle_timeout: idle_timeout.into(),
            trusted: vec![],
        })
    }

    /// Makes the session's folders, each its owner's alone (0700); a socket folder in `/tmp` that
    /// another user made is refused.
    pub fn make_folders(&self) -> Result<(), String> {
        for dir in [&self.home, &self.downloads, &self.socket_dir] {
            make_private_dir(dir)
                .and_then(|()| keep_private(dir, 0o700))
                .map_err(|e| format!("can't make {}: {e}", dir.display()))?;
        }
        use std::os::unix::fs::MetadataExt;
        let owner = std::fs::metadata(&self.socket_dir)
            .map(|m| m.uid())
            .map_err(|e| format!("can't read {}: {e}", self.socket_dir.display()))?;
        if owner != nix::unistd::getuid().as_raw() {
            return Err(format!(
                "{} belongs to another user: obid won't put the browser's sockets there.",
                self.socket_dir.display()
            ));
        }
        Ok(())
    }

    /// agent-browser's global flags for this session.
    pub fn flags(&self) -> Vec<String> {
        let p = |p: &Path| p.to_string_lossy().into_owned();
        let mut flags: Vec<String> = vec![
            "--json".into(),
            "--session".into(),
            self.name.clone(),
            "--profile".into(),
            p(&self.profile),
            "--download-path".into(),
            p(&self.downloads),
            "--no-auto-dialog".into(),
            "--idle-timeout".into(),
            self.idle_timeout.clone(),
            "--executable-path".into(),
            p(&self.chrome),
        ];
        if let Some(list) = self.spki_list() {
            flags.extend(["--args".into(), list]);
        }
        flags
    }

    /// The launch argument that trusts each key the person's Go on anyway trusted, or None.
    pub fn spki_list(&self) -> Option<String> {
        let mut keys: Vec<&str> = self.trusted.iter().map(|(_, k)| k.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        (!keys.is_empty())
            .then(|| format!("--ignore-certificate-errors-spki-list={}", keys.join(",")))
    }

    /// One agent-browser call in this session, killed when `limit` passes.
    pub async fn call(&self, args: &[String], limit: Duration) -> Call {
        let mut command = tokio::process::Command::new(&self.bin);
        command
            .env_clear()
            .envs(crate::process::kept(|name| std::env::var_os(name)))
            .env("PATH", SYSTEM_PATH)
            .env("HOME", &self.home)
            .env("AGENT_BROWSER_SOCKET_DIR", &self.socket_dir)
            .env("AGENT_BROWSER_NO_VERSION_CHECK", "1")
            // obid's own TMPDIR may be long (a scratch obid's is): Chrome's single-instance
            // socket goes in the job's short socket folder instead.
            .env("TMPDIR", &self.socket_dir);
        command
            .args(self.flags())
            .args(args)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let started = tokio::time::Instant::now();
        let out = match tokio::time::timeout(limit, command.output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Call::Failed(format!("agent-browser didn't start: {e}")),
            Err(_) => return Call::TimedOut(started.elapsed().as_millis() as u64),
        };
        let text = String::from_utf8_lossy(&out.stdout);
        match serde_json::from_str::<Value>(text.trim()) {
            Ok(v) if v["success"] == true => {
                let mut data = v["data"].clone();
                if let Some(o) = data.as_object_mut() {
                    o.remove("lifecycle");
                }
                if let Some(w) = v["warning"].as_str() {
                    data["warning"] = json!(w);
                }
                Call::Ok(data)
            }
            Ok(v) => Call::Failed(
                v["error"]
                    .as_str()
                    .unwrap_or("agent-browser failed")
                    .to_string(),
            ),
            Err(_) => Call::Failed(format!(
                "agent-browser {}: {} {}",
                args.join(" "),
                text.trim().chars().take(300).collect::<String>(),
                String::from_utf8_lossy(&out.stderr)
                    .trim()
                    .chars()
                    .take(300)
                    .collect::<String>()
            )),
        }
    }

    /// The daemon's PID, as agent-browser writes it into the socket folder once it runs.
    pub fn daemon_pid(&self) -> Option<u32> {
        pid_in(&self.socket_dir, &self.name)
    }
}

/// The PID in `<dir>/<name>.pid`, if any.
pub fn pid_in(dir: &Path, name: &str) -> Option<u32> {
    std::fs::read_to_string(dir.join(format!("{name}.pid")))
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|p| *p > 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> Installed {
        Installed {
            agent_browser: "/h/browser/agent-browser-0.43.0/package/bin/agent-browser".into(),
            chrome: "/h/browser/chrome-158.0.8127.36/chrome".into(),
        }
    }

    #[test]
    fn browser_24_each_job_has_its_own_session_profile_downloads_home_and_short_socket_folder() {
        let home = Path::new("/h");
        let s = Session::for_job(home, "job-1", &installed(), "10m").unwrap();
        let other = Session::for_job(home, "job-2", &installed(), "10m").unwrap();
        assert_ne!(s.name, other.name);
        assert_ne!(s.socket_dir, other.socket_dir);
        assert_eq!(s.profile, Path::new("/h/browser/jobs/job-1/profile"));
        assert_eq!(s.downloads, s.profile.join("Downloads"));
        assert_eq!(s.home, Path::new("/h/browser/jobs/job-1/home"));
        assert!(s.socket_dir.starts_with("/h/b"));
        let flags = s.flags().join(" ");
        assert!(
            flags.contains("--no-auto-dialog --idle-timeout 10m --executable-path"),
            "{flags}"
        );
        assert!(flags.contains("--download-path /h/browser/jobs/job-1/profile/Downloads"));
    }

    #[test]
    fn browser_24_a_long_home_puts_the_sockets_in_a_short_private_folder() {
        let home = PathBuf::from(format!("/{}", "x".repeat(90)));
        let s = Session::for_job(&home, "job-1", &installed(), "10m").unwrap();
        assert!(
            s.socket_dir.starts_with("/tmp"),
            "{}",
            s.socket_dir.display()
        );
        let longest = s.socket_dir.join(format!("{}.stream", s.name));
        assert!(longest.as_os_str().len() <= SOCKET_PATH_MAX);
        assert!(s.socket_dir.join(CHROME_SINGLETON).as_os_str().len() <= SOCKET_PATH_MAX);
    }

    /// Chrome's single-instance socket fits too (it goes under the socket folder, Chrome's
    /// TMPDIR): a home short enough for agent-browser's sockets but not for Chrome's moves both.
    #[test]
    fn browser_24_chromes_singleton_socket_fits_under_the_socket_folder() {
        // `<home>/b/<12>/oby-<12>.stream` fits, `<home>/b/<12>/org.chromium…/SingletonSocket` doesn't.
        let home = PathBuf::from(format!("/{}", "y".repeat(50)));
        let s = Session::for_job(&home, "job-1", &installed(), "10m").unwrap();
        assert!(
            s.socket_dir.join(CHROME_SINGLETON).as_os_str().len() <= SOCKET_PATH_MAX,
            "{}",
            s.socket_dir.display()
        );
    }

    #[test]
    fn a_job_id_that_isnt_a_plain_name_never_becomes_a_folder() {
        for bad in ["", "../x", "a/b", "a b", &"x".repeat(65)] {
            assert!(Session::for_job(Path::new("/h"), bad, &installed(), "10m").is_err());
        }
    }
}
