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
/// Node scripts, the fixture's check is `node --test`) and `gitleaks` (the secret scan), each a
/// clone of the one found, and nothing else.
fn tools(run: &Run) -> Result<PathBuf> {
    let bin = run.scratch.join("bin");
    private_dir(&bin)?;
    for name in ["node", "gitleaks"] {
        let link = bin.join(name);
        if link.exists() {
            continue;
        }
        let found = which(name).with_context(|| format!("{name} isn't on PATH"))?;
        let found = std::fs::canonicalize(&found).unwrap_or(found);
        // A copy, cloned on APFS (free on disk), not a link: a job's box reads the tools on its
        // PATH, never where a link points (a check's `node` living under ~/.nvm). `cp -c` is
        // macOS's; elsewhere (Linux CI, no box) the link is enough.
        let cloned = cfg!(target_os = "macos")
            && std::process::Command::new("/bin/cp")
                .arg("-c")
                .arg(&found)
                .arg(&link)
                .status()
                .is_ok_and(|s| s.success());
        if !cloned {
            std::os::unix::fs::symlink(&found, &link)?;
        }
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
    /// Files to put in its home before it starts: relative path → text, or `{"fixture": "<path>"}`
    /// for the text of tools/obid-spec/fixtures/<path> (so `person` and `github` too).
    pub files: Option<serde_json::Map<String, Value>>,
    /// Files to put in its scratch HOME (the person's home, for its stand-ins: `github/work.json`
    /// for the fake gh) before it starts.
    pub person: Option<serde_json::Map<String, Value>>,
    /// GitHub as obid's recording stand-in answers it (core/crates/obid/tests/fakes/gh, its routes
    /// as core/crates/obid/tests/github_recorded/README.md says): put first on its `PATH`, before
    /// the stand-ins' own `gh`, with these files (relative path → text: `routes.json` and what its
    /// routes name) in its folder, `gh-recording` beside the scratch HOME, where it also writes
    /// `calls.jsonl`; the fixture's pushes then get pull requests (`pullRequests`). Never the real
    /// GitHub.
    pub github: Option<serde_json::Map<String, Value>>,
    /// The person's ssh as core/crates/obid/tests/fakes/ssh stands in for it, put first on its
    /// `PATH`, by destination: `{"pairs": <another scratch obid>}` runs that obid's own
    /// `obid pair-computer` (COMPUTERS-25), `{"says": "<stderr>", "exit": <n>}` says what ssh or a
    /// shell says there and exits so. Its tunnels are relayed on this machine; every call is in
    /// `ssh-bin/calls` in its folder. Never the real ssh.
    pub ssh: Option<serde_json::Map<String, Value>>,
    /// A Codex stand-in account for the browser's workflows, whose jobs act as a job's agent.
    /// obid has no browser engine to put in place: its tabs are Oby's window's (the stand-in
    /// window, `browser-window`, in these workflows).
    pub browser: Option<bool>,
    /// A gitleaks that can't run first on its `PATH` (obid's tests' `bin-broken-gitleaks`), so its
    /// secret scan can't run: nothing with text goes out (BROWSER-22, "Non-negotiable").
    #[serde(rename = "brokenScanner")]
    pub broken_scanner: Option<bool>,
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
        // Local-network reach off unless a workflow gives `direct` (COMPUTERS-31): a workflow's
        // obid binds nothing but loopback by itself.
        "direct": [],
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
    // The browser's workflows act as a job's agent: a Codex stand-in account, whose jobs are given
    // Oby's own MCP server with their broker token (browser-job).
    let mut files = input.files.clone().unwrap_or_default();
    if input.browser == Some(true) {
        let codex = json!({"agents": {"codex": {"accounts": [{"id": "codex", "name": "Codex",
            "configDir": home.join("accounts/codex")}]}}});
        merge(&mut config, &codex);
        files
            .entry("accounts/codex/auth.json".to_string())
            .or_insert(json!({"OPENAI_API_KEY": "fake-codex"}));
    }
    let files = Some(files);
    // With GitHub's recording stand-in, the fixture's pushes get their pull requests there.
    if input.github.is_some() {
        config["repos"][0]["pullRequests"] = json!(true);
    }
    if let Some(over) = &input.config {
        merge(&mut config, over);
    }
    // A tool given by a bare name is looked for where the person's real CLIs live: refused.
    for (tool, agent) in config["agents"].as_object().into_iter().flatten() {
        let bin = agent["bin"].as_str().unwrap_or_default();
        anyhow::ensure!(
            Path::new(bin).is_absolute(),
            "agents.{tool}.bin is {bin:?}: a scratch obid takes a stand-in by absolute path only"
        );
    }
    let config_path = folder.join("config.json");
    private_file(&config_path, &serde_json::to_string_pretty(&config)?)?;
    let recording = folder.join("gh-recording");
    let mut path_first = vec![];
    if input.github.is_some() {
        private_dir(&recording)?;
        let bin = folder.join("gh-bin");
        private_dir(&bin)?;
        let _ = std::fs::remove_file(bin.join("gh"));
        let fake = run.agents().with_file_name("fakes").join("gh");
        anyhow::ensure!(fake.is_file(), "no recording gh at {}", fake.display());
        std::os::unix::fs::symlink(&fake, bin.join("gh"))?;
        path_first.push(bin);
    }
    if let Some(hosts) = &input.ssh {
        path_first.push(stand_in_ssh(run, &folder, hosts)?);
    }
    if input.broken_scanner == Some(true) {
        path_first.push(run.agents().with_file_name("bin-broken-gitleaks"));
    }
    for (base, files) in [
        (&home, &files),
        (&person, &input.person),
        (&recording, &input.github),
    ] {
        for (rel, text) in files.iter().flatten() {
            anyhow::ensure!(
                !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
                "{rel} isn't a path inside its folder"
            );
            let path = base.join(rel);
            private_dir(path.parent().unwrap())?;
            let text = match text {
                Value::String(s) => s.clone(),
                // `{"fixture": "<path>"}`: the text of tools/obid-spec/fixtures/<path>.
                Value::Object(o)
                    if o.len() == 1 && o.get("fixture").is_some_and(Value::is_string) =>
                {
                    let rel = o["fixture"].as_str().unwrap_or_default();
                    anyhow::ensure!(
                        !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
                        "{rel} isn't a path inside tools/obid-spec/fixtures"
                    );
                    let file = run.root.join("tools/obid-spec/fixtures").join(rel);
                    std::fs::read_to_string(&file).with_context(|| format!("{}", file.display()))?
                }
                other => other.to_string(),
            };
            private_file(&path, &text)?;
        }
    }
    let path = std::env::join_paths(path_first.into_iter().chain([
        run.agents(),
        tools(run)?,
        "/usr/bin".into(),
        "/bin".into(),
        "/usr/sbin".into(),
        "/sbin".into(),
    ]))?;
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
    // With a `gateway` in its configuration, the second line says where the paired phones' listener is.
    let mut gateway = None;
    if config.get("gateway").is_some() {
        let mut said = String::new();
        let _ = tokio::time::timeout(Duration::from_secs(30), stdout.read_line(&mut said)).await;
        gateway = said
            .trim()
            .strip_prefix("obid gateway listening on http://127.0.0.1:")
            .and_then(|p| p.parse::<u16>().ok())
            .map(|p| format!("http://127.0.0.1:{p}"));
        anyhow::ensure!(
            gateway.is_some(),
            "obid's gateway didn't say where it listens: {:?}",
            said.trim()
        );
    }
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
        "owner": config["owner"], "restarted": restart, "gatewayBase": gateway,

        "github": input.github.is_some().then_some(recording),
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

/// The fake ssh's folder for one scratch obid (StartInput's `ssh`): a link to
/// core/crates/obid/tests/fakes/ssh and what each destination runs there.
fn stand_in_ssh(
    run: &Run,
    folder: &Path,
    hosts: &serde_json::Map<String, Value>,
) -> Result<PathBuf> {
    let bin = folder.join("ssh-bin");
    let _ = std::fs::remove_dir_all(&bin);
    private_dir(&bin.join("hosts"))?;
    let fake = run.agents().with_file_name("fakes").join("ssh");
    anyhow::ensure!(fake.is_file(), "no stand-in ssh at {}", fake.display());
    std::os::unix::fs::symlink(&fake, bin.join("ssh"))?;
    for (destination, host) in hosts {
        anyhow::ensure!(
            !destination.is_empty()
                && destination
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
            "ssh: {destination:?} isn't a plain destination"
        );
        let script = if let Some(obid) = host.get("pairs") {
            let other = obid["folder"]
                .as_str()
                .context("ssh: `pairs` takes a scratch obid (obid-start's answer)")?;
            format!(
                "exec '{}' pair-computer --config '{}'",
                run.obid.display(),
                Path::new(other).join("config.json").display()
            )
        } else {
            let says = host["says"].as_str().unwrap_or_default().replace('\'', "");
            let exit = host["exit"].as_u64().unwrap_or(255);
            format!("echo '{says}' >&2; exit {exit}")
        };
        let path = bin.join("hosts").join(destination);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(bin)
}
