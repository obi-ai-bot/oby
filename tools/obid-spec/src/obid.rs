//! A scratch obid: its own folder, home, port and token, on stand-in agents only. Never the
//! owner's obid on 127.0.0.1:7777, their logins, their real CLIs or their GitHub: its `PATH` holds
//! obid's stand-in agents and fake `gh` (core/crates/obid/tests/agents), `node` and `gitleaks`,
//! and the system's folders, nothing else; its `HOME` is a scratch folder; its Keychain is a
//! stand-in `security`.

use crate::procs;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

/// What the host hands the nodes child about this run (environment `OBID_SPEC_*`).
#[derive(Clone, Debug)]
pub struct Run {
    /// The obid binary built once for the run.
    pub obid: PathBuf,
    /// The repository's root.
    pub root: PathBuf,
    /// This workflow's own folder: every scratch obid and Sphere state goes under it.
    pub scratch: PathBuf,
    /// The run's local Sphere stand-in (`http://127.0.0.1:<port>`) and its operator token, when the
    /// run started one.
    pub sphere_url: Option<String>,
    pub sphere_operator: Option<String>,
}

impl Run {
    pub fn from_env() -> Result<Run> {
        let var = |k: &str| std::env::var(k).with_context(|| format!("{k} isn't set"));
        Ok(Run {
            obid: var("OBID_SPEC_OBID")?.into(),
            root: var("OBID_SPEC_ROOT")?.into(),
            scratch: var("OBID_SPEC_SCRATCH")?.into(),
            sphere_url: std::env::var("OBID_SPEC_SPHERE_URL").ok(),
            sphere_operator: std::env::var("OBID_SPEC_SPHERE_OPERATOR").ok(),
        })
    }
    pub fn agents(&self) -> PathBuf {
        self.root.join("core/crates/obid/tests/agents")
    }
}

pub fn private_dir(path: &Path) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .with_context(|| format!("can't make {}", path.display()))
}

pub fn private_file(path: &Path, text: &str) -> Result<()> {
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    std::io::Write::write_all(&mut file, text.as_bytes())?;
    Ok(())
}

pub fn merge(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(k) {
                    Some(slot) if v.is_object() && slot.is_object() => merge(slot, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, o) => *b = o.clone(),
    }
}

/// The folder of tools a scratch obid may run besides its stand-ins: `node` (the stand-ins are
/// Node scripts) and `gitleaks` (the secret scan), each a link to the one found, and nothing else.
fn tools(run: &Run) -> Result<PathBuf> {
    let bin = run.scratch.join("bin");
    private_dir(&bin)?;
    for name in ["node", "gitleaks"] {
        let link = bin.join(name);
        if link.exists() {
            continue;
        }
        let found = which(name).with_context(|| format!("{name} isn't on PATH"))?;
        std::os::unix::fs::symlink(found, &link)?;
    }
    Ok(bin)
}

pub fn which(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// A bare repository with one commit on `main` (a README and a passing `node --test` file), the
/// scratch obid's one configured repository.
pub fn fixture_repo(folder: &Path) -> Result<PathBuf> {
    let remote = folder.join("remote.git");
    if remote.exists() {
        return Ok(remote);
    }
    let work = folder.join("seed");
    std::fs::create_dir_all(work.join("test"))?;
    std::fs::write(work.join("README.md"), "# fixture-app\n")?;
    std::fs::write(
        work.join("test/ok.test.js"),
        "const test = require('node:test');\ntest('ok', () => {});\n",
    )?;
    let git = |dir: &Path, args: &[&str]| -> Result<()> {
        let out = std::process::Command::new("/usr/bin/git")
            .current_dir(dir)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", folder)
            .env("GIT_AUTHOR_NAME", "obid-spec")
            .env("GIT_AUTHOR_EMAIL", "obid-spec@example.invalid")
            .env("GIT_COMMITTER_NAME", "obid-spec")
            .env("GIT_COMMITTER_EMAIL", "obid-spec@example.invalid")
            .output()?;
        if !out.status.success() {
            bail!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    };
    git(&work, &["init", "-q", "-b", "main"])?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "-q", "-m", "init"])?;
    git(
        folder,
        &[
            "clone",
            "-q",
            "--bare",
            work.to_str().unwrap(),
            "remote.git",
        ],
    )?;
    Ok(remote)
}

/// The input of `obid-start`.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StartInput {
    /// Its folder's name under the workflow's own (default `obid`). Started again with the same
    /// name, it keeps its home: a restart.
    pub name: Option<String>,
    /// Configuration's `owner` (default `alex`).
    pub owner: Option<String>,
    /// The stand-in Claude Code (a file in core/crates/obid/tests/agents, default `fake-harness`).
    pub agent: Option<String>,
    /// Over the configuration this node writes.
    pub config: Option<Value>,
    /// Over the environment this node gives it.
    pub env: Option<serde_json::Map<String, Value>>,
    /// Files to put in its home before it starts: relative path → text.
    pub files: Option<serde_json::Map<String, Value>>,
    /// Files to put in its scratch HOME (the person's home, for its stand-ins: `github/work.json`
    /// for the fake gh) before it starts.
    pub person: Option<serde_json::Map<String, Value>>,
}

pub async fn start(run: &Run, input: StartInput) -> Result<Value> {
    let name = input.name.unwrap_or_else(|| "obid".into());
    anyhow::ensure!(
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "a name is letters, digits and dashes"
    );
    let folder = run.scratch.join(&name);
    let home = folder.join("home");
    let person = folder.join("env-home");
    for dir in [
        &folder,
        &home,
        &person,
        &folder.join("tmp"),
        &folder.join("gh"),
    ] {
        private_dir(dir)?;
    }
    let restart = home.join("api-token").exists();
    let token = if restart {
        std::fs::read_to_string(home.join("api-token"))?
            .trim()
            .to_string()
    } else {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
        let token = hex::encode(bytes);
        private_file(&home.join("api-token"), &format!("{token}\n"))?;
        token
    };
    let account = home.join("accounts/stand-in");
    private_dir(&account)?;
    let login = account.join(".credentials.json");
    if !login.exists() {
        let expires = chrono::Utc::now().timestamp_millis() + 365 * 86_400_000;
        private_file(
            &login,
            &json!({"claudeAiOauth": {"accessToken": "obid-spec-stand-in", "expiresAt": expires}})
                .to_string(),
        )?;
    }
    let remote = fixture_repo(&run.scratch.join("fixture"))?;
    let agent = run
        .agents()
        .join(input.agent.as_deref().unwrap_or("fake-harness"));
    anyhow::ensure!(agent.is_file(), "no stand-in {}", agent.display());
    let mut config = json!({
        "listen": "127.0.0.1:0",
        "home": home,
        "owner": input.owner.as_deref().unwrap_or("alex"),
        "telemetry": false,
        "allowFileRemotes": true,
        // On macOS a sign-in needs it on (SIGN-IN-5); the Keychain obid asks is the stand-in
        // `security` (OBID_SECURITY_BIN), never the person's.
        "claudeKeychain": cfg!(target_os = "macos"),
        "cursorKeychain": false,
        // Every tool pinned to a stand-in by absolute path: obid looks for a tool named bare on
        // its PATH, its login shell's and the usual install folders (`/opt/homebrew/bin` among
        // them), where the person's real CLIs live.
        "agents": {"claude": {"bin": agent,
            "accounts": [{"id": "stand-in", "name": "Stand-in", "configDir": account}]},
            "codex": {"bin": run.agents().join("fake-codex"), "accounts": []},
            "cursor": {"bin": run.agents().join("fake-cursor"), "accounts": []}},
        "repos": [{"repo": "obi-e2e/fixture-app", "remote": format!("file://{}", remote.display()),
                   "defaultBranch": "main", "check": "node --test", "pullRequests": false}],
    });
    if let Some(over) = &input.config {
        merge(&mut config, over);
    }
    let config_path = folder.join("config.json");
    private_file(&config_path, &serde_json::to_string_pretty(&config)?)?;
    for (base, files) in [(&home, &input.files), (&person, &input.person)] {
        for (rel, text) in files.iter().flatten() {
            anyhow::ensure!(
                !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
                "{rel} isn't a path inside its folder"
            );
            let path = base.join(rel);
            private_dir(path.parent().unwrap())?;
            let text = match text {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            private_file(&path, &text)?;
        }
    }
    let path = std::env::join_paths([
        run.agents(),
        tools(run)?,
        "/usr/bin".into(),
        "/bin".into(),
        "/usr/sbin".into(),
        "/sbin".into(),
    ])?;
    // obid reads the owner's login shell's environment for the CLIs it runs (LANES-4): the real
    // one's `/etc/zprofile` puts `/opt/homebrew/bin` and the like on `PATH`, where the person's own
    // claude, codex and gh live. A stand-in shell answers this obid's own `PATH` and nothing else.
    let shell = folder.join("login-shell");
    let script = format!(
        "#!/bin/sh\n# obid-spec's stand-in login shell: the scratch obid's own PATH, nothing of the person's.\nPATH='{}'\nexport PATH\nexec /usr/bin/env -0\n",
        path.to_string_lossy().replace('\'', "")
    );
    let _ = std::fs::remove_file(&shell);
    std::fs::write(&shell, script)?;
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700))?;
    let log = folder.join("obid.log");
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let mut command = tokio::process::Command::new(&run.obid);
    command
        .args(["serve", "--config"])
        .arg(&config_path)
        .env_clear()
        .env("PATH", path)
        .env("HOME", &person)
        .env("USER", "obid-spec")
        .env("LOGNAME", "obid-spec")
        .env("TMPDIR", folder.join("tmp"))
        .env("GH_CONFIG_DIR", folder.join("gh"))
        .env("OBID_LOGIN_SHELL", &shell)
        .env("OBID_LOG", "info")
        .env("OBID_PERSON_HOMES", &person)
        .env("OBID_MEMORY_AVAILABLE", "1099511627776")
        .env("OBID_GITHUB_REFRESH_SECONDS", "0")
        .env("OBID_SECURITY_BIN", run.agents().join("fake-security"))
        .env("OBID_PRESTART", "off")
        .env("OBID_USAGE_READS", "off")
        .env("OBID_LOAD", "0")
        .env("OBID_BASE_CHECK", "off")
        .env("OBID_RECHECK_MS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .process_group(0);
    for (k, v) in input.env.iter().flatten() {
        anyhow::ensure!(
            k.starts_with("OBID_"),
            "only OBID_ settings may be given ({k})"
        );
        command.env(k, v.as_str().map(str::to_string).unwrap_or(v.to_string()));
    }
    let mut child = command.spawn().context("obid didn't start")?;
    let pid = child.id().context("obid ended at once")?;
    procs::remember(pid, &format!("obid {name}"));
    let mut stdout = BufReader::new(child.stdout.take().context("no stdout")?);
    let mut line = String::new();
    let read = tokio::time::timeout(Duration::from_secs(60), stdout.read_line(&mut line)).await;
    let port: Option<u16> = line
        .trim()
        .strip_prefix("obid listening on http://127.0.0.1:")
        .and_then(|p| p.parse().ok());
    let Some(port) = port else {
        procs::stop(pid, Duration::from_secs(5)).await;
        let said = std::fs::read_to_string(&log).unwrap_or_default();
        let tail: Vec<&str> = said.lines().rev().take(15).collect();
        bail!(
            "obid didn't say it listens ({}): {:?}\n{}",
            if read.is_err() { "60 s" } else { "ended" },
            line.trim(),
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    };
    // Its stdout is drained in the background, so it never blocks writing.
    tokio::spawn(async move {
        let mut sink = String::new();
        while stdout.read_line(&mut sink).await.is_ok_and(|n| n > 0) {
            sink.clear();
        }
        let _ = child.wait().await;
    });
    Ok(json!({
        "name": name, "pid": pid, "port": port, "base": format!("http://127.0.0.1:{port}"),
        "token": token, "home": home, "folder": folder, "person": person, "log": log,
        "owner": config["owner"], "restarted": restart,
    }))
}

/// SIGTERM, then SIGKILL after `grace`. Only an obid this suite started.
pub async fn stop(pid: u32, grace: Duration) -> Result<Value> {
    anyhow::ensure!(
        procs::ours(pid),
        "process {pid} isn't one this suite started"
    );
    let gone = procs::stop(pid, grace).await;
    Ok(json!({"pid": pid, "stopped": gone}))
}

pub fn mode_of(path: &Path) -> Option<String> {
    std::fs::metadata(path)
        .ok()
        .map(|m| format!("{:04o}", m.permissions().mode() & 0o7777))
}
