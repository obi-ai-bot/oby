//! The lab's machines (src/lab/mod.rs): a Linux computer of its own per workflow, made with
//! OrbStack and deleted at the workflow's end whatever it ended in. It stands in for "another
//! computer" (a Linux box, a server): isolated by default, so it shares no folder with this Mac,
//! and its user is `oby`, never this Mac's.
//!
//! A machine's name is `obid-lab-<workflow>-<name>`, `<workflow>` from this workflow's scratch
//! folder, so two workflows (or two runs) never share one, and only a name this workflow made is
//! ever deleted. Without OrbStack (not installed, not running) a step answers `skipped`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::obid::Run;

pub const NODES: &[(&str, &str)] = &[
    ("lab.machine.up", "Makes a Linux machine of this workflow's own with OrbStack (ubuntu:noble by default; isolated, user oby), waits until it answers, and remembers it for the cleanup. Skipped without OrbStack."),
    ("lab.machine.run", "Runs a bash command on a machine this workflow made (as oby, or root), answering its exit status, stdout and stderr; with expect."),
    ("lab.machine.down", "Deletes a machine this workflow made, now rather than at the cleanup."),
    ("lab.machine.push", "Copies a file or a folder of this checkout (or of the workflow's own folder) into a machine this workflow made, owned by oby."),
    ("lab.machine.obid-build", "Builds the Linux obid this checkout makes for an arch (this Mac's by default), once per commit and arch, in a machine of its own deleted after, and caches it; its step gives a limit long enough for a cold build."),
    ("lab.machine.obid", "Puts a Linux obid on a machine (built once per commit and arch, in a machine of its own, and cached), with obid's stand-in agents, node and git, and starts it as oby on the machine's own 127.0.0.1: its API and its gateway."),
    ("lab.machine.ssh", "Gives a machine an SSH server and this workflow a key of its own to reach it as oby: a scratch ssh config (its alias, the key, the machine's host key as the only known one); the person's ~/.ssh and agent are never read."),
];

static MADE: Mutex<Vec<String>> = Mutex::new(Vec::new());

const USER: &str = "oby";

/// Why the lab can't make machines here, or None.
async fn unsupported() -> Option<String> {
    match orb(&["status"], Duration::from_secs(10)).await {
        Ok((0, out, _)) if out.trim() == "Running" => None,
        Ok((_, out, err)) => Some(format!(
            "skipped: OrbStack isn't running here ({}{})",
            out.trim(),
            err.trim()
        )),
        Err(e) => Some(format!("skipped: OrbStack isn't installed here ({e:#})")),
    }
}

/// `orb` with these arguments, killed after `limit`: (exit status, stdout, stderr).
async fn orb(args: &[&str], limit: Duration) -> Result<(i32, String, String)> {
    let child = tokio::process::Command::new("orb")
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(limit, child)
        .await
        .with_context(|| format!("orb {} took over {}s", args.join(" "), limit.as_secs()))?
        .context("orb didn't start")?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// `name` as this workflow's machine: `obid-lab-<workflow>-<name>`.
pub fn full_name(run: &Run, name: &str) -> Result<String> {
    anyhow::ensure!(
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "a machine's name is lowercase letters, digits and dashes: {name:?}"
    );
    let workflow: String = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        run.scratch.hash(&mut h);
        format!("{:08x}", h.finish() as u32)
    };
    Ok(format!("obid-lab-{workflow}-{name}"))
}

fn ours(machine: &str) -> bool {
    MADE.lock().unwrap().iter().any(|m| m == machine)
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    if let Some(why) = unsupported().await {
        return Ok(json!({ "skipped": why }));
    }
    match name {
        "lab.machine.up" => up(run, input).await,
        "lab.machine.run" => {
            let machine = input["machine"]["machine"]
                .as_str()
                .or_else(|| input["machine"].as_str())
                .context("machine: a lab.machine.up step's output")?;
            anyhow::ensure!(
                ours(machine),
                "{machine} isn't a machine this workflow made"
            );
            let command = input["command"].as_str().context("command")?;
            let user = if input["root"] == true { "root" } else { USER };
            let limit = Duration::from_secs(input["limit"].as_u64().unwrap_or(600));
            let (status, stdout, stderr) = orb(
                &["run", "-m", machine, "-u", user, "bash", "-lc", command],
                limit,
            )
            .await?;
            Ok(json!({ "status": status, "stdout": stdout, "stderr": stderr }))
        }
        "lab.machine.down" => {
            let machine = input["machine"]["machine"]
                .as_str()
                .or_else(|| input["machine"].as_str())
                .context("machine: a lab.machine.up step's output")?;
            anyhow::ensure!(
                ours(machine),
                "{machine} isn't a machine this workflow made"
            );
            delete(machine).await?;
            Ok(json!({ "deleted": machine }))
        }
        "lab.machine.push" => {
            let machine = machine_of(input)?;
            let from = std::path::PathBuf::from(input["from"].as_str().context("from")?);
            let from = if from.is_absolute() {
                from
            } else {
                run.root.join(from)
            };
            let to = input["to"].as_str().context("to: a path on the machine")?;
            push(run, &machine, &from, to).await
        }
        "lab.machine.obid-build" => {
            // A cold build takes longer than a step's default 300 s: its step gives a `limit`
            // (src/main.rs, `step_limit`).
            let arch = input["arch"].as_str().unwrap_or(std::env::consts::ARCH);
            let (binary, built) = linux_obid(run, arch).await?;
            Ok(json!({ "built": built,
                "binary": binary.file_name().map(|n| n.to_string_lossy().into_owned()) }))
        }
        "lab.machine.obid" => obid(run, input).await,
        "lab.machine.ssh" => ssh(run, input).await,
        other => bail!("{other} isn't a machine node"),
    }
}

fn machine_of(input: &Value) -> Result<String> {
    let machine = input["machine"]["machine"]
        .as_str()
        .or_else(|| input["machine"].as_str())
        .context("machine: a lab.machine.up step's output")?;
    anyhow::ensure!(
        ours(machine),
        "{machine} isn't a machine this workflow made"
    );
    Ok(machine.to_string())
}

/// `orb` with these arguments and `input` on its stdin.
async fn orb_with(args: &[&str], input: Vec<u8>, limit: Duration) -> Result<(i32, String, String)> {
    use tokio::io::AsyncWriteExt as _;
    let mut child = tokio::process::Command::new("orb")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("orb didn't start")?;
    let mut stdin = child.stdin.take().context("no stdin")?;
    let feed = tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
    });
    let out = tokio::time::timeout(limit, child.wait_with_output())
        .await
        .with_context(|| format!("orb {} took over {}s", args.join(" "), limit.as_secs()))??;
    let _ = feed.await;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// A script as oby (or root) on `machine`, read from stdin (`bash -s`): no command line carries
/// it, so a `pkill -f` in it never matches its own shell.
pub async fn run_script(machine: &str, script: &str, root: bool) -> Result<(i32, String, String)> {
    let user = if root { "root" } else { USER };
    orb_with(
        &["run", "-m", machine, "-u", user, "bash", "-s"],
        script.as_bytes().to_vec(),
        Duration::from_secs(600),
    )
    .await
}

/// A script on a machine this workflow made (`machine`: a lab.machine.up step's output), for the
/// other lab features.
pub async fn script_on(machine: &Value, script: &str, root: bool) -> Result<(i32, String, String)> {
    let machine = machine_of(&json!({ "machine": machine }))?;
    run_script(&machine, script, root).await
}

/// `command` as oby on `machine`, with `input` on its stdin.
async fn run_with(
    machine: &str,
    command: &str,
    input: Vec<u8>,
    root: bool,
) -> Result<(i32, String, String)> {
    let user = if root { "root" } else { USER };
    orb_with(
        &["run", "-m", machine, "-u", user, "bash", "-c", command],
        input,
        Duration::from_secs(600),
    )
    .await
}

/// A path the lab may copy from: this checkout, the workflow's own folder or the lab's cache.
fn may_copy(run: &Run, from: &Path) -> Result<PathBuf> {
    let from =
        std::fs::canonicalize(from).with_context(|| format!("{} isn't there", from.display()))?;
    let allowed = [run.root.clone(), run.scratch.clone(), cache_dir()]
        .into_iter()
        .filter_map(|p| std::fs::canonicalize(p).ok());
    for base in allowed {
        if from.starts_with(&base) {
            return Ok(from);
        }
    }
    bail!(
        "{} is outside this checkout and the workflow's folder",
        from.display()
    )
}

/// A path on the machine: absolute, or under oby's home; plain characters only.
fn machine_path(to: &str) -> Result<String> {
    anyhow::ensure!(
        !to.is_empty()
            && to
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
            && !to.split('/').any(|p| p == ".."),
        "{to:?} isn't a plain path"
    );
    Ok(if to.starts_with('/') {
        to.to_string()
    } else {
        format!("/home/{USER}/{to}")
    })
}

async fn push(run: &Run, machine: &str, from: &Path, to: &str) -> Result<Value> {
    let from = may_copy(run, from)?;
    let to = machine_path(to)?;
    let (bytes, command) = if from.is_dir() {
        let tar = tokio::process::Command::new("/usr/bin/tar")
            .arg("-C")
            .arg(&from)
            .args(["--no-xattrs", "-cf", "-", "."])
            .env("COPYFILE_DISABLE", "1")
            .output()
            .await?;
        anyhow::ensure!(
            tar.status.success(),
            "tar: {}",
            String::from_utf8_lossy(&tar.stderr)
        );
        (
            tar.stdout,
            format!("mkdir -p '{to}' && tar --warning=no-unknown-keyword -C '{to}' -xf -"),
        )
    } else {
        let mode =
            std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&from)?.permissions());
        (
            tokio::fs::read(&from).await?,
            format!("mkdir -p \"$(dirname '{to}')\" && cat > '{to}.part' && chmod {:o} '{to}.part' && mv '{to}.part' '{to}'", mode & 0o755),
        )
    };
    let size = bytes.len();
    let (status, out, err) = run_with(machine, &command, bytes, false).await?;
    anyhow::ensure!(
        status == 0,
        "push to {machine}:{to} failed ({status}): {out}{err}"
    );
    Ok(json!({"machine": machine, "to": to, "bytes": size}))
}

// ------------------------------------------------------------------- a Linux obid

/// Where Linux obids built for the lab are kept, by commit and arch, across runs.
fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("obid-spec-lab")
}

/// Which obid this checkout builds: the tree of `core/` at HEAD, and what's changed in it since.
fn build_key(run: &Run) -> Result<String> {
    let git = |args: &[&str]| -> Result<Vec<u8>> {
        let out = std::process::Command::new("/usr/bin/git")
            .current_dir(&run.root)
            .args(args)
            .output()?;
        anyhow::ensure!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(out.stdout)
    };
    let tree = String::from_utf8(git(&["rev-parse", "HEAD:core"])?)?
        .trim()
        .to_string();
    let mut changed = git(&["diff", "HEAD", "--binary", "--", "core"])?;
    let untracked = git(&[
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        "core",
    ])?;
    for file in untracked.split(|b| *b == 0).filter(|f| !f.is_empty()) {
        changed.extend_from_slice(file);
        changed.extend(
            std::fs::read(run.root.join(String::from_utf8_lossy(file).as_ref()))
                .unwrap_or_default(),
        );
    }
    if changed.is_empty() {
        return Ok(tree[..12].to_string());
    }
    use sha2::Digest as _;
    Ok(format!(
        "{}-{}",
        &tree[..12],
        &hex::encode(sha2::Sha256::digest(&changed))[..12]
    ))
}

/// A Linux obid for `arch` (`aarch64`, `x86_64`): from the cache, or built now in a machine of
/// its own (not isolated: it reads this checkout) that's deleted once the binary is out. Two
/// workflows that need one at once build it once: the second waits on the first's lock.
pub async fn linux_obid(run: &Run, arch: &str) -> Result<(PathBuf, bool)> {
    let orb_arch = match arch {
        "aarch64" | "arm64" => "arm64",
        "x86_64" | "amd64" => "amd64",
        other => bail!("no Linux obid for {other}"),
    };
    let dir = cache_dir();
    std::fs::create_dir_all(&dir)?;
    let key = build_key(run)?;
    let binary = dir.join(format!("obid-linux-{orb_arch}-{key}"));
    let lock = dir.join(format!("obid-linux-{orb_arch}-{key}.lock"));
    let until = std::time::Instant::now() + Duration::from_secs(1800);
    loop {
        if binary.is_file() {
            return Ok((binary, false));
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(mut file) => {
                // The lock names the process building: when it's gone (killed mid-build), the
                // next run takes the lock over at once.
                std::io::Write::write_all(&mut file, std::process::id().to_string().as_bytes())?;
                break;
            }
            Err(_) => {
                let holder: Option<i32> = std::fs::read_to_string(&lock)
                    .ok()
                    .and_then(|t| t.trim().parse().ok());
                let gone = holder.is_some_and(|pid| unsafe { libc::kill(pid, 0) } != 0);
                let old = std::fs::metadata(&lock)
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| t.elapsed().is_ok_and(|e| e > Duration::from_secs(2400)));
                let stale = gone || old;
                if stale {
                    let _ = std::fs::remove_file(&lock);
                    continue;
                }
                if std::time::Instant::now() >= until {
                    bail!(
                        "another run's Linux obid build held {} for 30 minutes",
                        lock.display()
                    );
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    let built = build(run, orb_arch, &binary).await;
    let _ = std::fs::remove_file(&lock);
    built.map(|()| (binary, true))
}

async fn build(run: &Run, orb_arch: &str, binary: &Path) -> Result<()> {
    let machine = full_name(run, &format!("build-{orb_arch}"))?;
    MADE.lock().unwrap().push(machine.clone());
    let made = async {
        let (status, out, err) = orb(
            &["create", "-u", USER, "-a", orb_arch, "ubuntu:noble", &machine],
            Duration::from_secs(600),
        )
        .await?;
        anyhow::ensure!(status == 0, "orb create {machine}: {out}{err}");
        let core = run.root.join("core");
        let script = format!(
            r#"set -eu
export DEBIAN_FRONTEND=noninteractive
sudo -E apt-get update -qq >/dev/null
sudo -E apt-get install -y -qq build-essential pkg-config git cmake >/dev/null
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null 2>&1
. "$HOME/.cargo/env"
cd '{}'
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_TARGET_DIR="$HOME/target" cargo build -q -p obid --bin obid
"#,
            core.display()
        );
        let (status, out, err) = orb(&["run", "-m", &machine, "-u", USER, "bash", "-c", &script], Duration::from_secs(1800)).await?;
        anyhow::ensure!(status == 0, "building obid for Linux failed ({status}): {}{}", out, err.chars().rev().take(3000).collect::<String>().chars().rev().collect::<String>());
        let part = binary.with_extension("part");
        let file = std::fs::File::create(&part)?;
        let copied = tokio::process::Command::new("orb")
            .args(["run", "-m", &machine, "-u", USER, "cat", "/home/oby/target/debug/obid"])
            .stdout(file)
            .status()
            .await?;
        anyhow::ensure!(copied.success(), "copying obid out of {machine}");
        std::fs::set_permissions(&part, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
        std::fs::rename(&part, binary)?;
        Ok(())
    }
    .await;
    // The build machine goes at once, whatever happened: it holds a toolchain and a target.
    let _ = delete(&machine).await;
    made
}

/// Node as CI runs it (22), for `os` (`linux`, `darwin`) and `arch`: nodejs.org's own build, fetched once into the lab's cache
/// and checked against nodejs.org's SHASUMS256.txt.
pub async fn node_tarball(os: &str, arch: &str) -> Result<PathBuf> {
    anyhow::ensure!(os == "linux" || os == "darwin", "no Node for {os}");
    const VERSION: &str = "v22.12.0";
    let arch = match arch {
        "aarch64" | "arm64" => "arm64",
        "x86_64" | "amd64" => "x64",
        other => bail!("no Node for {other}"),
    };
    let name = format!("node-{VERSION}-{os}-{arch}.tar.gz");
    let dir = cache_dir();
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(&name);
    if file.is_file() {
        return Ok(file);
    }
    let base = format!("https://nodejs.org/dist/{VERSION}");
    let sums = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "-fsSL",
            "--max-time",
            "60",
            &format!("{base}/SHASUMS256.txt"),
        ])
        .output()
        .await?;
    anyhow::ensure!(
        sums.status.success(),
        "nodejs.org's SHASUMS256.txt: {}",
        String::from_utf8_lossy(&sums.stderr)
    );
    let sums = String::from_utf8_lossy(&sums.stdout);
    let want = sums
        .lines()
        .find(|l| l.ends_with(&format!("  {name}")))
        .and_then(|l| l.split_whitespace().next())
        .with_context(|| format!("{name} isn't in nodejs.org's SHASUMS256.txt"))?
        .to_string();
    let part = dir.join(format!("{name}.part-{}", std::process::id()));
    let got = tokio::process::Command::new("/usr/bin/curl")
        .args(["-fsSL", "--max-time", "300", "-o"])
        .arg(&part)
        .arg(format!("{base}/{name}"))
        .status()
        .await?;
    anyhow::ensure!(got.success(), "downloading {name}");
    use sha2::Digest as _;
    let have = hex::encode(sha2::Sha256::digest(std::fs::read(&part)?));
    if have != want {
        let _ = std::fs::remove_file(&part);
        bail!("{name}: SHA-256 {have}, nodejs.org says {want}");
    }
    std::fs::rename(&part, &file)?;
    Ok(file)
}

/// The ports a lab obid listens on, on its machine's own 127.0.0.1.
const API: &str = "127.0.0.1:7811";
const GATEWAY: &str = "127.0.0.1:8810";
const OBY: &str = "/home/oby/.oby";

async fn obid(run: &Run, input: &Value) -> Result<Value> {
    let machine = machine_of(input)?;
    let (_, arch, _) = orb(
        &["run", "-m", &machine, "-u", USER, "uname", "-m"],
        Duration::from_secs(30),
    )
    .await?;
    // From the cache once a lab.machine.obid-build step made it; else built now, within this
    // step's own limit.
    let (binary, _) = linux_obid(run, arch.trim()).await?;
    let (status, out, err) = run_with(
        &machine,
        "export DEBIAN_FRONTEND=noninteractive; command -v git >/dev/null && command -v bwrap >/dev/null || { apt-get update -qq >/dev/null && apt-get install -y -qq git bubblewrap >/dev/null; }",
        vec![],
        true,
    )
    .await?;
    anyhow::ensure!(status == 0, "git and bubblewrap on {machine}: {out}{err}");
    // The stand-in agents are Node scripts written for CI's Node 22; Ubuntu's own is 18.
    let node = node_tarball("linux", arch.trim()).await?;
    push(run, &machine, &node, "/home/oby/.oby/node.tar.gz").await?;
    let (status, out, err) = run_with(
        &machine,
        "tar -C /usr/local --strip-components=1 -xzf /home/oby/.oby/node.tar.gz && rm /home/oby/.oby/node.tar.gz && /usr/local/bin/node -v",
        vec![],
        true,
    )
    .await?;
    anyhow::ensure!(status == 0, "node on {machine}: {out}{err}");
    push(run, &machine, &binary, &format!("{OBY}/bin/obid")).await?;
    push(run, &machine, &run.agents(), &format!("{OBY}/agents")).await?;
    let owner = input["owner"].as_str().unwrap_or("alex");
    let agent = input["agent"].as_str().unwrap_or("fake-harness-jobs");
    anyhow::ensure!(
        [owner, agent]
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')),
        "owner and agent are plain names"
    );
    let mut config = json!({
        "listen": API, "gateway": GATEWAY, "home": format!("{OBY}/home"), "owner": owner,
        "telemetry": false, "allowFileRemotes": true, "claudeKeychain": false, "cursorKeychain": false,
        "agents": {"claude": {"bin": format!("{OBY}/agents/{agent}"),
            "accounts": [{"id": "stand-in", "name": "Stand-in", "configDir": format!("{OBY}/home/accounts/stand-in")}]},
            "codex": {"bin": format!("{OBY}/agents/fake-codex"), "accounts": []},
            "cursor": {"bin": format!("{OBY}/agents/fake-cursor"), "accounts": []}},
        "repos": [{"repo": "obi-e2e/fixture-app", "remote": "file:///home/oby/fixture/remote.git",
            "defaultBranch": "main", "check": "node --test", "pullRequests": false}],
    });
    if let Some(over) = input.get("config") {
        crate::obid::merge(&mut config, over);
    }
    let expires = chrono::Utc::now().timestamp_millis() + 365 * 86_400_000;
    let login =
        json!({"claudeAiOauth": {"accessToken": "obid-spec-stand-in", "expiresAt": expires}});
    let path = format!("{OBY}/agents:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin");
    let script = format!(
        r#"set -eu
umask 077
mkdir -p {OBY}/home/accounts/stand-in {OBY}/tmp /home/oby/fixture
cat > {OBY}/home/accounts/stand-in/.credentials.json <<'OBY_JSON'
{login}
OBY_JSON
cat > {OBY}/config.json <<'OBY_JSON'
{config}
OBY_JSON
cat > {OBY}/pair-computer <<'OBY_PAIR'
{pair}OBY_PAIR
chmod 700 {OBY}/pair-computer
cat > {OBY}/login-shell <<'OBY_SH'
#!/bin/sh
PATH='{path}'
export PATH
exec /usr/bin/env -0
OBY_SH
chmod 700 {OBY}/login-shell
if [ ! -d /home/oby/fixture/remote.git ]; then
  cd /home/oby/fixture && rm -rf seed && mkdir -p seed/test && cd seed
  printf '# fixture-app\n' > README.md
  printf "const test = require('node:test');\ntest('ok', () => {{}});\n" > test/ok.test.js
  git init -q -b main && git add . && git -c user.name=obid-spec -c user.email=obid-spec@example.invalid commit -q -m init
  cd .. && git clone -q --bare seed remote.git
fi
pkill -f '/home/oby/.oby/bin/obid serve' || true
setsid nohup env -i PATH='{path}' HOME=/home/oby USER=oby LOGNAME=oby TMPDIR={OBY}/tmp \
  OBID_LOGIN_SHELL={OBY}/login-shell OBID_LOG=info OBID_PERSON_HOMES=/home/oby \
  OBID_MEMORY_AVAILABLE=1099511627776 OBID_GITHUB_REFRESH_SECONDS=0 OBID_PRESTART=off \
  OBID_USAGE_READS=off OBID_LOAD=0 OBID_BASE_CHECK=off OBID_RECHECK_MS=0 \
  {OBY}/bin/obid serve --config {OBY}/config.json > {OBY}/obid.out 2> {OBY}/obid.log < /dev/null &
for i in $(seq 1 300); do grep -q 'gateway listening' {OBY}/obid.out 2>/dev/null && break; sleep 0.1; done
cat {OBY}/obid.out
grep -q 'gateway listening' {OBY}/obid.out || {{ tail -20 {OBY}/obid.log; exit 1; }}
"#,
        config = serde_json::to_string_pretty(&config)?,
        pair = super::remote::computers::pair_computer_script(
            &format!("{OBY}/home"),
            API,
            &format!("http://{GATEWAY}"),
        )
        .map_err(anyhow::Error::msg)?,
    );
    let (status, out, err) = run_script(&machine, &script, false).await?;
    anyhow::ensure!(
        status == 0,
        "obid didn't start on {machine} ({status}): {out}{err}"
    );
    Ok(json!({
        "machine": machine, "api": API, "gateway": GATEWAY, "home": format!("{OBY}/home"),
        "config": format!("{OBY}/config.json"), "pairCommand": format!("{OBY}/pair-computer"), "log": format!("{OBY}/obid.log"), "owner": owner,
        "binary": binary.file_name().map(|n| n.to_string_lossy().into_owned()),
        "said": out.trim(),
    }))
}

// ------------------------------------------------------------------- SSH

/// An SSH server on the machine, and a key and config of this workflow's own to reach it: the
/// machine's host key is read from the machine itself (out of band, as the person would check
/// it) into a scratch known_hosts, so the first connection is already strict.
async fn ssh(run: &Run, input: &Value) -> Result<Value> {
    let machine = machine_of(input)?;
    let alias = input["alias"].as_str().unwrap_or("lab-box");
    anyhow::ensure!(
        alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !alias.starts_with('-'),
        "alias: letters, digits and dashes"
    );
    let (status, out, err) = run_with(
        &machine,
        r#"set -eu
export DEBIAN_FRONTEND=noninteractive
command -v sshd >/dev/null || { apt-get update -qq >/dev/null && apt-get install -y -qq openssh-server >/dev/null; }
mkdir -p /etc/ssh/sshd_config.d
printf 'PasswordAuthentication no\nKbdInteractiveAuthentication no\nPermitRootLogin no\n' > /etc/ssh/sshd_config.d/oby-lab.conf
systemctl enable --now ssh >/dev/null 2>&1 || systemctl restart ssh
systemctl is-active ssh >/dev/null || systemctl is-active ssh.socket >/dev/null
cut -d' ' -f1,2 /etc/ssh/ssh_host_ed25519_key.pub
hostname -I | cut -d' ' -f1
"#,
        vec![],
        true,
    )
    .await?;
    anyhow::ensure!(status == 0, "sshd on {machine} ({status}): {out}{err}");
    let mut lines = out.lines();
    let host_key = lines.next().context("no host key")?.trim().to_string();
    let ip = lines.next().context("no address")?.trim().to_string();
    let access = ssh_access(run, alias, &ip, USER, &host_key)?;
    let (status, out, err) = run_with(
        &machine,
        "umask 077; mkdir -p ~/.ssh && cat >> ~/.ssh/authorized_keys",
        access.public.clone().into_bytes(),
        false,
    )
    .await?;
    anyhow::ensure!(status == 0, "authorized_keys on {machine}: {out}{err}");
    let (config, home, known) = (&access.config, &access.home, &access.known);
    Ok(
        json!({"machine": machine, "alias": alias, "config": config, "home": home,
        "knownHosts": known, "host": ip, "user": USER, "hostKey": host_key}),
    )
}

/// A workflow's own way into an SSH host: an ed25519 key made for it, and a scratch ssh config
/// and known_hosts naming `alias` (and `<alias>-unknown`, the same host under a host key name
/// nobody accepted: what a host the person never checked looks like). Run ssh with `-F config`
/// and `HOME=home`, so nothing of the person's (`~/.ssh`, their agent) is read.
pub struct SshAccess {
    pub config: PathBuf,
    pub home: PathBuf,
    pub known: PathBuf,
    /// The key's public line, for the host's authorized_keys.
    pub public: String,
}

pub fn ssh_access(
    run: &Run,
    alias: &str,
    host: &str,
    user: &str,
    host_key: &str,
) -> Result<SshAccess> {
    anyhow::ensure!(
        [alias, host, user].iter().all(|s| !s.is_empty()
            && !s.starts_with('-')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-:".contains(c))),
        "alias, host and user are plain names"
    );
    let dir = run.scratch.join(format!("ssh-{alias}"));
    let home = dir.join("home");
    crate::obid::private_dir(&home)?;
    let key = dir.join("id_ed25519");
    if !key.exists() {
        let made = std::process::Command::new("/usr/bin/ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "obid-spec-lab", "-f"])
            .arg(&key)
            .output()?;
        anyhow::ensure!(
            made.status.success(),
            "ssh-keygen: {}",
            String::from_utf8_lossy(&made.stderr)
        );
    }
    let public = std::fs::read_to_string(dir.join("id_ed25519.pub"))?;
    let known = dir.join("known_hosts");
    crate::obid::private_file(&known, &format!("{alias} {host_key}\n"))?;
    let config = dir.join("config");
    let entry = |name: &str| {
        format!(
            "Host {name}\n  HostName {host}\n  User {user}\n  IdentityFile {key}\n  IdentitiesOnly yes\n  IdentityAgent none\n  HostKeyAlias {name}\n  UserKnownHostsFile {known}\n  GlobalKnownHostsFile /dev/null\n  StrictHostKeyChecking yes\n  UpdateHostKeys no\n  PasswordAuthentication no\n  KbdInteractiveAuthentication no\n",
            key = key.display(),
            known = known.display(),
        )
    };
    crate::obid::private_file(
        &config,
        &format!("{}\n{}", entry(alias), entry(&format!("{alias}-unknown"))),
    )?;
    Ok(SshAccess {
        config,
        home,
        known,
        public,
    })
}

async fn up(run: &Run, input: &Value) -> Result<Value> {
    let machine = full_name(run, input["name"].as_str().unwrap_or("box"))?;
    let distro = input["distro"].as_str().unwrap_or("ubuntu:noble");
    let mut args = vec!["create", "-u", USER];
    if input["isolated"] != false {
        args.push("--isolated");
    }
    if let Some(arch) = input["arch"].as_str() {
        args.extend(["-a", arch]);
    }
    args.extend([distro, machine.as_str()]);
    // Remembered before it exists, so a create that half-made it is still deleted.
    MADE.lock().unwrap().push(machine.clone());
    let (status, out, err) = orb(&args, Duration::from_secs(600)).await?;
    anyhow::ensure!(
        status == 0,
        "orb create failed ({status}): {}{}",
        out.trim(),
        err.trim()
    );
    let until = std::time::Instant::now() + Duration::from_secs(120);
    let arch = loop {
        match orb(
            &["run", "-m", &machine, "-u", USER, "uname", "-m"],
            Duration::from_secs(30),
        )
        .await
        {
            Ok((0, arch, _)) => break arch.trim().to_string(),
            _ if std::time::Instant::now() >= until => bail!("{machine} didn't answer within 120s"),
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    };
    let (_, address, _) = orb(
        &[
            "run",
            "-m",
            &machine,
            "-u",
            USER,
            "bash",
            "-c",
            "hostname -I | cut -d' ' -f1",
        ],
        Duration::from_secs(30),
    )
    .await?;
    Ok(
        json!({ "machine": machine, "distro": distro, "arch": arch, "user": USER, "address": address.trim() }),
    )
}

async fn delete(machine: &str) -> Result<()> {
    let (status, out, err) = orb(&["delete", "-f", machine], Duration::from_secs(120)).await?;
    anyhow::ensure!(
        status == 0,
        "orb delete {machine} failed ({status}): {}{}",
        out.trim(),
        err.trim()
    );
    MADE.lock().unwrap().retain(|m| m != machine);
    Ok(())
}

/// Deletes every machine this workflow made and hasn't deleted yet, after what runs against
/// them in this process (the remote feature's tunnels and supervisors) has stopped.
pub async fn cleanup() -> Value {
    let remote = super::remote::cleanup().await;
    let made: Vec<String> = MADE.lock().unwrap().clone();
    let mut deleted = vec![];
    for m in made {
        match delete(&m).await {
            Ok(()) => deleted.push(json!(m)),
            Err(e) => deleted.push(json!({ "machine": m, "error": format!("{e:#}") })),
        }
    }
    json!({"machines": deleted, "remote": remote})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(scratch: &str) -> Run {
        Run {
            obid: "obid".into(),
            root: ".".into(),
            scratch: scratch.into(),
            sphere_url: None,
            sphere_operator: None,
        }
    }

    #[test]
    fn a_machine_is_named_for_its_workflow_and_two_workflows_never_share_one() {
        let a = full_name(&run("/tmp/obid-spec-1/w-a"), "box").unwrap();
        let b = full_name(&run("/tmp/obid-spec-1/w-b"), "box").unwrap();
        assert!(a.starts_with("obid-lab-") && a.ends_with("-box"), "{a}");
        assert_ne!(a, b);
        assert!(full_name(&run("/tmp/x"), "Box!").is_err());
        assert!(full_name(&run("/tmp/x"), "").is_err());
    }

    #[test]
    fn only_a_machine_this_workflow_made_is_ever_touched() {
        assert!(!ours("some-other-machine"));
    }
}
