//! The recorded baseline's comparison (README.md beside this file): what a test's obid asked the
//! recording `gh` (tests/fakes/gh), and what obid answered with GitHub's data in it, against the
//! files kept here. A difference fails the test and prints, line by line, what changed.
//!
//! `GITHUB_RECORDED=bless` writes the files instead of comparing: for a change meant to change
//! what obid asks GitHub or shows of it, and only then (README.md, "Re-recording").

use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Map, Value};

use crate::support::{Folder, Obid, config, start, stub_agent};

/// This crate's folder, where `cargo test` runs obid: the working folder of every `gh` call obid
/// doesn't give one.
fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap()
}

/// Where a path's routes, fixtures and recordings are kept.
pub fn kept(path: &str) -> PathBuf {
    crate_dir().join("tests/github_recorded").join(path)
}

/// The folder to put first on obid's `PATH`: the recording `gh`.
pub fn bin() -> PathBuf {
    crate_dir().join("tests/fakes")
}

/// A scratch obid for a recorded path: the support harness's configuration with the owner
/// `o-owner`, the recording `gh` first on its `PATH`, the routes and fixtures kept for `path` in
/// place.
pub fn recorded_obid(name: &str, path: &str) -> (Folder, Recording, Obid, Value) {
    let (folder, recording, config) = recorded_config(name, path);
    let obid = start(&folder.0, &config, Some(&bin()));
    recording.started(&obid);
    (folder, recording, obid, config)
}

/// [`recorded_obid`]'s folder, recording and configuration, for a test that starts obid itself.
pub fn recorded_config(name: &str, path: &str) -> (Folder, Recording, Value) {
    let folder = Folder::new(&format!("github-recorded-{name}"));
    let recording = Recording::new(&folder, path);
    let remote = folder.0.join("remote.git");
    let mut config = config(&folder.0, &remote, &stub_agent());
    config["owner"] = "o-owner".into();
    (folder, recording, config)
}

/// One path's recording, in its test's folder (`<folder>/gh-recording`, where tests/fakes/gh finds
/// it next to obid's HOME).
pub struct Recording {
    pub path: String,
    pub dir: PathBuf,
    folder: PathBuf,
    pid: std::cell::Cell<Option<u32>>,
    commits: std::cell::RefCell<Vec<String>>,
    compared: std::cell::Cell<bool>,
}

/// A test that fails before it compares (it waited for a state obid never reached, because a call
/// changed and no route answered it) says which calls no route answered, as they were made: what
/// obid asked GitHub that it didn't ask when this path was recorded.
impl Drop for Recording {
    fn drop(&mut self) {
        if !std::thread::panicking() || self.compared.get() {
            return;
        }
        let text = std::fs::read_to_string(self.dir.join("calls.jsonl")).unwrap_or_default();
        let unanswered: Vec<String> = text
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|call| call.get("args").is_some() && call["route"].is_null())
            .map(|call| {
                let args: Vec<String> = call["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| self.commits(&ids(&self.paths(a.as_str().unwrap()))))
                    .collect();
                format!("  gh {}", args.join(" "))
            })
            .collect();
        if !unanswered.is_empty() {
            eprintln!(
                "{}: calls no route in {} answered (obid's other paths' reads at its start among them, and any call this path didn't make when it was recorded):\n{}",
                self.path,
                kept(&self.path).join("routes.json").display(),
                unanswered.join("\n")
            );
        }
    }
}

impl Recording {
    /// The routes and fixture files kept for `path`, copied into the test's folder.
    pub fn new(folder: &Folder, path: &str) -> Recording {
        let dir = folder.0.join("gh-recording");
        std::fs::create_dir_all(dir.join("flags")).unwrap();
        for entry in std::fs::read_dir(kept(path)).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // The recordings themselves aren't the fake's to read.
            if name == "calls.json" || name.starts_with("answers") {
                continue;
            }
            std::fs::copy(entry.path(), dir.join(&*name)).unwrap();
        }
        Recording {
            path: path.to_owned(),
            dir,
            folder: folder.0.clone(),
            pid: std::cell::Cell::new(None),
            commits: std::cell::RefCell::new(Vec::new()),
            compared: std::cell::Cell::new(false),
        }
    }

    /// The obid whose calls these are: its process id names some of its temporary folders.
    pub fn started(&self, obid: &Obid) {
        self.pid.set(Some(obid.child.id()));
    }

    /// Raises the flag file `name` a route waits for (`waitFor`) or depends on (`when`, `unless`).
    pub fn raise(&self, name: &str) {
        std::fs::write(self.dir.join("flags").join(name), "").unwrap();
    }

    /// Whether every call so far whose arguments start with `prefix` has ended: a test that waits
    /// for one to end before asking obid again keeps its own reads out of that call's time.
    pub fn ended(&self, prefix: &str) -> bool {
        let text = std::fs::read_to_string(self.dir.join("calls.jsonl")).unwrap_or_default();
        let lines: Vec<Value> = text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let started: Vec<&str> = lines
            .iter()
            .filter(|l| {
                l["args"].as_array().is_some_and(|args| {
                    let joined: Vec<&str> = args.iter().filter_map(Value::as_str).collect();
                    joined.join(" ").starts_with(prefix)
                })
            })
            .filter_map(|l| l["id"].as_str())
            .collect();
        !started.is_empty()
            && started
                .iter()
                .all(|id| lines.iter().any(|l| l["end"].as_str() == Some(id)))
    }

    /// Every call so far, as tests/fakes/gh wrote it down, made comparable ([`normalize_call`]).
    pub fn calls(&self) -> Vec<Value> {
        let text = std::fs::read_to_string(self.dir.join("calls.jsonl")).unwrap_or_default();
        let lines: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let calls = lines
            .into_iter()
            .filter(|l| l.get("args").is_some())
            .map(|l| self.normalize_call(l))
            .collect();
        polls_once(calls)
    }

    /// The calls whose arguments start with one of `prefixes` (each joined by spaces): a path's
    /// own calls, apart from those obid's other loops make while it runs.
    pub fn calls_starting(&self, prefixes: &[&str]) -> Vec<Value> {
        let calls = self
            .calls()
            .into_iter()
            .filter(|call| {
                let joined = call["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(" ");
                prefixes.iter().any(|p| joined.starts_with(p))
            })
            .collect();
        polls_once(calls)
    }

    /// `calls` against the kept `calls.json`.
    pub fn assert_calls(&self, calls: &[Value]) {
        self.compared.set(true);
        compare(
            &kept(&self.path).join("calls.json"),
            &Value::Array(calls.to_vec()),
        );
    }

    /// `answers` (obid's answers carrying GitHub data, by what was asked) against the kept
    /// `answers.json`, made comparable ([`normalize_answer`]) first.
    pub fn assert_answers(&self, answers: &Value) {
        let made = self.normalize_answer(answers.clone(), None);
        compare(&kept(&self.path).join("answers.json"), &made);
    }

    /// One call, comparable. Only what differs from run to run, by the test's folder or obid's
    /// clock or randomness, is replaced; arguments, URLs, file texts and keys stay as they were.
    fn normalize_call(&self, mut call: Value) -> Value {
        // The fake's own bookkeeping, not what obid asked.
        let map = call.as_object_mut().unwrap();
        for key in ["route", "id", "start"] {
            map.remove(key);
        }
        // Rule: a working folder in the test's folder or the crate's is said relative to it; the
        // folder itself is a temporary one, different on every run.
        if let Some(cwd) = map.get("cwd").and_then(Value::as_str) {
            let cwd = self.paths(cwd);
            map.insert("cwd".into(), Value::String(cwd));
        }
        // Rule: macOS gives every process it starts `__CF_USER_TEXT_ENCODING`, whatever its parent
        // set; Linux doesn't. Every other name stays, exactly as obid set it.
        if let Some(Value::Array(env)) = map.get_mut("env") {
            env.retain(|name| name != "__CF_USER_TEXT_ENCODING");
        }
        self.normalize_strings(Value::Object(map.clone()))
    }

    /// The test's folder and the crate's in `text`, as `<FOLDER>` and `<CRATE>`; then random ids.
    fn paths(&self, text: &str) -> String {
        let folder = self.folder.to_string_lossy().into_owned();
        let mut out = text.replace(&folder, "<FOLDER>");
        // Rule: macOS's temporary folder is reached through /var, a link to /private/var; obid may
        // say either.
        if let Some(rest) = folder.strip_prefix("/private") {
            out = out.replace(rest, "<FOLDER>");
        }
        out = out.replace(&*crate_dir().to_string_lossy(), "<CRATE>");
        // Rule: obid names some temporary folders after its process id (`github-sign-in-<pid>-<n>`).
        if let Some(pid) = self.pid.get() {
            out = out.replace(&format!("-{pid}-"), "-<PID>-");
        }
        out
    }

    /// Commit hashes in `text` as `<commit-n>`, n counting distinct hashes in the order this
    /// recording first met them, calls before answers. Rule: a commit made while the test runs (the
    /// fixture's, an agent's, obid's merge of a base) has a hash that changes with the clock; the
    /// number keeps which calls and answers name the same commit.
    fn commits(&self, text: &str) -> String {
        static SHA: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let sha = SHA.get_or_init(|| Regex::new(r"\b[0-9a-f]{40}\b").unwrap());
        let mut seen = self.commits.borrow_mut();
        let full = sha
            .replace_all(text, |m: &regex::Captures| {
                let hash = m[0].to_owned();
                let n = match seen.iter().position(|h| *h == hash) {
                    Some(n) => n,
                    None => {
                        seen.push(hash);
                        seen.len() - 1
                    }
                };
                format!("<commit-{}>", n + 1)
            })
            .into_owned();
        // The short forms obid shows of a hash it met (12 and 7 characters), the same way.
        let mut out = full;
        for (n, hash) in seen.iter().enumerate() {
            for len in [12, 7] {
                let short = Regex::new(&format!(r"\b{}\b", &hash[..len])).unwrap();
                out = short
                    .replace_all(&out, format!("<commit-{}>", n + 1).as_str())
                    .into_owned();
            }
        }
        out
    }

    /// Every string in `value` with [`paths`](Self::paths), [`ids`] and
    /// [`commits`](Self::commits) applied.
    fn normalize_strings(&self, value: Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.commits(&ids(&self.paths(&s)))),
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|v| self.normalize_strings(v))
                    .collect(),
            ),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, self.normalize_strings(v)))
                    .collect(),
            ),
            other => other,
        }
    }

    /// An answer of obid's, comparable: [`normalize_strings`], and the values of the keys that
    /// hold obid's own clock ([`CLOCK_KEYS`]) as `<TIME>`. Never a GitHub URL, a key or a value
    /// GitHub gave.
    fn normalize_answer(&self, value: Value, key: Option<&str>) -> Value {
        match value {
            Value::Object(map) => {
                let mut out = Map::new();
                for (k, v) in map {
                    let v = self.normalize_answer(v, Some(&k));
                    out.insert(k, v);
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|v| self.normalize_answer(v, key))
                    .collect(),
            ),
            // Rule: a time obid stamps from its own clock (when it read, started, ended).
            Value::String(_) | Value::Number(_) if key.is_some_and(|k| CLOCK_KEYS.contains(&k)) => {
                Value::String("<TIME>".into())
            }
            other => self.normalize_strings(other),
        }
    }
}

/// The keys whose values are times obid takes from its own clock, never from GitHub: when it read
/// a pull request (`checkedAt`), when a sign-in's code ends (`expiresAt`), when a job or an item
/// was made, started, changed or ended, when a merge was noted (`at`), when an outgoing action went
/// (`date`), when an item was claimed to post its review (`posting`), and when the work feed reads
/// next (`nextCheck`).
const CLOCK_KEYS: &[&str] = &[
    "checkedAt",
    "created",
    "date",
    "posting",
    "nextCheck",
    "expiresAt",
    "createdAt",
    "startedAt",
    "updatedAt",
    "endedAt",
    "finishedAt",
    "readAt",
    "at",
];

/// `calls` with every write, in order, and each read only the first time it was made. Rule, for a
/// path where obid's own loops read the same thing again at times of their own (the repository
/// list rereads gh's login once a sign-in ends): how often and when a read is made again is the
/// machine's timing; which reads there are, and every write in its order, are obid's.
pub fn distinct_reads(calls: Vec<Value>) -> Vec<Value> {
    let mut seen: Vec<Value> = Vec::new();
    calls
        .into_iter()
        .filter(|call| {
            if writes(call) {
                return true;
            }
            if seen.contains(call) {
                return false;
            }
            seen.push(call.clone());
            true
        })
        .collect()
}

/// Whether `call` changes something on GitHub or in a login: a request with a method of its own,
/// a pull request's create, edit, merge, review or close, a run's rerun, an issue's edit, a sign-in.
/// Everything else reads.
pub fn writes(call: &Value) -> bool {
    let args: Vec<&str> = call["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    let pair = args.iter().take(2).copied().collect::<Vec<_>>().join(" ");
    let method = args
        .windows(2)
        .any(|w| (w[0] == "-X" || w[0] == "--method") && w[1] != "GET");
    method
        || matches!(
            pair.as_str(),
            "pr create"
                | "pr edit"
                | "pr merge"
                | "pr review"
                | "pr close"
                | "pr comment"
                | "run rerun"
                | "issue edit"
                | "issue comment"
                | "auth login"
                | "auth logout"
                | "auth git-credential"
        )
}

/// `calls` with a block of reads that repeats right after itself kept once. Rule: a test waits for
/// state by asking obid again (support::within), and obid's timers read GitHub again while nothing
/// changes; how many times depends on the machine's speed, so a poll's reads are recorded once,
/// where it began. A block holding a write is never folded: every write is recorded each time.
pub fn polls_once(mut calls: Vec<Value>) -> Vec<Value> {
    let mut i = 0;
    while i < calls.len() {
        let mut folded = false;
        for k in 1..=6 {
            if i + 2 * k > calls.len() {
                break;
            }
            let block = &calls[i..i + k];
            if block.iter().all(|c| !writes(c)) && block == &calls[i + k..i + 2 * k] {
                calls.drain(i + k..i + 2 * k);
                folded = true;
                break;
            }
        }
        if !folded {
            i += 1;
        }
    }
    calls
}

/// Random ids in `text`, as `<UUID>` (Rule: obid names jobs, temporary files and folders with
/// fresh UUIDs, different on every run, in their usual form or their simple one, 32 hex digits
/// with no dashes: `listen-post-<32 hex>`, `approve-<32 hex>`).
fn ids(text: &str) -> String {
    static UUID: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let uuid = UUID.get_or_init(|| {
        Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|\b[0-9a-f]{32}\b")
            .unwrap()
    });
    uuid.replace_all(text, "<UUID>").into_owned()
}

/// `made` against the file `kept`: written there under `GITHUB_RECORDED=bless`, else equal to it
/// or the test fails with the lines that differ.
pub fn compare(kept: &Path, made: &Value) {
    let text = serde_json::to_string_pretty(made).unwrap() + "\n";
    if std::env::var("GITHUB_RECORDED").as_deref() == Ok("bless") {
        std::fs::write(kept, &text).unwrap();
        eprintln!("recorded {}", kept.display());
        return;
    }
    let was = std::fs::read_to_string(kept).unwrap_or_else(|_| {
        panic!(
            "no recording at {}: record it with GITHUB_RECORDED=bless (README.md)\n{text}",
            kept.display()
        )
    });
    if was != text {
        panic!(
            "GitHub's recorded behavior changed: {}\n(- recorded, + now; re-record only for a change meant to change it, README.md)\n{}",
            kept.display(),
            diff(&was, &text)
        );
    }
}

/// The lines of `was` and `now` that differ, `-` and `+`, with two lines of context around each.
pub fn diff(was: &str, now: &str) -> String {
    let a: Vec<&str> = was.lines().collect();
    let b: Vec<&str> = now.lines().collect();
    // Longest common subsequence, by lines.
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut lines: Vec<(char, &str)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            lines.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            lines.push(('+', b[j]));
            j += 1;
        } else {
            lines.push(('-', a[i]));
            i += 1;
        }
    }
    let changed: Vec<usize> = (0..lines.len()).filter(|&k| lines[k].0 != ' ').collect();
    let mut out = String::new();
    let mut last: Option<usize> = None;
    for (k, (mark, line)) in lines.iter().enumerate() {
        if !changed.iter().any(|&c| c.abs_diff(k) <= 2) {
            continue;
        }
        if last.is_some_and(|l| k > l + 1) {
            out.push_str("   ...\n");
        }
        out.push_str(&format!("{mark} {line}\n"));
        last = Some(k);
    }
    out
}
