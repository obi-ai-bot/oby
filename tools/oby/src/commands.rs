//! Each subcommand, on obid's existing routes (docs/api/jobs.md, inbox.md, routines.md,
//! capabilities.md, account-limits.md). oby signs nothing, approves nothing and answers nothing.

use crate::args::Command;
use crate::exit::{exit_for, Asks, Exit, Status};
use crate::http::{Answer, Client};
use crate::{endpoint, out, token, Fail};
use serde_json::{Map, Value};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// How often a wait reads the job and the inbox beside `/wait`.
const POLL: Duration = Duration::from_secs(2);
/// The inbox kinds that wait for the owner.
const OWNER_KINDS: [&str; 6] = [
    "permission",
    "question",
    "agentQuestion",
    "plan",
    "confirm",
    "limit",
];

pub fn run(command: Command) -> Result<Exit, Fail> {
    let endpoint = endpoint::from_env().map_err(|why| Fail::new(Exit::Unreachable, why))?;
    let home = endpoint::home().map_err(|why| Fail::new(Exit::Unreachable, why))?;
    let client = Arc::new(Client::new(endpoint, home.join(token::FILE))?);
    match command {
        Command::Help => Ok(Exit::Done),
        Command::Status { json } => {
            let answer = client.send("GET", "/v1/status", None)?;
            if answer.status == 503 {
                return Err(Fail::new(Exit::Unreachable, answer.refused().message));
            }
            let status = ok(answer)?;
            show(&status, json, |v| {
                let runner = &v["runner"];
                vec![
                    ("engine", one_line(&runner["engine"])),
                    ("version", one_line(&runner["version"])),
                ]
            });
            Ok(Exit::Done)
        }
        Command::Jobs { json } => {
            let answer = ok(client.send("GET", "/v1/jobs?limit=50", None)?)?;
            if json {
                out(&answer.text());
            } else {
                for job in answer.json().as_array().into_iter().flatten() {
                    out(&format!(
                        "id={} status={} repo={}",
                        one_line(&job["id"]),
                        one_line(&job["status"]),
                        one_line(&job["repo"])
                    ));
                }
            }
            Ok(Exit::Done)
        }
        Command::Job { id, json } => {
            let answer = ok(client.send("GET", &format!("/v1/jobs/{}", segment(&id)), None)?)?;
            show(&answer, json, |v| {
                ["id", "status", "repo", "branch", "pr", "step"]
                    .map(|k| (k, one_line(&v[k])))
                    .to_vec()
            });
            Ok(Exit::Done)
        }
        Command::Stop { id, json } => {
            let answer =
                ok(client.send("POST", &format!("/v1/jobs/{}/stop", segment(&id)), None)?)?;
            show(&answer, json, |v| {
                vec![
                    ("job", one_line(&v["id"])),
                    ("status", one_line(&v["status"])),
                ]
            });
            Ok(Exit::Done)
        }
        Command::RoutineRun { id, json } => {
            let answer =
                client.send("POST", &format!("/v1/routines/{}/run", segment(&id)), None)?;
            if answer.status != 201 {
                return Err(answer.refused());
            }
            show(&answer, json, |v| vec![("job", one_line(&v["job"]["id"]))]);
            Ok(Exit::Done)
        }
        Command::Run {
            job,
            wait: then_wait,
            timeout,
            json,
        } => {
            let body = Value::Object(Map::from_iter([("job".to_string(), Value::Object(job))]))
                .to_string();
            let answer = ok(client.send("POST", "/v1/jobs", Some(body.as_bytes()))?)?;
            let id = one_line(&answer.json()["id"]);
            if !then_wait {
                show(&answer, json, |_| vec![("job", id.clone())]);
                return Ok(Exit::Done);
            }
            // With --json, stdout is the wait's Job alone; else the id shows at once.
            if !json {
                out(&format!("job={id}"));
            }
            wait(client, &id, timeout, json)
        }
        Command::Wait { id, timeout, json } => wait(client, &id, timeout, json),
    }
}

/// A 2xx, else obid's error: exit 1.
fn ok(answer: Answer) -> Result<Answer, Fail> {
    if answer.ok() {
        Ok(answer)
    } else {
        Err(answer.refused())
    }
}

/// obid's JSON as it came with `--json`, else `key=value` lines.
fn show(answer: &Answer, json: bool, lines: impl Fn(&Value) -> Vec<(&'static str, String)>) {
    if json {
        out(&answer.text());
    } else {
        for (key, value) in lines(&answer.json()) {
            out(&format!("{key}={value}"));
        }
    }
}

/// A value on one line: text with its newlines as spaces, nothing for null or missing.
fn one_line(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.replace(['\n', '\r'], " "),
        other => other.to_string(),
    }
}

/// An id as one path segment, whatever it holds.
fn segment(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The job as last read: its JSON as obid sent it, and as read.
struct Job {
    text: String,
    value: Value,
}

impl Job {
    fn of(answer: &Answer) -> Option<Job> {
        let value = answer.json();
        value.is_object().then(|| Job {
            text: answer.text(),
            value,
        })
    }

    fn status(&self) -> Status {
        self.value["status"]
            .as_str()
            .map_or(Status::Unknown, Status::named)
    }
}

/// The owner's item a job waits on: its id and kind.
struct Ask {
    item: String,
    kind: String,
}

/// An open inbox item of the owner's that names the job, else a pending limit decision on it.
fn ask(job: &Value, inbox: &Value, id: &str) -> Option<Ask> {
    let item = inbox.as_array().into_iter().flatten().find(|item| {
        item["job"].as_str() == Some(id)
            && item["state"].as_str() != Some("done")
            && item["kind"]
                .as_str()
                .is_some_and(|k| OWNER_KINDS.contains(&k))
    });
    if let Some(item) = item {
        return Some(Ask {
            item: one_line(&item["id"]),
            kind: one_line(&item["kind"]),
        });
    }
    let limit = &job["accountLimit"];
    (limit["state"].as_str() == Some("pending")).then(|| Ask {
        item: one_line(&limit["decision"]),
        kind: "limit".to_string(),
    })
}

/// What a wait's threads saw.
enum Seen {
    Ended(Option<Job>),
    Read(Option<Job>, Option<Ask>),
}

/// `/wait` held open on a thread of its own, the job and the inbox read every 2 seconds on another,
/// and the deadline checked here, until `exit_for` says how it ends. A timeout stops nothing.
fn wait(
    client: Arc<Client>,
    id: &str,
    timeout: Option<Duration>,
    json: bool,
) -> Result<Exit, Fail> {
    let deadline = timeout.map(|t| Instant::now() + t);
    let (tx, rx) = mpsc::channel::<Result<Seen, Fail>>();
    let path = format!("/v1/jobs/{}", segment(id));
    {
        let (client, tx, path) = (client.clone(), tx.clone(), format!("{path}/wait"));
        thread::spawn(move || {
            let seen = client
                .send("GET", &path, None)
                .and_then(ok)
                .map(|a| Seen::Ended(Job::of(&a)));
            let _ = tx.send(seen);
        });
    }
    {
        let id = id.to_string();
        thread::spawn(move || loop {
            let seen = (|| {
                let job = ok(client.send("GET", &path, None)?)?;
                let inbox = ok(client.send("GET", "/v1/inbox", None)?)?;
                let job = Job::of(&job);
                let ask = job.as_ref().and_then(|j| ask(&j.value, &inbox.json(), &id));
                Ok(Seen::Read(job, ask))
            })();
            let failed = seen.is_err();
            if tx.send(seen).is_err() || failed {
                return;
            }
            thread::sleep(POLL);
        });
    }
    let (mut job, mut asked) = (None::<Job>, None::<Ask>);
    loop {
        let timed_out = deadline.is_some_and(|d| Instant::now() >= d);
        let status = job.as_ref().map_or(Status::Unknown, Job::status);
        let asks = if asked.is_some() {
            Asks::Owner
        } else {
            Asks::Nobody
        };
        if let Some(exit) = exit_for(status, asks, timed_out) {
            let shown = job
                .as_ref()
                .map_or(String::new(), |j| one_line(&j.value["status"]));
            match (&job, json) {
                (Some(job), true) => out(&job.text),
                _ => out(&format!("job={id}\nstatus={shown}")),
            }
            if let (Exit::Owner, Some(ask)) = (exit, &asked) {
                let line = format!("inbox={}\nkind={}", ask.item, ask.kind);
                if json {
                    eprintln!("{line}");
                } else {
                    out(&line);
                }
            }
            return Ok(exit);
        }
        let got = match deadline {
            Some(d) => rx.recv_timeout(d.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match got {
            Ok(Ok(Seen::Ended(seen))) => job = seen.or(job),
            Ok(Ok(Seen::Read(seen, ask))) => (job, asked) = (seen.or(job), ask),
            Ok(Err(fail)) => return Err(fail),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(Fail::new(Exit::Unreachable, "obid stopped answering"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_id_is_one_path_segment() {
        assert_eq!(segment("9f08fd10-ab_C"), "9f08fd10-ab_C");
        assert_eq!(segment("../stop?x"), "%2E%2E%2Fstop%3Fx");
    }

    #[test]
    fn only_the_owners_open_items_on_this_job_are_asks() {
        let job = json!({"id": "j1", "accountLimit": null});
        let item = |id: &str, kind: &str, on: &str, state: &str| json!({"id": id, "kind": kind, "job": on, "state": state});
        let none = json!([
            item("a", "failed", "j1", "open"),
            item("b", "question", "j2", "open"),
            item("c", "permission", "j1", "done")
        ]);
        assert!(ask(&job, &none, "j1").is_none());
        for kind in OWNER_KINDS {
            let found = ask(&job, &json!([item("x", kind, "j1", "open")]), "j1").unwrap();
            assert_eq!((found.item.as_str(), found.kind.as_str()), ("x", kind));
        }
        let limited = json!({"id": "j1", "accountLimit": {"decision": "d1", "state": "pending", "resetAt": null}});
        let found = ask(&limited, &json!([]), "j1").unwrap();
        assert_eq!((found.item.as_str(), found.kind.as_str()), ("d1", "limit"));
        let resumed = json!({"id": "j1", "accountLimit": {"decision": "d1", "state": "resume"}});
        assert!(ask(&resumed, &json!([]), "j1").is_none());
    }

    #[test]
    fn values_show_on_one_line() {
        assert_eq!(one_line(&json!("a\nb\r\nc")), "a b  c");
        assert_eq!(one_line(&json!(33)), "33");
        assert_eq!(one_line(&Value::Null), "");
    }
}
