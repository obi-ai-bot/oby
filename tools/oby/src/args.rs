//! The command line, judged whole before anything reaches obid or the token file: any mistake is a
//! usage error, exit 2.

use serde_json::{Map, Value};
use std::io::Read;
use std::time::Duration;

pub const USAGE: &str = "\
usage:
  oby status [--json]
  oby jobs [--json]
  oby job <id> [--json]
  oby run <repo> <task|-> [--kind K] [--agent A] [--account A] [--mode M] [--check C]
          [--title T] [--id KEY] [--wait] [--timeout D] [--json]
  oby wait <id> [--timeout D] [--json]
  oby stop <id> [--json]
  oby routine run <id> [--json]

A task of - is read from standard input. --kind is fix unless given. --id is the retry key: the
same key sent again gets the same job. D is a whole number and s, m or h, as 30m.

Exit codes: 0 done, 1 failed or stopped, 2 usage, 3 waits for you, 4 timed out (the job goes
on), 5 obid not reachable or no usable token.

obid is OBID_URL (http://127.0.0.1:7777 unless set; loopback only), and its token is read from
api-token in OBID_HOME (obid's own home unless set).
";

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Status {
        json: bool,
    },
    Jobs {
        json: bool,
    },
    Job {
        id: String,
        json: bool,
    },
    /// `job` is the body's `job` object, as sent.
    Run {
        job: Map<String, Value>,
        wait: bool,
        timeout: Option<Duration>,
        json: bool,
    },
    Wait {
        id: String,
        timeout: Option<Duration>,
        json: bool,
    },
    Stop {
        id: String,
        json: bool,
    },
    RoutineRun {
        id: String,
        json: bool,
    },
}

/// Why the command line isn't one oby takes.
#[derive(Debug, PartialEq)]
pub struct Usage(pub String);

struct Flags {
    positional: Vec<String>,
    values: Vec<(&'static str, String)>,
    switches: Vec<&'static str>,
}

impl Flags {
    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    fn switch(&self, name: &str) -> bool {
        self.switches.contains(&name)
    }

    fn timeout(&self) -> Result<Option<Duration>, Usage> {
        self.value("--timeout").map(duration).transpose()
    }
}

fn flags(
    args: &[String],
    values: &[&'static str],
    switches: &[&'static str],
) -> Result<Flags, Usage> {
    let mut flags = Flags {
        positional: Vec::new(),
        values: Vec::new(),
        switches: Vec::new(),
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "-" || !arg.starts_with('-') {
            flags.positional.push(arg.clone());
        } else if let Some(name) = values.iter().find(|v| **v == arg) {
            match args.next() {
                Some(value) if !value.starts_with("--") => flags.values.push((name, value.clone())),
                _ => return Err(Usage(format!("{arg} needs a value"))),
            }
        } else if let Some(name) = switches.iter().find(|s| **s == arg) {
            flags.switches.push(name);
        } else {
            return Err(Usage(format!("{arg} isn't a flag oby knows")));
        }
    }
    Ok(flags)
}

/// The positional arguments, exactly as many as `names`.
fn exactly<'a>(flags: &'a Flags, names: &[&str]) -> Result<&'a [String], Usage> {
    match flags.positional.len().cmp(&names.len()) {
        std::cmp::Ordering::Less => Err(Usage(format!(
            "{} is missing",
            names[flags.positional.len()]
        ))),
        std::cmp::Ordering::Greater => Err(Usage(format!(
            "{} wasn't expected",
            flags.positional[names.len()]
        ))),
        std::cmp::Ordering::Equal => Ok(&flags.positional),
    }
}

/// A positive whole number of seconds, minutes or hours: `30s`, `30m`, `2h`.
pub fn duration(text: &str) -> Result<Duration, Usage> {
    let bad = || {
        Usage(format!(
            "--timeout {text} isn't a whole number and s, m or h, as 30m"
        ))
    };
    let unit = match text.chars().last() {
        Some('s') => 1,
        Some('m') => 60,
        Some('h') => 3600,
        _ => return Err(bad()),
    };
    let digits = &text[..text.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let seconds = digits
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(unit))
        .filter(|s| *s > 0);
    seconds.map(Duration::from_secs).ok_or_else(bad)
}

const RUN_FIELDS: [(&str, &str); 6] = [
    ("--agent", "agent"),
    ("--account", "account"),
    ("--mode", "mode"),
    ("--check", "check"),
    ("--title", "title"),
    ("--id", "clientRequestId"),
];

/// The command `args` names; `stdin` is read only for a task of `-`.
pub fn parse(args: &[String], stdin: &mut dyn Read) -> Result<Command, Usage> {
    let Some((name, rest)) = args.split_first() else {
        return Err(Usage("no command given".to_string()));
    };
    let json = ["--json"];
    match name.as_str() {
        "--help" | "-h" => Ok(Command::Help),
        "status" | "jobs" => {
            let flags = flags(rest, &[], &json)?;
            exactly(&flags, &[])?;
            let json = flags.switch("--json");
            Ok(if name == "status" {
                Command::Status { json }
            } else {
                Command::Jobs { json }
            })
        }
        "job" | "stop" => {
            let flags = flags(rest, &[], &json)?;
            let id = exactly(&flags, &["<id>"])?[0].clone();
            let json = flags.switch("--json");
            Ok(if name == "job" {
                Command::Job { id, json }
            } else {
                Command::Stop { id, json }
            })
        }
        "wait" => {
            let flags = flags(rest, &["--timeout"], &json)?;
            let id = exactly(&flags, &["<id>"])?[0].clone();
            Ok(Command::Wait {
                id,
                timeout: flags.timeout()?,
                json: flags.switch("--json"),
            })
        }
        "routine" => {
            let flags = flags(rest, &[], &json)?;
            let given = exactly(&flags, &["run", "<id>"])?;
            if given[0] != "run" {
                return Err(Usage(format!(
                    "routine {} isn't a command oby knows",
                    given[0]
                )));
            }
            Ok(Command::RoutineRun {
                id: given[1].clone(),
                json: flags.switch("--json"),
            })
        }
        "run" => {
            let mut values = vec!["--kind", "--timeout"];
            values.extend(RUN_FIELDS.map(|(flag, _)| flag));
            let flags = flags(rest, &values, &["--wait", "--json"])?;
            let given = exactly(&flags, &["<repo>", "<task>"])?;
            let task = if given[1] == "-" {
                let mut text = String::new();
                stdin.read_to_string(&mut text).map_err(|_| {
                    Usage("the task on standard input isn't UTF-8 text".to_string())
                })?;
                text.trim_end().to_string()
            } else {
                given[1].clone()
            };
            if task.trim().is_empty() {
                return Err(Usage("the task is empty".to_string()));
            }
            let (wait, timeout) = (flags.switch("--wait"), flags.timeout()?);
            if timeout.is_some() && !wait {
                return Err(Usage("--timeout is for --wait".to_string()));
            }
            let mut job = Map::new();
            job.insert("repo".into(), given[0].clone().into());
            job.insert("kind".into(), flags.value("--kind").unwrap_or("fix").into());
            job.insert("task".into(), task.into());
            for (flag, key) in RUN_FIELDS {
                if let Some(value) = flags.value(flag) {
                    job.insert(key.into(), value.into());
                }
            }
            Ok(Command::Run {
                job,
                wait,
                timeout,
                json: flags.switch("--json"),
            })
        }
        other => Err(Usage(format!("{other} isn't a command oby knows"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str], stdin: &str) -> Result<Command, Usage> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse(&args, &mut stdin.as_bytes())
    }

    #[test]
    fn a_run_sends_only_the_flags_given() {
        let Ok(Command::Run {
            job,
            wait,
            timeout,
            json,
        }) = parsed(
            &[
                "run",
                "o/r",
                "fix it",
                "--id",
                "k1",
                "--wait",
                "--timeout",
                "30m",
            ],
            "",
        )
        else {
            panic!("not a run");
        };
        assert_eq!(
            Value::Object(job),
            serde_json::json!({"repo": "o/r", "kind": "fix", "task": "fix it", "clientRequestId": "k1"})
        );
        assert_eq!(
            (wait, timeout, json),
            (true, Some(Duration::from_secs(1800)), false)
        );
    }

    #[test]
    fn a_task_of_dash_is_standard_input() {
        let Ok(Command::Run { job, .. }) =
            parsed(&["run", "o/r", "-", "--kind", "investigate"], "say hi\n")
        else {
            panic!("not a run");
        };
        assert_eq!(
            (job["task"].as_str(), job["kind"].as_str()),
            (Some("say hi"), Some("investigate"))
        );
        assert!(parsed(&["run", "o/r", "-"], " \n").is_err());
    }

    #[test]
    fn mistakes_are_usage_errors() {
        for args in [
            &[][..],
            &["frob"],
            &["run"],
            &["run", "repo"],
            &["run", "repo", ""],
            &["run", "repo", "task", "--bogus"],
            &["run", "repo", "task", "--timeout", "5x", "--wait"],
            &["run", "repo", "task", "--timeout", "5m"],
            &["run", "repo", "task", "--title"],
            &["run", "repo", "task", "--title", "--wait"],
            &["run", "repo", "task", "extra"],
            &["wait"],
            &["wait", "j1", "--timeout", "0s"],
            &["wait", "j1", "--timeout", "m"],
            &["wait", "j1", "--timeout", "-5m"],
            &["wait", "j1", "--timeout", "99999999999999999999h"],
            &["job"],
            &["stop", "j1", "j2"],
            &["routine", "run"],
            &["routine", "pause", "r1"],
            &["status", "--wait"],
            &["jobs", "x"],
        ] {
            assert!(matches!(parsed(args, ""), Err(Usage(_))), "{args:?}");
        }
    }

    #[test]
    fn each_command_is_read() {
        assert_eq!(parsed(&["-h"], ""), Ok(Command::Help));
        assert_eq!(
            parsed(&["status", "--json"], ""),
            Ok(Command::Status { json: true })
        );
        assert_eq!(parsed(&["jobs"], ""), Ok(Command::Jobs { json: false }));
        assert_eq!(
            parsed(&["job", "j1"], ""),
            Ok(Command::Job {
                id: "j1".into(),
                json: false
            })
        );
        assert_eq!(
            parsed(&["stop", "j1", "--json"], ""),
            Ok(Command::Stop {
                id: "j1".into(),
                json: true
            })
        );
        assert_eq!(
            parsed(&["wait", "j1", "--timeout", "2h"], ""),
            Ok(Command::Wait {
                id: "j1".into(),
                timeout: Some(Duration::from_secs(7200)),
                json: false
            })
        );
        assert_eq!(
            parsed(&["routine", "run", "r1"], ""),
            Ok(Command::RoutineRun {
                id: "r1".into(),
                json: false
            })
        );
    }
}
