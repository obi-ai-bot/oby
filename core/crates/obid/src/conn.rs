//! One agent process speaking newline-delimited JSON over its stdio, in its own process group: what
//! every adapter runs on (Claude Code's stream-json, Codex's app-server, ACP). obid writes lines to
//! its stdin and reads its stdout split on `\n` only (a JSON string may hold U+2028), sends its
//! stderr to the job's log, and ends it with everything it started. A turn has a time limit (a
//! [`Limit`]: at most so long, and, for a job's turn, no longer than so long without progress) and
//! a stop signal: [`Conn::next`] returns [`Cut::TimedOut`] or [`Cut::Stopped`] when one comes first.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::watch;

use crate::agent::Launch;
use crate::process;

/// Why a turn's messages ended before the turn did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// The agent closed its output: it exited, or is exiting.
    Closed,
    /// The turn ran past its time.
    TimedOut,
    /// obid was told to stop it.
    Stopped,
}

/// How many quiet windows in a row a change in the worktree, CPU its processes used, or a background
/// task still running (`Conn::holding`), alone stands for progress: a quiet build is, a loop
/// touching a file or spinning, or a server that never ends, isn't.
pub const FILE_WINDOWS: u32 = 4;

/// How long after a sign of progress the CPU its quiet window starts from is read (a quarter of a
/// shorter window): by then the agent has gone quiet, so what it used talking isn't counted.
const CPU_SETTLE: Duration = Duration::from_secs(1);

/// The nice a job's agent and everything it starts run at (PUBLISHING-30).
const AGENT_NICE: i32 = 10;

/// Something outside the agent the turn waits on (an Apple check on a Mac): while it says so, the
/// turn isn't quiet, as while it waits for the owner.
pub type Waiting = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

/// How long a turn may take: `most` at all, and, with `quiet`, no longer than that without a line
/// from the agent, a wait (`waiting`) or, for [`FILE_WINDOWS`] windows at most, a change in `watch`
/// (a job's worktree) or CPU its processes used: a job runs as long as it makes progress
/// (docs/runner-api.md, `limit`). A plain `Duration` is a fixed limit.
#[derive(Clone)]
pub struct Limit {
    pub most: Duration,
    pub quiet: Option<Duration>,
    pub watch: Option<PathBuf>,
    pub waiting: Option<Waiting>,
}

impl From<Duration> for Limit {
    fn from(most: Duration) -> Limit {
        Limit {
            most,
            quiet: None,
            watch: None,
            waiting: None,
        }
    }
}

/// Something in `dir` (files, folders, its git folder too) changed after `since`, and not after
/// now (a file dated ahead is no progress): what the agent did without a word, like a quiet build
/// writing its output. Links aren't followed.
// ponytail: a full walk, once per quiet window and stopping at the first change; a file watcher if
// worktrees get too big for that.
pub fn changed_since(dir: &Path, since: SystemTime) -> bool {
    let now = SystemTime::now();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.modified().is_ok_and(|m| m > since && m <= now) {
                return true;
            }
            if meta.is_dir() && !meta.file_type().is_symlink() {
                folders.push(entry.path());
            }
        }
    }
    false
}

/// How long after a sign of progress a quiet window's CPU is read: [`CPU_SETTLE`], or a quarter of a
/// shorter window.
fn settle(quiet: Option<Duration>) -> Duration {
    quiet.map_or(CPU_SETTLE, |quiet| CPU_SETTLE.min(quiet / 4))
}

/// A JSON-RPC error the agent answered with.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// A running agent and its stdio.
pub struct Conn {
    child: Child,
    pgid: u32,
    /// An own run's commands' temp folder, its working folder, its `login` folder (`env.sh`'s)
    /// and its tag, what its leftovers are found by when it ends ([`process::end_sandboxed`],
    /// LANES-16).
    leftovers: Option<(PathBuf, PathBuf, PathBuf, PathBuf)>,
    stdin: Option<ChildStdin>,
    lines: BufReader<ChildStdout>,
    copying: Option<tokio::task::JoinHandle<()>>,
    stop: watch::Receiver<process::Signal>,
    give_up_at: std::pin::Pin<Box<tokio::time::Sleep>>,
    /// When the turn is quiet for too long (`Limit::quiet`), and what else counts as progress.
    quiet: Option<Duration>,
    quiet_at: std::pin::Pin<Box<tokio::time::Sleep>>,
    watch: Option<PathBuf>,
    waiting: Option<Waiting>,
    /// The last sign of progress, and the quiet windows since the agent's last line that only a
    /// change in `watch` filled.
    heard_at: SystemTime,
    file_windows: u32,
    /// The CPU time the agent's processes had used when the quiet window began, in nanoseconds,
    /// once read; `cpu_due` while it waits to be read, at `cpu_at` (`CPU_SETTLE`).
    cpu: Option<u64>,
    cpu_due: bool,
    cpu_at: std::pin::Pin<Box<tokio::time::Sleep>>,
    buffer: Vec<u8>,
    next_id: u64,
    /// Every message obid writes carries `"jsonrpc": "2.0"` (ACP does; Codex's app-server leaves
    /// it out, and Claude Code's stream-json isn't JSON-RPC).
    jsonrpc: bool,
    pub stopped: bool,
    pub timed_out: bool,
    /// It timed out for being quiet (`Limit::quiet`), not at its most.
    pub quiet_out: bool,
    /// Its last quiet window had no change in `watch` and no background task held, and its CPU
    /// could be read: the agent is idle (hung, RECOVERY-12) or only spinning (RECOVERY-16), not
    /// working without a word.
    pub idle_out: bool,
    /// The agent ended its turn and waits for a background task of its own that is still running
    /// (crate::agent): the task stands for progress as a change in `watch` does, for
    /// [`FILE_WINDOWS`] windows at most without a line (a server or a `--watch` never ends).
    pub holding: bool,
    /// Its processes count as its job's while it runs (crate::meter).
    _metered: crate::meter::Root,
    /// A job's run: recorded while it runs, for the sweep at obid's next start (crate::groups).
    _recorded: Option<crate::groups::Recorded>,
}

impl Conn {
    /// Starts `launch` with its own empty environment plus what it names, in a process group of its
    /// own. `limit` is how long the turn may take from now.
    pub async fn spawn(
        launch: &Launch,
        limit: impl Into<Limit>,
        stop: watch::Receiver<process::Signal>,
        jsonrpc: bool,
    ) -> std::io::Result<Conn> {
        // The agent's stderr goes to obid, which writes it to the log: a file of obid's home would
        // be one the sandbox hides from the agent, and some tools refuse to start with such a stderr.
        let mut log = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&launch.stderr)
            .await?;
        let (program, args) = launch
            .argv
            .split_first()
            .ok_or_else(|| std::io::Error::other("nothing to run"))?;
        let mut command = tokio::process::Command::new(program);
        command
            .args(args)
            .env_clear()
            .envs(launch.env.iter().map(|(k, v)| (k, v)))
            .current_dir(&launch.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(group) = &launch.group {
            crate::groups::take(group)
                .await
                .map_err(std::io::Error::other)?;
            // PUBLISHING-30: a job's agent, and all it starts, runs below obid's checks, which
            // decide publishing. Its first program, `obid __session`, sets it before anything runs.
            command.env(process::NICE, AGENT_NICE.to_string());
        }
        // No process group set here: the command line is the job sandbox's (`Sandbox::wrap`), whose
        // first program starts a session of its own, and so a process group whose id is the child's.
        let mut child = command.spawn()?;
        let pgid = child.id().unwrap_or(0);
        let recorded = match &launch.group {
            Some(group) => Some(crate::groups::record(group, pgid).await),
            None => None,
        };
        let stdin = child.stdin.take();
        let copying = child.stderr.take().map(|mut stderr| {
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut stderr, &mut log).await;
            })
        });
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("no stdout"))?;
        let stopped = *stop.borrow() != process::Signal::Run;
        let limit = limit.into();
        let set = |name: &str| {
            let value = launch.env.iter().rev().find(|(set, _)| set == name);
            value.map(|(_, value)| PathBuf::from(value))
        };
        let leftovers = launch.own_sandbox.then(|| {
            let login = set("CLAUDE_ENV_FILE")?.parent()?.to_path_buf();
            let tag = set(process::RUN_TAG)?;
            Some((set(process::CLAUDE_TMPDIR)?, launch.cwd.clone(), login, tag))
        });
        Ok(Conn {
            child,
            pgid,
            leftovers: leftovers.flatten(),
            stdin,
            lines: BufReader::new(stdout),
            copying,
            stop,
            give_up_at: Box::pin(tokio::time::sleep(limit.most)),
            quiet: limit.quiet,
            quiet_at: Box::pin(tokio::time::sleep(limit.quiet.unwrap_or(limit.most))),
            watch: limit.watch,
            waiting: limit.waiting,
            heard_at: SystemTime::now(),
            file_windows: 0,
            cpu: None,
            cpu_due: true,
            cpu_at: Box::pin(tokio::time::sleep(settle(limit.quiet))),
            buffer: Vec::new(),
            next_id: 0,
            jsonrpc,
            stopped,
            timed_out: false,
            quiet_out: false,
            idle_out: false,
            holding: false,
            _metered: crate::meter::root(pgid, &launch.cwd),
            _recorded: recorded,
        })
    }

    /// The agent's process id.
    pub fn pid(&self) -> u32 {
        self.pgid
    }

    /// A running agent's next turn: `limit` from now, and `stop` its stop signal.
    pub fn renew(&mut self, limit: impl Into<Limit>, stop: watch::Receiver<process::Signal>) {
        let limit = limit.into();
        self.give_up_at
            .as_mut()
            .reset(tokio::time::Instant::now() + limit.most);
        self.quiet = limit.quiet;
        self.watch = limit.watch;
        self.waiting = limit.waiting;
        self.heard();
        self.stopped = *stop.borrow() != process::Signal::Run;
        self.stop = stop;
        self.timed_out = false;
        self.quiet_out = false;
        self.idle_out = false;
        self.holding = false;
    }

    /// A sign of progress from the agent (or a wait): the quiet window starts again.
    fn heard(&mut self) {
        self.file_windows = 0;
        self.cpu_due = true;
        self.cpu_at
            .as_mut()
            .reset(tokio::time::Instant::now() + settle(self.quiet));
        self.rearm();
    }

    /// The quiet window starts again.
    fn rearm(&mut self) {
        self.heard_at = SystemTime::now();
        if let Some(quiet) = self.quiet {
            self.quiet_at
                .as_mut()
                .reset(tokio::time::Instant::now() + quiet);
        }
    }

    /// Whether the agent's processes used CPU over the quiet window just ended: at least a
    /// thousandth of one core over it (an idle event loop's timers use less), or a busy process
    /// ended. `None` when it couldn't be read, or the window's start wasn't. The next window starts
    /// from now.
    async fn cpu_used(&mut self) -> Option<bool> {
        let now = crate::meter::group_cpu(self.pgid).await;
        let since = if self.cpu_due { None } else { self.cpu };
        self.cpu = now;
        self.cpu_due = false;
        let least = self.quiet.unwrap_or_default().as_nanos() / 1000;
        Some(u128::from(now?.abs_diff(since?)) >= least.max(1))
    }

    /// Nothing waits to be read and the agent hasn't closed its output, which is how a running
    /// agent stays between turns: looked at without waiting. A line it wrote while idle would be
    /// taken for the next turn's.
    pub async fn quiet(&mut self) -> bool {
        self.buffer.is_empty()
            && self.lines.buffer().is_empty()
            && tokio::time::timeout(Duration::ZERO, self.lines.fill_buf())
                .await
                .is_err()
    }

    /// The next JSON message, or why none comes. Lines that aren't JSON are skipped.
    pub async fn next(&mut self) -> Result<Value, Cut> {
        loop {
            if self.stopped {
                return Err(Cut::Stopped);
            }
            if self.timed_out {
                return Err(Cut::TimedOut);
            }
            tokio::select! {
                read = self.lines.read_until(b'\n', &mut self.buffer) => {
                    match read {
                        Ok(0) | Err(_) => return Err(Cut::Closed),
                        Ok(_) => self.heard(),
                    }
                    // Cleared only once its line is whole: a branch below that fires while a line
                    // is half read leaves those bytes here for the next read to finish.
                    let message = serde_json::from_slice::<Value>(&self.buffer);
                    self.buffer.clear();
                    if let Ok(message) = message {
                        return Ok(message);
                    }
                }
                () = &mut self.give_up_at => {
                    self.timed_out = true;
                }
                () = &mut self.cpu_at, if self.cpu_due && self.quiet.is_some() => {
                    self.cpu_due = false;
                    self.cpu = crate::meter::group_cpu(self.pgid).await;
                }
                () = &mut self.quiet_at, if self.quiet.is_some() => {
                    let since = self.heard_at;
                    let counts = self.file_windows < FILE_WINDOWS;
                    // Both read past the cap too: a spinning agent, or one writing, isn't idle.
                    let changed = match self.watch.clone() {
                        Some(dir) => {
                            tokio::task::spawn_blocking(move || changed_since(&dir, since))
                                .await
                                .unwrap_or(false)
                        }
                        _ => false,
                    };
                    let busy = self.cpu_used().await;
                    if self.waiting.as_ref().is_some_and(|waiting| waiting()) {
                        self.heard();
                    } else if counts && (changed || busy == Some(true) || self.holding) {
                        self.file_windows += 1;
                        self.rearm();
                    } else {
                        self.timed_out = true;
                        self.quiet_out = true;
                        // Idle, or spinning past the cap (RECOVERY-16); CPU that couldn't be read
                        // is neither.
                        self.idle_out = !changed && !self.holding && busy.is_some();
                    }
                }
                changed = self.stop.changed() => {
                    if changed.is_err() || *self.stop.borrow() != process::Signal::Run {
                        self.stopped = true;
                    }
                }
            }
        }
    }

    /// The answer `ruling` comes to: now, or once the owner gives it. While obid waits for them the
    /// turn's clock stands still (its time limit moves on by the wait), and a stop ends the wait.
    /// A question nobody will answer any more (obid dropped it) is a no.
    pub async fn settle(
        &mut self,
        ruling: crate::agent::Ruling,
    ) -> Result<crate::agent::Answer, Cut> {
        use crate::agent::{Answer, Ruling};
        let mut asked = match ruling {
            Ruling::Now(answer) => return Ok(answer),
            Ruling::Ask(asked) => asked,
        };
        let deadline = self.give_up_at.deadline();
        let since = tokio::time::Instant::now();
        let answer = loop {
            if self.stopped {
                return Err(Cut::Stopped);
            }
            tokio::select! {
                answer = &mut asked => {
                    break answer.unwrap_or_else(|_| Answer::Deny(
                        "Nobody answered that: obid let the question go.".into(),
                    ));
                }
                changed = self.stop.changed() => {
                    if changed.is_err() || *self.stop.borrow() != process::Signal::Run {
                        self.stopped = true;
                    }
                }
            }
        };
        self.give_up_at.as_mut().reset(deadline + since.elapsed());
        // Waiting for the owner isn't being quiet.
        self.heard();
        Ok(answer)
    }

    /// Writes one message as a line. An agent that stopped reading says so through its exit.
    pub async fn send(&mut self, message: &Value) {
        let mut message = message.clone();
        if self.jsonrpc
            && let Some(object) = message.as_object_mut()
        {
            object.insert("jsonrpc".into(), json!("2.0"));
        }
        let Ok(mut bytes) = serde_json::to_vec(&message) else {
            return;
        };
        bytes.push(b'\n');
        if let Some(input) = self.stdin.as_mut() {
            let _ = input.write_all(&bytes).await;
            let _ = input.flush().await;
        }
    }

    /// Sends a request and returns its id; the answer comes back through [`Conn::next`].
    pub async fn request(&mut self, method: &str, params: Value) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"id": id, "method": method, "params": params}))
            .await;
        id
    }

    /// A notification: nothing answers it.
    pub async fn notify(&mut self, method: &str, params: Option<Value>) {
        let message = match params {
            Some(params) => json!({"method": method, "params": params}),
            None => json!({"method": method}),
        };
        self.send(&message).await;
    }

    /// Answers the agent's own request `id`.
    pub async fn reply(&mut self, id: &Value, result: Value) {
        self.send(&json!({"id": id, "result": result})).await;
    }

    /// Refuses the agent's own request `id`, so it never waits on obid.
    pub async fn refuse(&mut self, id: &Value) {
        self.send(&json!({"id": id, "error": {"code": -32601, "message": "obid doesn't answer this request."}}))
            .await;
    }

    /// Reads what the agent still says until it closes its output, for 10 seconds at most: after
    /// its turn, with stdin closed, it is exiting.
    pub async fn drain(&mut self) {
        self.drain_within(Duration::from_secs(10)).await;
    }

    /// [`Conn::drain`], for `limit` at most.
    pub async fn drain_within(&mut self, limit: Duration) {
        let _ = tokio::time::timeout(limit, async { while self.next().await.is_ok() {} }).await;
    }

    /// No more input: an agent that waits for it exits once it's idle.
    pub fn close_input(&mut self) {
        drop(self.stdin.take());
    }

    /// Ends the agent and waits for it: at once when the turn was stopped or ran out of time, else
    /// once it exits by itself (10 seconds at most). Everything left of its group goes too. Returns
    /// how it ended: `exit status 0`, `killed by signal 9`.
    pub async fn finish(self) -> String {
        self.finish_within(Duration::from_secs(10)).await
    }

    /// [`Conn::finish`], with `wait` for the agent to exit by itself.
    pub async fn finish_within(mut self, wait: Duration) -> String {
        self.close_input();
        if self.timed_out || self.stopped {
            process::end_group(self.pgid, process::STOPPING).await;
        }
        let status = match tokio::time::timeout(wait, self.child.wait()).await {
            Ok(status) => status.ok(),
            Err(_) => {
                process::end_group(self.pgid, process::STOPPING).await;
                let _ = self.child.start_kill();
                self.child.wait().await.ok()
            }
        };
        process::signal_group(self.pgid, nix::sys::signal::Signal::SIGKILL);
        if let Some(copying) = self.copying.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), copying).await;
        }
        status
            .map(process::describe)
            .unwrap_or_else(|| "ended".into())
    }
}

/// A turn dropped without [`Conn::finish`] (its job's task panicked, #914) leaves nothing running:
/// its whole group goes, as finish's end does, not only the agent `kill_on_drop` ends; and, for an
/// own run, whatever its commands started that left the group (LANES-16): a job's at its turn's
/// end, a conversation's when its waiting session ends.
impl Drop for Conn {
    fn drop(&mut self) {
        process::signal_group(self.pgid, nix::sys::signal::Signal::SIGKILL);
        self.end_leftovers();
    }
}

impl Conn {
    /// Ends now what an own run's commands left running (LANES-16), and never again for this run
    /// once a sweep proved none is left; one that couldn't is logged, and the drop tries again.
    pub fn end_leftovers(&mut self) {
        let Some((temp, room, login, tag)) = &self.leftovers else {
            return;
        };
        match process::end_sandboxed(temp, room, login, tag) {
            Ok(_) => {
                let _ = std::fs::remove_file(tag);
                self.leftovers = None;
            }
            Err(why) => tracing::warn!("an own run's leftovers may still run: {why}"),
        }
    }

    /// An own run's agent, and all its group, end now, then what its commands left running
    /// (LANES-16): a conversation's waiting session, before a next session starts on the same
    /// folders. Nothing of the agent's can start another command after it, and its drop sweeps
    /// nothing, even when this sweep couldn't finish (it's logged, and the run's tag stays, so its
    /// conversation's folders do too). A boxed run's agent isn't touched: it ends by itself
    /// ([`Conn::finish`]).
    pub fn end_own_now(&mut self) {
        // Taken whatever the sweep finds: the drop, later, may run while a next session works on
        // the same folders, and must never sweep them. Its tag stays when it couldn't finish.
        let Some((temp, room, login, tag)) = self.leftovers.take() else {
            return;
        };
        process::signal_group(self.pgid, nix::sys::signal::Signal::SIGKILL);
        let _ = self.child.start_kill();
        match process::end_sandboxed(&temp, &room, &login, &tag) {
            Ok(_) => drop(std::fs::remove_file(&tag)),
            Err(why) => tracing::warn!("an ended session's leftovers may still run: {why}"),
        }
    }
}

/// The answer to request `id` in `message`, when it is one: its result, or the error it carries.
pub fn answer_to(message: &Value, id: u64) -> Option<Result<Value, RpcError>> {
    if message.get("method").is_some() || message.get("id").and_then(Value::as_u64) != Some(id) {
        return None;
    }
    Some(match message.get("error") {
        Some(error) if !error.is_null() => Err(RpcError {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("refused")
                .to_owned(),
        }),
        _ => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
    })
}

/// The agent's own request (it carries a method and an id): its id.
pub fn request_id(message: &Value) -> Option<&Value> {
    message.get("method")?;
    message.get("id").filter(|id| !id.is_null())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file written in the folder, however deep, is progress; nothing written isn't.
    #[test]
    fn a_change_in_the_worktree_is_progress() {
        let dir = std::env::temp_dir().join(format!("obid-changed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();
        let since = SystemTime::now();
        assert!(!changed_since(&dir, since + Duration::from_secs(1)));
        std::thread::sleep(Duration::from_millis(20));
        let since = SystemTime::now();
        std::thread::sleep(Duration::from_millis(20));
        assert!(!changed_since(&dir, since));
        std::fs::write(dir.join("target/debug/out.o"), "x").unwrap();
        assert!(changed_since(&dir, since));
        // A file dated ahead (a `touch -d` loop) is no progress.
        let since = SystemTime::now() + Duration::from_secs(1);
        let ahead = std::fs::File::create(dir.join("ahead")).unwrap();
        ahead
            .set_modified(SystemTime::now() + Duration::from_secs(3600))
            .unwrap();
        assert!(!changed_since(&dir, since));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECOVERY-12: a message the agent writes in pieces, with the CPU read firing between them,
    /// arrives whole.
    #[tokio::test]
    async fn recovery_12_a_message_split_across_reads_arrives_whole() {
        let dir = std::env::temp_dir().join(format!("obid-split-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let launch = crate::agent::Launch {
            argv: [
                "/bin/sh",
                "-c",
                r#"printf '{"a":'; sleep 1; printf '"b","c"'; sleep 1; printf ':1}\n'"#,
            ]
            .map(Into::into)
            .into(),
            env: vec![("PATH".into(), "/bin:/usr/bin".into())],
            cwd: dir.clone(),
            stderr: dir.join("stderr"),
            own_sandbox: false,
            group: None,
        };
        let limit = Limit {
            quiet: Some(Duration::from_secs(3)),
            ..Duration::from_secs(30).into()
        };
        let (_stop, stop) = watch::channel(process::Signal::Run);
        let mut conn = Conn::spawn(&launch, limit, stop, false).await.unwrap();
        assert_eq!(conn.next().await.unwrap(), json!({"a": "b", "c": 1}));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LANES-16: an own run's process that left its group with `setsid` ends when its Conn does
    /// (a job's turn's end, a conversation's waiting session's, a stop or a drop), once: what the
    /// next session on the same folders runs after that isn't touched.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn lanes_16_an_own_runs_leftover_ends_with_its_conn() {
        let dir = std::env::temp_dir().join(format!("obid-lanes-16c-{}", uuid::Uuid::new_v4()));
        let [temp, room, login] = ["temp", "room", "login"].map(|d| dir.join(d));
        for folder in [&temp, &room, &login] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let [temp, room, login] = [temp, room, login].map(|d| d.canonicalize().unwrap());
        let env_file = login.join("env.sh");
        std::fs::write(&env_file, "").unwrap();
        // As Claude Code's own sandbox for the run: its temp folder and room, nothing else of it.
        // The run's tag, a file only its sandbox may write (crate::jobs, `own_sandbox_env`).
        let tag = temp.with_file_name(format!(
            "{}.run-test",
            temp.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&tag, "").unwrap();
        let profile = format!(
            "(version 1)(allow default)(deny file-write* (subpath \"{}\"))(allow file-write* (subpath \"{}\"))(allow file-write* (subpath \"{}\"))(allow file-write* (literal \"{}\"))",
            dir.canonicalize().unwrap().display(),
            temp.display(),
            room.display(),
            tag.display()
        );
        let pid_file = temp.join("pid");
        // What the test starts, killed however it ends (crate::process::tests::Started).
        let mut started = crate::process::tests::Started {
            children: Vec::new(),
            file: Some(pid_file.clone()),
        };
        let script = format!(
            "/usr/bin/sandbox-exec -p '{profile}' /usr/bin/perl -e 'use POSIX; setsid(); exec q(sleep), 60' &\necho $! > '{}'\nsleep 0.5\necho '{{}}'\nexec sleep 60\n",
            pid_file.display()
        );
        let launch = crate::agent::Launch {
            argv: ["/bin/sh".into(), "-c".into(), script.into()].into(),
            env: vec![
                ("PATH".into(), "/bin:/usr/bin".into()),
                (process::CLAUDE_TMPDIR.into(), temp.clone().into()),
                (process::RUN_TAG.into(), tag.clone().into()),
                ("CLAUDE_ENV_FILE".into(), env_file.into()),
            ],
            cwd: room.clone(),
            stderr: dir.join("stderr"),
            own_sandbox: true,
            group: None,
        };
        let (_stop, stop) = watch::channel(process::Signal::Run);
        let mut conn = Conn::spawn(&launch, Duration::from_secs(30), stop, false)
            .await
            .unwrap();
        assert_eq!(conn.next().await.unwrap(), json!({}));
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // Running: there, and not a zombie its parent (the agent, still running) hasn't reaped.
        let alive = || {
            let ps = std::process::Command::new("/bin/ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .unwrap();
            let stat = String::from_utf8_lossy(&ps.stdout);
            !stat.trim().is_empty() && !stat.trim().starts_with('Z')
        };
        assert!(alive());
        // A waiting conversation's session ends its leftovers as it ends (`Warm::end`), before
        // the next session on the same folders starts; its drop later spares that one's commands.
        // A sweep that can't prove which processes are the run's (its `login` folder gone)
        // ends nothing, and the run stays to be swept again.
        std::fs::rename(&login, dir.join("away")).unwrap();
        conn.end_leftovers();
        assert!(
            conn.leftovers.is_some(),
            "a sweep that proved nothing forgot its run"
        );
        assert!(alive());
        std::fs::rename(dir.join("away"), &login).unwrap();
        conn.end_leftovers();
        assert!(conn.leftovers.is_none());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!alive(), "the leftover outlived its run");
        let next = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "/usr/bin/perl", "-e"])
            .arg("use POSIX; setsid(); exec q(sleep), 60")
            .spawn()
            .unwrap();
        started.children.push(next);
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(conn);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let ended = started.children[0].try_wait().unwrap();
        assert!(ended.is_none(), "the next session's command ended");
        drop(started);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LANES-16: a waiting session ended (`end_own_now`) with a sweep that couldn't finish never
    /// sweeps again when it drops, after a next session started on the same folders: that one's
    /// command survives.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn lanes_16_an_ended_session_never_sweeps_the_next_ones_folders() {
        let dir = std::env::temp_dir().join(format!("obid-lanes-16e-{}", uuid::Uuid::new_v4()));
        let [temp, room, login] = ["temp", "room", "login"].map(|d| dir.join(d));
        for folder in [&temp, &room, &login] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let [temp, room, login] = [temp, room, login].map(|d| d.canonicalize().unwrap());
        std::fs::write(login.join("env.sh"), "").unwrap();
        // The run's tag, a file only its sandbox may write (crate::jobs, `own_sandbox_env`).
        let tag = temp.with_file_name(format!(
            "{}.run-test",
            temp.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&tag, "").unwrap();
        let profile = format!(
            "(version 1)(allow default)(deny file-write* (subpath \"{}\"))(allow file-write* (subpath \"{}\"))(allow file-write* (subpath \"{}\"))(allow file-write* (literal \"{}\"))",
            dir.canonicalize().unwrap().display(),
            temp.display(),
            room.display(),
            tag.display()
        );
        let mut started = crate::process::tests::Started {
            children: Vec::new(),
            file: None,
        };
        let launch = crate::agent::Launch {
            argv: ["/bin/sh", "-c", "echo '{}'; exec sleep 60"]
                .map(Into::into)
                .into(),
            env: vec![
                ("PATH".into(), "/bin:/usr/bin".into()),
                (process::CLAUDE_TMPDIR.into(), temp.clone().into()),
                (process::RUN_TAG.into(), tag.clone().into()),
                ("CLAUDE_ENV_FILE".into(), login.join("env.sh").into()),
            ],
            cwd: room.clone(),
            stderr: dir.join("stderr"),
            own_sandbox: true,
            group: None,
        };
        let (_stop, stop) = watch::channel(process::Signal::Run);
        let mut conn = Conn::spawn(&launch, Duration::from_secs(30), stop, false)
            .await
            .unwrap();
        assert_eq!(conn.next().await.unwrap(), json!({}));
        // Its sweep can't finish (its `login` folder away for the moment).
        std::fs::rename(&login, dir.join("away")).unwrap();
        conn.end_own_now();
        std::fs::rename(dir.join("away"), &login).unwrap();
        // The next session's command, on the same folders, then the old session's drop.
        let next = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "/usr/bin/perl", "-e"])
            .arg("use POSIX; setsid(); exec q(sleep), 60")
            .spawn()
            .unwrap();
        started.children.push(next);
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(conn);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let ended = started.children[0].try_wait().unwrap();
        assert!(
            ended.is_none(),
            "the old session's drop ended the next one's command"
        );
        drop(started);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn answers_and_requests() {
        assert_eq!(
            answer_to(&json!({"id": 3, "result": {"a": 1}}), 3),
            Some(Ok(json!({"a": 1})))
        );
        assert_eq!(answer_to(&json!({"id": 4, "result": {}}), 3), None);
        assert_eq!(
            answer_to(&json!({"id": 3, "method": "x", "params": {}}), 3),
            None
        );
        assert_eq!(
            answer_to(
                &json!({"id": 3, "error": {"code": -32000, "message": "Authentication required"}}),
                3
            ),
            Some(Err(RpcError {
                code: -32000,
                message: "Authentication required".into()
            }))
        );
        assert_eq!(
            request_id(&json!({"id": "a", "method": "session/request_permission"})),
            Some(&json!("a"))
        );
        assert_eq!(request_id(&json!({"method": "session/update"})), None);
        assert_eq!(request_id(&json!({"id": 1, "result": {}})), None);
    }
}
