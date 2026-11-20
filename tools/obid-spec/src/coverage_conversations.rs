//! Nodes for the conversations area's coverage (`workflows/routes/conversations/`): what the
//! existing nodes can't reach without them. They only read or act, never judge beyond the step's own
//! `expect`, as the suite's other nodes do.
//!
//! - `conversations.person-file`: a file in a scratch obid's person's home (its `HOME`, where the
//!   stand-in `gh` records the merges and reviews it was asked for), as `home-file` reads obid's own.
//! - `conversations.kill-agent`: SIGKILL to a conversation's stand-in agent found among the scratch
//!   obid's own descendants, so a boxed Codex, whose command line doesn't name obid's home, can be
//!   killed while it waits (CONVERSATION-AGENT-45), and nothing outside this obid ever is.

use crate::{expect, obid, sphere};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// Every node of this file: its name, what it does.
pub const NODES: &[(&str, &str)] = &[
    ("conversations.person-file", "Reads a file in a scratch obid's person's home (`path`, relative), as home-file reads obid's own: {path, exists, mode, json, text}; `expect` takes exists, mode, json and text (the whole file), `within` (seconds) reads it again until it holds."),
    ("conversations.kill-agent", "Kills (SIGKILL) a scratch obid's stand-in agent (`fake-*`, never what starts it) serving `conversation`, found among that obid's own descendants by its command line naming the conversation, waiting up to `within` seconds for one: {killed: [pids]}; none found fails."),
];

pub async fn run_node(name: &str, input: &Value) -> Result<Value> {
    match name {
        "conversations.person-file" => person_file(input).await,
        "conversations.kill-agent" => kill_agent(input).await,
        other => bail!("no node named {other}"),
    }
}

async fn person_file(input: &Value) -> Result<Value> {
    let person = std::path::PathBuf::from(sphere::str_of(&input["obid"], "person")?);
    let rel = sphere::str_of(input, "path")?;
    anyhow::ensure!(
        !rel.is_empty() && !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
        "{rel} isn't a path in the person's home"
    );
    let path = person.join(rel);
    let until = Instant::now() + Duration::from_secs(input["within"].as_u64().unwrap_or(0));
    loop {
        let exists = path.exists();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let out = json!({"path": rel, "exists": exists, "mode": obid::mode_of(&path),
            "json": parsed, "text": text});
        let failures = match input.get("expect") {
            Some(want) => judge(want, &out),
            None => vec![],
        };
        if failures.is_empty() {
            let mut out = out;
            if let Some(want) = input.get("expect") {
                out["red"] = red_proof(want, &out);
            }
            return Ok(out);
        }
        if Instant::now() >= until {
            bail!("{}", failures.join("; "));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// `expect` as home-file takes it: `exists`, `mode`, `json` (a subset, or {body, paths}), `text`.
fn judge(want: &Value, out: &Value) -> Vec<String> {
    let mut failures = vec![];
    for key in ["exists", "mode"] {
        if let Some(w) = want.get(key) {
            if &out[key] != w {
                failures.push(format!("{key} is {}, expected {w}", out[key]));
            }
        }
    }
    if let Some(j) = want.get("json") {
        let j = if j.get("body").is_some() || j.get("paths").is_some() {
            j.clone()
        } else {
            json!({"body": j})
        };
        failures.extend(expect::check(&j, &out["json"], ""));
    }
    if let Some(t) = want.get("text") {
        expect::matches("text", t, &out["text"], &mut failures);
    }
    failures
}

/// The proof that the step's assertion can fail (OBIDSPEC-6), as home-file makes it: there or not
/// flipped, another mode, and its JSON and its text each made wrong at every place it looks, must
/// fail it every time.
fn red_proof(want: &Value, out: &Value) -> Value {
    let with = |key: &str, v: Value| {
        let mut o = out.clone();
        o[key] = v;
        o
    };
    let mut cases: Vec<(String, Vec<String>)> = vec![];
    if want.get("exists").is_some() {
        let flipped = !out["exists"].as_bool().unwrap_or(false);
        cases.push((
            "exists flipped".into(),
            judge(want, &with("exists", json!(flipped))),
        ));
    }
    if want.get("mode").is_some() {
        cases.push((
            "mode 0777".into(),
            judge(want, &with("mode", json!("0777"))),
        ));
    }
    if let Some(j) = want.get("json") {
        let j = if j.get("body").is_some() || j.get("paths").is_some() {
            j.clone()
        } else {
            json!({"body": j})
        };
        for m in expect::mutants(&j, 0, &out["json"], "") {
            cases.push((m.what.clone(), judge(want, &with("json", m.body))));
        }
    }
    if let Some(t) = want.get("text") {
        let text = out["text"].as_str().unwrap_or_default();
        for m in expect::mutants(&json!({"text": t}), 0, &Value::Null, text) {
            cases.push((m.what.clone(), judge(want, &with("text", json!(m.text)))));
        }
    }
    let vacuous: Vec<String> = (cases.iter())
        .filter(|(_, failures)| failures.is_empty())
        .map(|(w, _)| w.clone())
        .collect();
    json!({"mutants": cases.len(), "vacuous": vacuous})
}

/// Each process's parent, from `ps`.
fn processes() -> Result<Vec<(i32, i32, String)>> {
    let ps = std::process::Command::new("ps")
        .args(["-axww", "-o", "pid=,ppid=,command="])
        .output()
        .context("ps")?;
    Ok(String::from_utf8_lossy(&ps.stdout)
        .lines()
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let pid = words.next()?.parse().ok()?;
            let ppid = words.next()?.parse().ok()?;
            Some((pid, ppid, words.collect::<Vec<_>>().join(" ")))
        })
        .collect())
}

/// Whether `command` is a stand-in agent itself (`node …/agents/fake-…`, or the script as its own
/// program), not what starts it.
fn is_stand_in(command: &str) -> bool {
    let mut words = command.split(' ');
    let program = words.next().unwrap_or_default();
    let script = |w: &str| w.contains("/agents/fake-");
    script(program)
        || (program.rsplit('/').next() == Some("node") && words.next().is_some_and(script))
}

async fn kill_agent(input: &Value) -> Result<Value> {
    let root = input["obid"]["pid"].as_i64().context("obid.pid")? as i32;
    anyhow::ensure!(
        crate::procs::ours(root as u32),
        "process {root} isn't an obid this suite started"
    );
    let conversation = sphere::str_of(input, "conversation")?;
    anyhow::ensure!(
        uuid::Uuid::parse_str(conversation).is_ok(),
        "{conversation} isn't a conversation's id"
    );
    let named = format!("conversation-{conversation}");
    let until = Instant::now() + Duration::from_secs(input["within"].as_u64().unwrap_or(0));
    loop {
        let all = processes()?;
        let mut children: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
        for (pid, ppid, _) in &all {
            children.entry(*ppid).or_default().push(*pid);
        }
        let mut mine = BTreeSet::new();
        let mut next = vec![root];
        while let Some(p) = next.pop() {
            for c in children.get(&p).into_iter().flatten() {
                if mine.insert(*c) {
                    next.push(*c);
                }
            }
        }
        let mut killed = vec![];
        for (pid, _, command) in &all {
            if mine.contains(pid)
                && is_stand_in(command)
                && command.contains(&named)
                && unsafe { libc::kill(*pid, libc::SIGKILL) } == 0
            {
                killed.push(*pid);
            }
        }
        if !killed.is_empty() {
            return Ok(json!({"killed": killed}));
        }
        if Instant::now() >= until {
            bail!("no stand-in agent of conversation {conversation} runs under obid {root}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_person_file_assertion_fails_when_its_text_is_wrong() {
        let want = json!({"text": {"$contains": "merge"}});
        let out =
            json!({"exists": true, "mode": null, "json": null, "text": "repos/o/r/pulls/3/merge"});
        assert!(judge(&want, &out).is_empty());
        assert_eq!(red_proof(&want, &out)["vacuous"], json!([]));
        assert!(!judge(&want, &json!({"text": "nothing"})).is_empty());
    }

    #[test]
    fn only_the_stand_in_itself_is_a_stand_in() {
        assert!(is_stand_in(
            "node /x/tests/agents/fake-codex-harness -c a app-server"
        ));
        assert!(!is_stand_in(
            "/usr/bin/bwrap --ro-bind /x/tests/agents/fake-codex-harness"
        ));
    }
}
