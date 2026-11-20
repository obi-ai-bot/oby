//! GitHub as obid uses it today, recorded (tests/github_recorded/README.md): each test drives one
//! GitHub path through obid's API, on a scratch obid with the recording `gh` (tests/fakes/gh) first
//! on its `PATH`, and compares (a) every `gh` call obid made for it and (b) every answer of obid's
//! that carries GitHub's data with the recordings kept in tests/github_recorded/<path>/. These are
//! the baseline a move of GitHub behind another interface must reproduce exactly.

mod support;

mod recorder {
    //! The recorded baseline's comparison (tests/github_recorded/README.md): what a test's obid asked the
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
                Value::String(_) | Value::Number(_)
                    if key.is_some_and(|k| CLOCK_KEYS.contains(&k)) =>
                {
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
            Regex::new(
                r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|\b[0-9a-f]{32}\b",
            )
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
}

mod sign_in {
    //! Sign-in status and the reviewer account: `GET /v1/github`, the owner's device sign-in
    //! (`POST /v1/github/sign-in`), and the reviewer's (`POST`/`DELETE /v1/github/reviewer/sign-in`).

    use serde_json::json;

    use crate::support::*;

    #[test]
    fn github_recorded_sign_in() {
        let (_folder, recording, obid, _) = crate::recorder::recorded_obid("sign-in", "sign_in");
        let mut answers = Vec::new();
        let mut said = |what: &str, answer: Answer| {
            answers.push(json!({"asked": what, "status": answer.status, "body": answer.body}));
        };

        said("GET /v1/github", call(&obid, "GET", "/v1/github", None));
        said(
            "POST /v1/github/sign-in",
            call(&obid, "POST", "/v1/github/sign-in", None),
        );
        recording.raise("approve");
        // The sign-in's gh ends before obid is asked again, so none of the reads below run at its time.
        within(20, "gh's sign-in to end", || {
            recording.ended("auth login").then_some(())
        });
        let signed = within(20, "gh to say it's signed in", || {
            let now = call(&obid, "GET", "/v1/github", None);
            (now.body["signedIn"] == true && now.body["signIn"].is_null()).then_some(now)
        });
        said("GET /v1/github, signed in", signed);

        said(
            "POST /v1/github/reviewer/sign-in",
            call(&obid, "POST", "/v1/github/reviewer/sign-in", None),
        );
        recording.raise("approve-reviewer");
        within(20, "the reviewer's sign-in to end", || {
            recording
                .ended("auth login --hostname github.com --git-protocol https --web --skip-ssh-key --insecure-storage")
                .then_some(())
        });
        let reviewer = within(20, "the reviewer to be signed in", || {
            let now = call(&obid, "GET", "/v1/github", None);
            (now.body["reviewer"]["signedIn"] == true && now.body["reviewer"]["signIn"].is_null())
                .then_some(now)
        });
        said("GET /v1/github, reviewer signed in", reviewer);
        said(
            "DELETE /v1/github/reviewer/sign-in",
            call(&obid, "DELETE", "/v1/github/reviewer/sign-in", None),
        );
        said(
            "GET /v1/github, reviewer signed out",
            call(&obid, "GET", "/v1/github", None),
        );

        recording.assert_calls(&crate::recorder::distinct_reads(
            recording.calls_starting(&["auth"]),
        ));
        recording.assert_answers(&json!(answers));
    }
}

mod work_feed {
    //! The GitHub work feed (docs/runner-api.md, "obid only: GitHub work"): review requests, assigned
    //! issues and the owner's own pull requests, read with one `gh api graphql`, listed by
    //! `GET /v1/github/work`, read again on `POST /v1/github/work/refresh`, and new work offered once as
    //! `github` inbox items.

    use serde_json::json;

    use crate::support::*;

    #[test]
    fn github_recorded_work_feed() {
        let (_folder, recording, obid, _) =
            crate::recorder::recorded_obid("work-feed", "work_feed");
        let mut answers = Vec::new();

        // obid's own read at its start ends first: the feed then has a next check.
        within(30, "obid's first read of GitHub work", || {
            let feed = call(&obid, "GET", "/v1/github/work", None);
            (!feed.body["nextCheck"].is_null()).then_some(())
        });
        let feed = call(&obid, "GET", "/v1/github/work", None);
        answers.push(
            json!({"asked": "GET /v1/github/work", "status": feed.status, "body": feed.body}),
        );
        // The first read on record notes what's there and offers nothing.
        answers.push(json!({"asked": "POST /v1/github/work/refresh", "body": refresh(&obid)}));
        answers.push(json!({"asked": "github items, open", "body": github_items(&obid, "open")}));

        // New work: offered once, however often it's read.
        recording.raise("second");
        answers.push(
            json!({"asked": "POST /v1/github/work/refresh, new work", "body": refresh(&obid)}),
        );
        answers
            .push(json!({"asked": "POST /v1/github/work/refresh, again", "body": refresh(&obid)}));
        // The quick model sorts an assigned issue (crate::triage): its answer, not GitHub's, so the
        // items are read once it has, or once each one still waiting says why (`triageWhy`: no
        // account here can sort it, TRIAGE-ANY-2).
        let mut items = within(150, "the new items sorted, or saying why they wait", || {
            let items = github_items(&obid, "open");
            items
                .iter()
                .all(|i| i["triage"] != "pending" || i["triageWhy"].is_string())
                .then_some(items)
        });
        items.sort_by_key(|i| i["title"].as_str().unwrap_or_default().to_owned());
        answers.push(json!({"asked": "github items, open, new work", "body": items}));

        // The feed's own calls: obid's other loops read GitHub at its start too, at the same time (the
        // noreply email's `api user --jq …`, the sign-in's `auth status`), each its own path.
        recording.assert_calls(&recording.calls_starting(&[
            "api graphql",
            "api user --jq .login",
            "api user/orgs",
        ]));
        recording.assert_answers(&json!(answers));
    }
}

mod issue_task {
    //! Starting a task from an issue (docs/contracts/issue-tasks.md, ISSUETASK): `POST /v1/jobs` with
    //! `issue`, its text read with obid's own gh and quoted as data in the job's worktree, the job run
    //! to its end by the stand-in agent, and the job's answer keeping the issue and its page.

    use serde_json::json;

    use crate::support::*;

    #[test]
    fn github_recorded_issue_task() {
        let (folder, recording, mut config) =
            crate::recorder::recorded_config("issue-task", "issue_task");
        let remote = fixture(&folder.0.join("fixture"));
        config["repos"][0]["remote"] = format!("file://{}", remote.display()).into();
        config["agents"]["claude"]["bin"] = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/agents/fake-claude")
            .to_string_lossy()
            .into_owned()
            .into();
        let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
        recording.started(&obid);

        let made = call(
            &obid,
            "POST",
            "/v1/jobs",
            Some(
                &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "fix",
                                 "task": "KEEP PROMPTS", "issue": 140}}),
            ),
        );
        assert_eq!(made.status, 201, "{}", made.text);
        let id = made.body["id"].as_str().unwrap().to_owned();
        let ended = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
        assert_eq!(ended.status, 200, "{}", ended.text);
        let read = call(&obid, "GET", &format!("/v1/jobs/{id}"), None);

        let github = |job: &serde_json::Value| {
            json!({"issue": job["issue"], "issueUrl": job["issueUrl"],
                   "pullRequest": job["pullRequest"], "published": job["published"]})
        };
        let answers = json!([
            {"asked": "POST /v1/jobs", "status": made.status, "body": github(&made.body)},
            {"asked": "GET /v1/jobs/{id}/wait", "status": ended.status, "body": github(&ended.body)},
            {"asked": "GET /v1/jobs/{id}", "status": read.status, "body": github(&read.body)},
        ]);
        // The quoted file the agent reads, as obid wrote it from what gh answered.
        let quoted = std::fs::read_to_string(obid.home.join(format!(
            "worktrees/{id}/.obi/inputs/github-fixture-app-140.md"
        )))
        .unwrap();

        // The issue's own calls: obid's other loops read GitHub at its start too (the work feed, the
        // noreply email, the sign-in's status), each its own path.
        recording.assert_calls(&recording.calls_starting(&[
            "issue view",
            "api repos/obi-e2e/fixture-app/issues/",
            "api graphql -f query=query($owner",
        ]));
        recording.assert_answers(&json!({"answers": answers, "quoted": quoted_now(&quoted)}));
    }

    /// The quoted file with what obid makes up as it writes it replaced. Rules: the time it read the
    /// issue (`as GitHub had it at <time>`) is its clock's, and the fence around each quote ends in a
    /// random nonce (`quoted-<16 hex>`), so no quoted text can close it.
    fn quoted_now(quoted: &str) -> String {
        let at = regex::Regex::new(r"as GitHub had it at \d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ").unwrap();
        let fence = regex::Regex::new(r"quoted-[0-9a-f]{16}").unwrap();
        let quoted = at.replace_all(quoted, "as GitHub had it at <TIME>");
        fence.replace_all(&quoted, "quoted-<NONCE>").into_owned()
    }
}

mod publish {
    //! A job publishing: the push, the pull request it opens (`gh pr list`, `gh pr create`), the
    //! follow read of it (`gh pr view`), and a follow-up's push onto the same branch, which brings that
    //! pull request's description up to date (`gh pr edit`).
    //!
    //! Not recorded: `gh` as git's credential helper. obid adds it (`credential.helper=!env … gh auth
    //! git-credential`, crate::publish::credentials) only for an `https://` remote, and these tests'
    //! remote is the fixture's `file://` one, so git never asks it; obid's API doesn't show the setting.

    use std::path::Path;

    use serde_json::{Value, json};

    use crate::support::*;

    /// The stand-in agent that commits what its task names (`COMMIT: <file>`).
    fn agent() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/publishing-claude")
    }

    /// What a job's answer says of GitHub: its pull request as obid read it, where it published, what
    /// went out, and why not.
    fn github_of(job: &Value) -> Value {
        let mut picked = serde_json::Map::new();
        for key in [
            "branch",
            "published",
            "publishError",
            "pullRequest",
            "pullRequestError",
            "outgoing",
            "issue",
            "issueUrl",
        ] {
            if let Some(value) = job.get(key) {
                picked.insert(key.into(), value.clone());
            }
        }
        Value::Object(picked)
    }

    /// The job once it has ended.
    fn ended(obid: &Obid, id: &str) -> Value {
        ended_within(obid, id, 120)
    }

    /// Rule: a job's id is 8 random hex digits, and its branch is named after it (`oby/build-<id>`):
    /// each job's id as `<job-n>`, in the order the test made them.
    fn jobs_named(text: &str, ids: &[&str]) -> String {
        let mut out = text.to_owned();
        for (n, id) in ids.iter().enumerate() {
            out = out.replace(id, &format!("<job-{}>", n + 1));
        }
        out
    }

    fn named(value: Value, ids: &[&str]) -> Value {
        serde_json::from_str(&jobs_named(&value.to_string(), ids)).unwrap()
    }

    #[test]
    fn github_recorded_publish() {
        let (folder, recording, mut config) =
            crate::recorder::recorded_config("publish", "publish");
        let remote = fixture(&folder.0.join("fixture"));
        config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
        config["agents"]["claude"]["bin"] = json!(agent());
        signed_in(&config);
        let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
        recording.started(&obid);

        let made = call_typed(
            &obid,
            "POST",
            "/v1/jobs",
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
                            "check": "test -f hello.txt", "title": "Hello file"}}),
        );
        assert_eq!(made.status, 201, "{}", made.text);
        let first = made.body["id"].as_str().unwrap().to_owned();
        let job = ended(&obid, &first);
        assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
        // obid reads the pull request right after it opened it.
        let opened = within(30, "obid reads the pull request", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{first}"), None).body;
            job["pullRequest"].is_object().then_some(job)
        });

        let told = call(
            &obid,
            "POST",
            &format!("/v1/jobs/{first}/messages"),
            Some(&json!({"text": "COMMIT: second.txt"})),
        );
        assert_eq!(told.status, 201, "{}", told.text);
        let next = told.body["id"].as_str().unwrap().to_owned();
        let follow = ended(&obid, &next);
        assert_eq!(follow["status"], "done", "{follow}\n{}", obid.log());
        let edited = within(30, "the follow-up's pull request read", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{next}"), None).body;
            job["pullRequest"].is_object().then_some(job)
        });

        let ids = [first.as_str(), next.as_str()];
        // Publishing's own calls: not obid's read of the login's email at its start (`noreply`) nor
        // its read of the GitHub work feed (`api graphql`, the feed's own recording).
        let calls: Vec<Value> = recording
            .calls_starting(&["pr ", "repo ", "issue ", "api"])
            .into_iter()
            .filter(|c| c["args"][1] != "user" && c["args"][1] != "graphql")
            .map(|c| named(c, &ids))
            .collect();
        recording.assert_calls(&calls);
        recording.assert_answers(&named(
            json!([
                {"asked": "the job, published", "job": github_of(&opened)},
                {"asked": "its follow-up, published onto its pull request", "job": github_of(&edited)},
            ]),
            &ids,
        ));
    }
}

mod noreply {
    //! The noreply email (crate::jobs, `github_email`): with no git email, obid asks gh for the GitHub
    //! login's noreply address (`gh api user --jq …`) at its start, and a job's commits are by it.

    use std::path::Path;

    use serde_json::{Value, json};

    use crate::support::*;

    #[test]
    fn github_recorded_noreply() {
        let (folder, recording, mut config) =
            crate::recorder::recorded_config("noreply", "noreply");
        let remote = fixture(&folder.0.join("fixture"));
        config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
        config["agents"]["claude"]["bin"] =
            json!(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/publishing-claude"));
        signed_in(&config);
        let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
        recording.started(&obid);
        // obid's read at its start answered (the fake raises `user-read` once it has).
        within(30, "obid to ask gh for the login's email", || {
            recording.dir.join("flags/user-read").exists().then_some(())
        });

        let made = call_typed(
            &obid,
            "POST",
            "/v1/jobs",
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
                            "check": "test -f hello.txt"}}),
        );
        assert_eq!(made.status, 201, "{}", made.text);
        let id = made.body["id"].as_str().unwrap();
        let job = ended_within(&obid, id, 120);
        assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
        let branch = job["branch"].as_str().unwrap();
        let author = std::process::Command::new("git")
            .args(["log", "-1", "--format=%an <%ae>|%cn <%ce>"])
            .arg(format!("refs/heads/{branch}"))
            .current_dir(&remote)
            .output()
            .unwrap();
        assert!(author.status.success(), "{author:?}");
        let author = String::from_utf8_lossy(&author.stdout).trim().to_owned();

        recording.assert_calls(&recording.calls_starting(&["api user"]));
        recording.assert_answers(&Value::Array(vec![json!({
            "asked": "the author and committer of the commit the job pushed",
            "commit": author,
        })]));
    }
}

mod merge {
    //! Merging (crate::merge): a remembered workflow merging an approved pull request (the reviewer's
    //! approval, the merge, the branch's deletion), GitHub's auto-merge armed while its checks run and
    //! the merge GitHub then made, and a base that takes it only through its merge queue. Each drives
    //! the follower of a published job kept for obid's start, as tests/merge_api.rs does.

    use std::path::Path;

    use serde_json::{Value, json};

    use crate::recorder::{Recording, recorded_config};
    use crate::support::*;

    pub const REPO: &str = "obi-e2e/fixture-app";
    pub const PULL: &str = "https://github.com/obi-e2e/fixture-app/pull/7";
    pub const ID: &str = "a1b2c3d4";

    /// The workflow where Oby's reviewer account approves and GitHub's checks pass.
    const REVIEWER: &str = r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"reviewer","checks":"github"}"#;
    /// The workflow where Oby's own review of the head is the review.
    pub const OBY: &str =
        r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"oby","checks":"github"}"#;

    fn head() -> String {
        "a".repeat(40)
    }

    /// A job done and published at `head()` into dev, scanned clean, which Oby's own review approved
    /// (tests/merge_api.rs, `published`).
    fn published() -> obid::job::Job {
        let mut job = obid::job::Job::new(
            ID.into(),
            REPO.into(),
            "fix".into(),
            "Fix the thing".into(),
            "claude".into(),
            "stub".into(),
        );
        job.status = obid::job::Status::Done;
        job.branch = "oby/fix-7".into();
        job.base = Some("dev".into());
        job.commits = Some(1);
        job.pushed = Some(1);
        job.published = Some(PULL.into());
        job.finished = Some(job.created);
        job.outgoing.push(obid::publish::Outgoing::new(
            "push",
            REPO,
            &head(),
            Some("oby/fix-7"),
        ));
        job.scan = Some(obid::publish::Scanned {
            ran: true,
            clean: Some(true),
            ..Default::default()
        });
        job.review = Some(obid::job::Review {
            verdict: "approved".into(),
            job: "r1b2c3d4".into(),
            agent: "codex".into(),
            account: "default".into(),
            same_account: false,
            round: 1,
            date: job.created,
            commit: head(),
            two_reviews: None,
        });
        job
    }

    /// The owner taught Oby how fixture-app merges: `workflow`.
    pub fn remember(home: &Path, workflow: &str) {
        let dir = home.join("memory/repos/obi-e2e/fixture-app");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("m1a2b3c4d.md"),
            format!(
                "---\nid: m1a2b3c4d\nscope: project\nrepo: {REPO}\nkind: workflow\nsource: user\ntitle: How fixture-app merges\nfrom: You merged fixture-app#6 this way\ncreated: 2026-10-24T10:00:00Z\nupdated: 2026-10-24T10:00:00Z\nused: 0\nworkflow: {workflow}\n---\nSquash-merge into dev after Oby's reviewer account approves and GitHub's checks pass on the same commit.\n"
            ),
        )
        .unwrap();
    }

    /// A scratch obid following `job`, kept for its start, with `workflow` remembered and, with
    /// `reviewer`, Oby's reviewer account (`o-reviewer`) signed in; it reads the pull request every
    /// second.
    fn following(
        name: &str,
        path: &str,
        job: obid::job::Job,
        workflow: &str,
        reviewer: bool,
    ) -> (Folder, Recording, Obid) {
        let (folder, recording, mut config) = recorded_config(name, path);
        let remote = fixture(&folder.0.join("fixture"));
        config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
        let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
        std::fs::create_dir_all(home.join("jobs")).unwrap();
        std::fs::write(
            home.join(format!("jobs/{}.json", job.id)),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        if reviewer {
            std::fs::create_dir_all(home.join("github/reviewer")).unwrap();
            std::fs::write(
                home.join("github/reviewer/hosts.yml"),
                "github.com:\n    user: o-reviewer\n",
            )
            .unwrap();
        }
        remember(&home, workflow);
        let obid = start_with(
            &folder.0,
            &config,
            Some(&crate::recorder::bin()),
            &[
                ("OBID_FOLLOW_SECONDS", "1"),
                ("OBID_RERUN_SETTLE_SECONDS", "0"),
            ],
        );
        recording.started(&obid);
        (folder, recording, obid)
    }

    /// What the job `id` shows of GitHub and of its merge.
    pub fn github_of(obid: &Obid, id: &str) -> Value {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        let mut shown = json!({});
        for key in [
            "published",
            "pullRequest",
            "issue",
            "issueUrl",
            "mergedBy",
            "mergeError",
            "mergeNeedsYou",
            "autoMerge",
            "autoMergeOff",
            "outgoing",
        ] {
            shown[key] = job.get(key).cloned().unwrap_or(Value::Null);
        }
        // Rule: a pull request obid merged itself is `mergedAt` obid's clock then; one GitHub merged
        // keeps GitHub's time (the fixtures' 2026-10-24T18:00:00Z).
        if let Some(at) = shown["pullRequest"]["mergedAt"].as_str()
            && !at.starts_with("2026-10-24T")
        {
            shown["pullRequest"]["mergedAt"] = json!("<TIME>");
        }
        let mut stage = job["pipeline"]["stages"]
            .as_array()
            .and_then(|stages| stages.iter().find(|s| s["id"] == "merged").cloned())
            .unwrap_or(Value::Null);
        // Rule: a stage's times are obid's clock, and how long it took the machine's speed.
        for key in ["started", "finished", "seconds", "waited"] {
            if stage.get(key).is_some() {
                stage[key] = json!("<TIME>");
            }
        }
        shown["mergedStage"] = stage;
        shown
    }

    /// The follower's calls for a path: [`MERGE_CALLS`], from obid's start to the path's last write.
    pub fn follower_calls(recording: &Recording) -> Vec<Value> {
        let mut calls = followed(recording);
        // Rule: past the path's last write the follower only reads the pull request again each
        // second, until the test stops obid: how many of those reads a run catches is the machine's.
        // A merge queue's base is read once more after it, for the runs GitHub's queue made of it
        // (crate::jobs::autofix, AUTOFIX-11, 12): the path ends with that read.
        if let Some(last) = calls.iter().rposition(crate::recorder::writes) {
            let queue = calls[last..]
                .iter()
                .position(reads_the_queue)
                .map(|n| last + n);
            calls.truncate(queue.unwrap_or(last) + 1);
        }
        calls
    }

    /// The follower's calls for a path: [`MERGE_CALLS`] from obid's start, the repository list's own
    /// read of the login left out.
    pub fn followed(recording: &Recording) -> Vec<Value> {
        // Rule: obid's repository list reads gh's login once at start (crate::repos::spawn), at the
        // same time as the follower's first read, in either order: that first `auth status` as the
        // owner is the list's, not the path's, and is left out before anything is compared.
        let raw = recording.dir.join("calls.jsonl");
        let text = std::fs::read_to_string(&raw).unwrap_or_default();
        let mut lines: Vec<&str> = text.lines().collect();
        if let Some(first) = lines.iter().position(|line| {
            let call: Value = serde_json::from_str(line).unwrap();
            call["args"][0] == "auth"
                && call["args"][1] == "status"
                && !call["env"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|n| n == "GH_CONFIG_DIR")
        }) {
            lines.remove(first);
        }
        std::fs::write(
            &raw,
            lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
        )
        .unwrap();
        recording.calls_starting(MERGE_CALLS)
    }

    /// Whether `call` reads the runs of GitHub's merge queue.
    fn reads_the_queue(call: &Value) -> bool {
        let args: Vec<&str> = (call["args"].as_array().unwrap().iter())
            .filter_map(Value::as_str)
            .collect();
        args.starts_with(&["run", "list"]) && args.contains(&"merge_group")
    }

    /// The merge cards, as the inbox shows them (open and done).
    pub fn merge_cards(obid: &Obid) -> Value {
        let cards: Vec<Value> = call(obid, "GET", "/v1/inbox", None)
            .body
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i["kind"] == "merge")
            .map(|c| json!({"title": c["title"], "text": c["text"], "options": c["options"], "done": !c["done"].is_null()}))
            .collect();
        json!(cards)
    }

    /// The calls of the follower's merge path: the pull request's reads and writes and the logins it
    /// asks about, apart from the work feed's and the commit email's reads obid makes meanwhile.
    pub const MERGE_CALLS: &[&str] = &[
        "auth status",
        "pr ",
        "run ",
        "api -X",
        "api --paginate --slurp repos/",
        "api repos/",
        "api user/orgs",
        "api user --jq .login",
    ];

    #[test]
    fn github_recorded_merge() {
        let (_folder, recording, obid) = following("merge", "merge", published(), REVIEWER, true);
        let merged = within(30, "the job to be merged", || {
            let shown = crate::merge::github_of(&obid, ID);
            (shown["mergedBy"]["by"] == "memory").then_some(shown)
        });
        let answers = json!({"job": merged, "mergeCards": merge_cards(&obid)});
        drop(obid);
        recording.assert_calls(&follower_calls(&recording));
        recording.assert_answers(&answers);
    }

    #[test]
    fn github_recorded_auto_merge() {
        let (_folder, recording, obid) =
            following("auto-merge", "auto_merge", published(), OBY, false);
        let armed = within(30, "the job handed to GitHub", || {
            let shown = github_of(&obid, ID);
            (shown["autoMerge"]["head"] == head()).then_some(shown)
        });
        recording.raise("merged-on-github");
        let merged = within(30, "the job merged by GitHub", || {
            let shown = github_of(&obid, ID);
            (shown["mergedBy"]["by"] == "memory"
                && shown["outgoing"]
                    .as_array()
                    .is_some_and(|o| o.iter().any(|s| s["action"] == "branchDelete")))
            .then_some(shown)
        });
        let answers = json!({"armed": armed, "merged": merged, "mergeCards": merge_cards(&obid)});
        drop(obid);
        recording.assert_calls(&follower_calls(&recording));
        recording.assert_answers(&answers);
    }

    #[test]
    fn github_recorded_merge_queue() {
        let mut own = published();
        own.own_conversation = true;
        let (_folder, recording, obid) = following("merge-queue", "merge_queue", own, OBY, false);
        let queued = within(30, "the job handed to the merge queue", || {
            let shown = github_of(&obid, ID);
            (shown["autoMerge"]["head"] == head()).then_some(shown)
        });
        // Armed, the follower reads the queue's runs on its next pass.
        within(30, "the merge queue's runs read", || {
            let raw =
                std::fs::read_to_string(recording.dir.join("calls.jsonl")).unwrap_or_default();
            raw.contains("merge_group").then_some(())
        });
        let answers = json!({"queued": queued, "mergeCards": merge_cards(&obid)});
        drop(obid);
        recording.assert_calls(&follower_calls(&recording));
        recording.assert_answers(&answers);
    }
}

mod autofix {
    //! After the pull request opened: the follower watching it, and a Fix CI round (AUTOFIX-1, 2, 3,
    //! 5). The owner's own task, really run and pushed, reads red on GitHub at the head it pushed: obid
    //! reads the failed run's log, the same job runs one more round, pushes over it, and reads it green.
    //! As tests/merge_api.rs's `autofix_1_a_red_head_resumes_its_own_task_with_no_card`.

    use std::path::Path;

    use serde_json::{Value, json};

    use crate::merge::{PULL, REPO, github_of, merge_cards, remember};
    use crate::recorder::recorded_config;
    use crate::support::*;

    /// The workflow where a person's approval is the review.
    const PERSON: &str = r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"person","checks":"github"}"#;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// The job's AUTOFIX lines, as obid keeps them in its file (the API never shows them).
    fn autofix_lines(home: &Path, id: &str) -> Vec<String> {
        let kept: Value =
            serde_json::from_slice(&std::fs::read(home.join(format!("jobs/{id}.json"))).unwrap())
                .unwrap();
        (kept["autofix"]["lines"].as_array().into_iter().flatten())
            .filter_map(|l| l.as_str().map(str::to_owned))
            .collect()
    }

    #[test]
    fn github_recorded_autofix() {
        let (folder, recording, mut config) = recorded_config("autofix", "autofix");
        let remote = fixture(&folder.0.join("fixture"));
        git(&remote, &["branch", "dev", "main"]);
        config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
        config["repos"][0]["pullRequests"] = json!(false);
        config["repos"][0].as_object_mut().unwrap().remove("check");
        config["agents"]["claude"]["bin"] =
            json!(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude"));
        let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
        remember(&home, PERSON);
        std::fs::write(recording.dir.join("remote"), remote.to_str().unwrap()).unwrap();
        let env = [
            ("OBID_FOLLOW_SECONDS", "1"),
            ("OBID_RERUN_SETTLE_SECONDS", "0"),
        ];
        let bin = crate::recorder::bin();

        // The task runs and pushes `oby/fix-7` (a `file://` remote opens no pull request).
        let obid = start_with(&folder.0, &config, Some(&bin), &env);
        let task = json!({"job": {"repo": REPO, "kind": "build", "task": "COMMIT: fix.txt",
                                  "branch": "oby/fix-7", "base": "dev", "check": null, "rounds": 1}});
        let posted = call(&obid, "POST", "/v1/jobs", Some(&task));
        assert_eq!(posted.status, 201, "{}", posted.text);
        let id = posted.body["id"].as_str().unwrap().to_owned();
        let done = ended_within(&obid, &id, 60);
        assert_eq!(done["status"], "done", "{done}");
        let head = git(&remote, &["rev-parse", "refs/heads/oby/fix-7"]);
        drop(obid);

        // Its file says it opened pull request #7, from the owner's own conversation; GitHub's check
        // `test` failed at the head it pushed (`red-at`), and the fake agent fixes checks when told.
        let file = home.join(format!("jobs/{id}.json"));
        let mut kept: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        kept["published"] = json!(PULL);
        kept["ownConversation"] = json!(true);
        kept.as_object_mut().unwrap().remove("unsigned");
        std::fs::write(&file, serde_json::to_vec(&kept).unwrap()).unwrap();
        let obi = home.join("worktrees").join(&id).join(".obi");
        std::fs::create_dir_all(&obi).unwrap();
        for leave in ["keep-prompts", "fix-checks"] {
            std::fs::write(obi.join(leave), "").unwrap();
        }
        std::fs::write(recording.dir.join("red-at"), &head).unwrap();
        // Rule: the path's recording begins with the obid that follows the published job; the calls
        // of the run that made it are the publishing path's, recorded there.
        std::fs::remove_file(recording.dir.join("calls.jsonl")).ok();

        let obid = start_with(&folder.0, &config, Some(&bin), &env);
        recording.started(&obid);
        let red = within(60, "the red head read", || {
            let shown = github_of(&obid, &id);
            (shown["pullRequest"]["head"] == head.as_str()).then_some(shown)
        });
        let lines = within(90, "its checks green after its round", || {
            let lines = autofix_lines(&home, &id);
            lines
                .iter()
                .any(|l| l == "Its checks pass on GitHub now.")
                .then_some(lines)
        });
        // Green, the person's workflow waits on a person's approval, which no teammate's Oby can give
        // out of an Obysphere: the card that says so is where the path settles. (A read made while the
        // round's push lands may show another card for a moment first.)
        let green = within(60, "its merge state settled on the green head", || {
            let shown = github_of(&obid, &id);
            let settled = shown["pullRequest"]["head"] != head.as_str()
                && shown["mergeError"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("Oby isn't in an Obysphere"))
                && merge_cards(&obid).as_array().unwrap().iter().any(|c| {
                    c["done"] == false && c["text"].as_str() == shown["mergeError"].as_str()
                });
            settled.then_some(shown)
        });
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        let answers = json!({
            "red": red,
            "green": green,
            "round": job["round"],
            "status": job["status"],
            "autofixInApi": job.get("autofix").cloned().unwrap_or(Value::Null),
            "autofixLines": lines,
            "mergeCards": merge_cards(&obid),
        });
        drop(obid);
        recording.assert_calls(&in_order_then_as_a_set(crate::merge::followed(&recording)));
        recording.assert_answers(&answers);
    }

    /// `calls` in their order up to obid's read of the failed run's log, then, from the round on, as a
    /// set. Rule: once the round runs, its publishing and the follower read GitHub at the same time,
    /// in no fixed order and as often as the machine's speed allows; what they ask is kept, each
    /// distinct call once, sorted.
    fn in_order_then_as_a_set(calls: Vec<Value>) -> Vec<Value> {
        let log = calls
            .iter()
            .position(|c| c["args"][0] == "run" && c["args"][1] == "view")
            .expect("the failed run's log read");
        let mut after: Vec<String> = calls[log + 1..]
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        after.sort();
        after.dedup();
        let mut kept = calls[..=log].to_vec();
        kept.push(json!({
            "from the round on, each distinct call once, in no fixed order": after
                .iter()
                .map(|c| serde_json::from_str::<Value>(c).unwrap())
                .collect::<Vec<_>>()
        }));
        kept
    }
}

mod sphere_approval {
    //! A teammate's pull request approved on GitHub through the Obysphere (crate::sphere::approval):
    //! a teammate's Oby asks for a review with `approve_on_host`, the review passes, obid asks its
    //! owner, and the owner's own signed Confirm posts the approval with the owner's `gh`.

    use serde_json::{Value, json};

    use crate::support::*;

    /// The open inbox item `id`, once it is there.
    fn item(obid: &Obid, id: &str) -> Value {
        within(60, id, || {
            call(obid, "GET", "/v1/inbox", None)
                .body
                .as_array()?
                .iter()
                .find(|i| i["id"] == id || (id == "obiRequest" && i["kind"] == "obiRequest"))
                .cloned()
        })
    }

    /// How many calls obid had made once its own reads at start were in (the GitHub work feed, the
    /// noreply email, the sign-in status, the repositories the login can push to): this path's calls
    /// are those after it. Rule: those reads run on obid's own tasks, in no fixed order with each
    /// other, and are other paths' recordings (github_work, noreply).
    fn after_startup(recording: &crate::recorder::Recording) -> usize {
        within(30, "obid's own reads of GitHub at its start", || {
            let calls = recording.calls();
            let made = |prefix: &str| {
                calls.iter().any(|c| {
                    let args: Vec<&str> = c["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| a.as_str().unwrap())
                        .collect();
                    args.join(" ").starts_with(prefix)
                })
            };
            [
                "api graphql",
                "api user --jq",
                "auth status",
                "api user/repos",
            ]
            .iter()
            .all(|p| made(p))
            .then_some(calls.len())
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn github_recorded_sphere_approval() {
        let (folder, recording, mut config) =
            crate::recorder::recorded_config("sphere-approval", "sphere_approval");
        let remote = fixture(&folder.0.join("fixture"));
        config["repos"][0]["remote"] = format!("file://{}", remote.display()).into();
        let sphere = FakeSphere::start().await;
        let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
        recording.started(&obid);
        let owner = Owner::new("o-owner");
        let joined = tokio::task::block_in_place(|| join_a(&obid, &sphere, &owner));
        assert_eq!(joined["state"], "joined", "{joined}");
        let me = joined["obiId"].as_str().unwrap().to_owned();
        let teammate = TestObi::join(&sphere, "obi-o-teammate", "o-teammate").await;
        let before = tokio::task::block_in_place(|| after_startup(&recording));

        // The teammate's Oby asks for a review of pull request #12 that approves it on GitHub.
        let mut asking = brief();
        asking["pr"] = json!(12);
        asking["approve_on_host"] = json!(true);
        let request = teammate.request(&me, "review_request", asking, 3600);
        let cid = request["id"].as_str().unwrap().to_owned();
        teammate.send(&request).await;
        let accepted = tokio::task::block_in_place(|| {
            let asked = item(&obid, "obiRequest");
            let id = asked["id"].as_str().unwrap();
            call_typed(
                &obid,
                "POST",
                &format!("/v1/inbox/{id}/answer"),
                &json!({"answer": "Accept"}),
            )
        });
        assert_eq!(accepted.status, 200, "{}", accepted.text);
        let replies = teammate.receive(2, 90).await;
        let result = replies
            .iter()
            .find(|r| !r["body"]["host_review"].is_null())
            .unwrap_or_else(|| panic!("no review result: {replies:?}"))
            .clone();
        assert_eq!(result["body"]["host_review"], "asked", "{replies:?}");

        // The ask, and the owner's own tap on it.
        let ask = tokio::task::block_in_place(|| item(&obid, &format!("obi-approve-{cid}")));
        let ask_id = ask["id"].as_str().unwrap().to_owned();
        let confirmed = tokio::task::block_in_place(|| {
            let tap = confirm_approval(&obid, &ask_id);
            call(
                &obid,
                "POST",
                &format!("/v1/inbox/{ask_id}/answer"),
                Some(&json!({"answer": "Confirm", "approval": tap})),
            )
        });
        assert_eq!(confirmed.status, 200, "{}", confirmed.text);
        let job = tokio::task::block_in_place(|| {
            let id = call(
                &obid,
                "GET",
                &format!("/v1/obid/sphere/exchanges/{cid}"),
                None,
            )
            .body["job"]
                .as_str()
                .unwrap()
                .to_owned();
            call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body
        });

        let job_id = job["id"].as_str().unwrap().to_owned();
        // This path's own rules, on top of `recorder`'s:
        // Rule: the review job's id is a fresh random one (8 hex), different on every run.
        let local = |value: Value| -> Value {
            let text = value
                .to_string()
                .replace(&format!("\"{job_id}\""), "\"<job>\"");
            serde_json::from_str(&text).unwrap()
        };
        let calls: Vec<Value> = recording.calls()[before..]
            .iter()
            .cloned()
            .map(&local)
            .collect();
        recording.assert_calls(&crate::recorder::polls_once(calls));
        let card = |item: &Value| {
            json!({"id": item["id"], "kind": item["kind"], "title": item["title"], "text": item["text"],
                   "options": item["options"], "confirm": item["confirm"], "state": item["state"],
                   "answer": item["answer"]})
        };
        let outgoing = job["outgoing"].clone();
        let answers = json!([
            {"asked": "the ask, open", "item": card(&ask)},
            {"asked": "POST /v1/inbox/{ask}/answer Confirm", "status": confirmed.status,
             "item": card(&confirmed.body)},
            {"asked": "GET /v1/jobs/{review job}", "outgoing": outgoing,
             "pullRequest": job["pullRequest"], "published": job["published"],
             "issue": job["issue"], "issueUrl": job["issueUrl"]},
            {"asked": "the teammate's Oby's reply", "host_review": result["body"]["host_review"]},
        ]);
        recording.assert_answers(&local(answers));
    }
}

mod review_submit {
    //! Review submit: the owner's Approve on a finished review's item (crate::review::post_review_with_gh,
    //! MERGING-42 and 43) posts the review it shows on the pull request, with the owner's `gh`: who
    //! opened it, its head still the commit Oby reviewed, then the review pinned to that commit.

    use serde_json::json;

    use crate::support::*;

    /// How many calls obid had made once its own reads at start were in (the GitHub work feed, the
    /// noreply email, the sign-in status, the repositories the login can push to): this path's calls
    /// are those after it. Rule: those reads run on obid's own tasks, in no fixed order with each
    /// other, and are other paths' recordings (github_work, noreply).
    fn after_startup(recording: &crate::recorder::Recording) -> usize {
        within(30, "obid's own reads of GitHub at its start", || {
            let calls = recording.calls();
            let made = |prefix: &str| {
                calls.iter().any(|c| {
                    let args: Vec<&str> = c["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| a.as_str().unwrap())
                        .collect();
                    args.join(" ").starts_with(prefix)
                })
            };
            [
                "api graphql",
                "api user --jq",
                "auth status",
                "api user/repos",
            ]
            .iter()
            .all(|p| made(p))
            .then_some(calls.len())
        })
    }

    #[test]
    fn github_recorded_review_submit() {
        let (folder, recording, config) =
            crate::recorder::recorded_config("review-submit", "review_submit");
        // The item a finished review of someone else's pull request raises (crate::jobs::
        // owner_review_item), kept as obid keeps it, so obid starts with it open.
        let inbox = folder.0.join("home/inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::copy(
            crate::recorder::kept("review_submit").join("item.json"),
            inbox.join("feed-review-r1b2c3d4.json"),
        )
        .unwrap();
        let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
        recording.started(&obid);

        let before = after_startup(&recording);
        let open = within(20, "the review's item", || {
            call(&obid, "GET", "/v1/inbox", None)
                .body
                .as_array()?
                .iter()
                .find(|i| i["id"] == "feed-review-r1b2c3d4")
                .cloned()
        });
        let approved = call_typed(
            &obid,
            "POST",
            "/v1/inbox/feed-review-r1b2c3d4/answer",
            &json!({"answer": "Approve"}),
        );
        assert_eq!(approved.status, 200, "{}", approved.text);

        let calls = recording.calls()[before..].to_vec();
        recording.assert_calls(&crate::recorder::polls_once(calls));
        let shown = |item: &serde_json::Value| {
            let watch = item["watch"].clone();
            json!({"id": item["id"], "kind": item["kind"], "title": item["title"], "text": item["text"],
                   "options": item["options"], "watch": watch, "state": item["state"],
                   "answer": item["answer"]})
        };
        recording.assert_answers(&json!([
            {"asked": "GET /v1/inbox, the review's item", "item": shown(&open)},
            {"asked": "POST /v1/inbox/{item}/answer Approve", "status": approved.status,
             "item": shown(&approved.body)},
        ]));
    }
}
