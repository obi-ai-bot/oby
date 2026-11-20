//! Nodes for the conversation bugs T3 Code fixed (`workflows/t3/conversations/`, tag `t3`): what a
//! conversation's turn sent on the event stream, in the order it came and how many times, a
//! stream followed again as a reconnecting `EventSource` does, and a stand-in agent that dies in
//! the middle of a turn.

use crate::{events, expect, sphere};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Every node of this file: its name, what it does.
pub const NODES: &[(&str, &str)] = &[
    ("t3.follow", "Follows GET /v1/events as `name` (the obid's base by default), from `after` and/or with a `Last-Event-ID` header (`lastEventId`), as a reconnecting EventSource does; events-wait and t3.trail then read it by that name."),
    ("t3.trail", "One conversation's events on a followed stream (`name`, or the obid's), in the order they came: once `until` ({event, expect}) has come (waiting up to `seconds`) and `settle` ms more, `turn` keeps only a turn's own (its `turn` or `id`); {trail: one line per event of `events`, its name then each `show` path's value; ids; revisions; deltaText: its `delta` texts joined; count}; with expect."),
    ("t3.kill-agent", "Kills (SIGKILL) a scratch obid's stand-in agent (`fake-*`, not what starts it) serving `conversation`, found by its command line, which names the obid's home and the conversation, skipping the pids in `not` (one killed before) and waiting up to `within` seconds for one: {killed: [pids]}; none found fails. With `heard` (a prompt's last line), every try instead: each stand-in that has read those words is killed once it has (the first within `within` seconds, default 20), again for each try obid makes, until none reads them for `quiet` seconds (default 10): {killed: [pids], tries}."),
    ("t3.heard", "What a conversation's stand-in agent heard, from its own record (`conversations/<id>/config/turns.jsonl` in the obid's home), oldest first: {heard: [{process, resumed, cut: whether obid's note said its run was cut, last: the prompt's last line}], count}; with expect and within (seconds)."),
];

pub async fn run_node(name: &str, input: &Value) -> Result<Value> {
    let mut out = match name {
        "t3.follow" => follow(input).await?,
        "t3.trail" => trail(input).await?,
        "t3.kill-agent" => kill_agent(input).await?,
        "t3.heard" => heard(input).await?,
        other => bail!("no node named {other}"),
    };
    if let Some(want) = input.get("expect") {
        let failures = expect::check(want, &out, "");
        if !failures.is_empty() {
            bail!("{}", failures.join("; "));
        }
        out["red"] = red_proof(want, &out);
    }
    Ok(out)
}

/// The proof that the step's assertion can fail (OBIDSPEC-6), as nodes.rs makes it for its own.
fn red_proof(want: &Value, out: &Value) -> Value {
    let all = expect::mutants(want, 0, out, "");
    let vacuous: Vec<String> = all
        .iter()
        .filter(|m| expect::check(want, &m.body, &m.text).is_empty())
        .map(|m| m.what.clone())
        .collect();
    json!({"mutants": all.len(), "vacuous": vacuous})
}

async fn follow(input: &Value) -> Result<Value> {
    let base = sphere::str_of(&input["obid"], "base")?;
    let key = input["name"].as_str().unwrap_or(base).to_string();
    events::follow_from(
        base,
        sphere::str_of(&input["obid"], "token")?,
        input["after"].as_str(),
        input["lastEventId"].as_str(),
        &key,
    )
    .await
}

fn key_of(input: &Value) -> Result<String> {
    match input["name"].as_str() {
        Some(n) => Ok(n.to_string()),
        None => Ok(sphere::str_of(&input["obid"], "base")?.to_string()),
    }
}

/// Whether an event is about `conversation`: its data names it, as `turn`, `turnDelivery`,
/// `delta`, `step`, `reply`, `thinking` and `underway` do.
fn of_conversation(data: &Value, conversation: &str) -> bool {
    data["conversation"].as_str() == Some(conversation)
}

async fn trail(input: &Value) -> Result<Value> {
    let key = key_of(input)?;
    let conversation = sphere::str_of(input, "conversation")?.to_string();
    let names: Vec<String> = input["events"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|n| n.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let show: Vec<String> = input["show"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|n| n.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let until = Instant::now() + Duration::from_secs(input["seconds"].as_u64().unwrap_or(30));
    if let Some(u) = input.get("until") {
        let event = sphere::str_of(u, "event")?;
        let want = u.get("expect").cloned().unwrap_or(json!({}));
        loop {
            let seen = events::seen(&key).with_context(|| {
                format!("{key}'s events aren't followed: an events-follow or t3.follow step first")
            })?;
            if seen.iter().any(|e| {
                e.name == event
                    && of_conversation(&e.data, &conversation)
                    && expect::check(&want, &e.data, "").is_empty()
            }) {
                break;
            }
            if Instant::now() > until {
                bail!("no {event} event of conversation {conversation} matching {want} came");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    // What could still come after it, a duplicate say, is waited for too.
    tokio::time::sleep(Duration::from_millis(
        input["settle"].as_u64().unwrap_or(500),
    ))
    .await;
    let seen = events::seen(&key).context("not followed")?;
    let mine: Vec<&events::Event> = seen
        .iter()
        .filter(|e| of_conversation(&e.data, &conversation))
        .collect();
    let turn = input["turn"].as_str();
    let picked: Vec<&&events::Event> = mine
        .iter()
        .filter(|e| names.is_empty() || names.contains(&e.name))
        .filter(|e| {
            turn.is_none_or(|t| {
                e.data["turn"].as_str() == Some(t) || e.data["id"].as_str() == Some(t)
            })
        })
        .collect();
    let line = |e: &events::Event| {
        let mut l = e.name.clone();
        for p in &show {
            let v = p.split('.').try_fold(&e.data, |v, k| v.get(k));
            match v {
                Some(Value::String(s)) => l.push_str(&format!(" {s}")),
                Some(Value::Null) | None => {}
                Some(other) => l.push_str(&format!(" {other}")),
            }
        }
        l
    };
    let delta_text: String = mine
        .iter()
        .filter(|e| e.name == "delta")
        .filter_map(|e| e.data["text"].as_str())
        .collect();
    Ok(json!({
        "trail": picked.iter().map(|e| line(e)).collect::<Vec<_>>(),
        "ids": picked.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
        "revisions": picked.iter().map(|e| e.data["revision"].clone()).collect::<Vec<_>>(),
        "events": picked.iter().map(|e| json!({"event": e.name, "id": e.id, "data": e.data})).collect::<Vec<_>>(),
        "deltaText": delta_text,
        "count": picked.len(),
    }))
}

/// Whether `command` is a stand-in agent itself (`node …/agents/fake-harness …`, or the script run
/// as its own program), not what starts it: obid's launcher, the shells that wait for its network,
/// the sandbox.
fn is_stand_in(command: &str) -> bool {
    let mut words = command.split(' ');
    let program = words.next().unwrap_or_default();
    let script = |w: &str| w.contains("/agents/fake-");
    script(program)
        || (program.rsplit('/').next() == Some("node") && words.next().is_some_and(script))
}

/// The stand-in agents' processes for `conversation`: a command line holding a `fake-` agent's
/// path, the obid's home and the conversation's id (its `--mcp-config` lives there), so nothing but
/// this workflow's own stand-in is ever signalled.
async fn kill_agent(input: &Value) -> Result<Value> {
    let home = sphere::str_of(&input["obid"], "home")?;
    let conversation = sphere::str_of(input, "conversation")?;
    let mut skip: Vec<i64> = input["not"]
        .as_array()
        .map(|l| l.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    if let Some(words) = input["heard"].as_str() {
        return kill_every_try(input, home, conversation, words, skip).await;
    }
    let until = Instant::now() + Duration::from_secs(input["within"].as_u64().unwrap_or(0));
    loop {
        let (killed, commands) = kill_stand_ins(home, conversation, &mut skip)?;
        if !killed.is_empty() {
            return Ok(json!({"killed": killed, "commands": commands}));
        }
        if Instant::now() >= until {
            bail!("no stand-in agent of conversation {conversation} is running");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// [`kill_agent`] with `heard`: a turn that dies on every try, however many obid makes. Each try's
/// stand-in is killed only once it has read the words (its line in turns.jsonl), so each kill
/// cuts a turn under way, never a process not yet handed them.
async fn kill_every_try(
    input: &Value,
    home: &str,
    conversation: &str,
    words: &str,
    mut skip: Vec<i64>,
) -> Result<Value> {
    let first = Duration::from_secs(input["within"].as_u64().unwrap_or(20));
    let quiet = Duration::from_secs(input["quiet"].as_u64().unwrap_or(10));
    let (mut killed, mut tries) = (vec![], 0);
    let mut last = Instant::now();
    loop {
        let heard = heard_of(input)?
            .iter()
            .filter(|h| h["last"] == words)
            .count();
        if heard > tries {
            let (pids, _) = kill_stand_ins(home, conversation, &mut skip)?;
            if !pids.is_empty() {
                killed.extend(pids);
                tries = heard;
                last = Instant::now();
            }
        }
        if tries == 0 && last.elapsed() >= first {
            bail!("no stand-in of conversation {conversation} read {words:?}");
        }
        if tries > 0 && last.elapsed() >= quiet {
            return Ok(json!({"killed": killed, "tries": tries}));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// SIGKILLs every stand-in of `conversation` not in `skip`, adding each to it: {pids, commands}.
fn kill_stand_ins(
    home: &str,
    conversation: &str,
    skip: &mut Vec<i64>,
) -> Result<(Vec<i32>, Vec<String>)> {
    let ps = std::process::Command::new("ps")
        .args(["-axww", "-o", "pid=,command="])
        .output()
        .context("ps")?;
    let (mut killed, mut commands) = (vec![], vec![]);
    for l in String::from_utf8_lossy(&ps.stdout).lines() {
        let Some((pid, command)) = l.trim_start().split_once(' ') else {
            continue;
        };
        let Ok(pid) = pid.parse::<i32>() else {
            continue;
        };
        if is_stand_in(command)
            && command.contains(home)
            && command.contains(conversation)
            && !skip.contains(&(pid as i64))
            && unsafe { libc::kill(pid, libc::SIGKILL) } == 0
        {
            skip.push(pid as i64);
            killed.push(pid);
            commands.push(command.chars().take(160).collect::<String>());
        }
    }
    Ok((killed, commands))
}

/// The stand-in's own record of each prompt it heard (tests/agents/fake-harness: one line of
/// turns.jsonl per message), shortened to what a step asserts. `within`: read again until `expect`
/// holds, for a try still being made.
async fn heard(input: &Value) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(input["within"].as_u64().unwrap_or(0));
    loop {
        let heard = heard_of(input)?;
        let out = json!({"count": heard.len(), "heard": heard});
        let holds = input
            .get("expect")
            .is_none_or(|want| expect::check(want, &out, "").is_empty());
        if holds || Instant::now() >= until {
            return Ok(out);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// What a conversation's stand-in heard, oldest first ([`heard`]).
fn heard_of(input: &Value) -> Result<Vec<Value>> {
    let home = std::path::PathBuf::from(sphere::str_of(&input["obid"], "home")?);
    let conversation = sphere::str_of(input, "conversation")?;
    anyhow::ensure!(
        !conversation.contains('/') && !conversation.contains(".."),
        "{conversation} isn't a conversation's id"
    );
    let path = home
        .join("conversations")
        .join(conversation)
        .join("config/turns.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    Ok(text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|t| {
            let prompt = t["prompt"].as_str().unwrap_or_default();
            json!({
                "process": t["id"],
                "resumed": t["resumed"],
                "cut": prompt.contains("Your run was cut"),
                "last": prompt.lines().last().unwrap_or_default(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::is_stand_in;

    #[test]
    fn only_the_stand_in_itself_is_killed_never_what_starts_it() {
        let agent = "/repo/core/crates/obid/tests/agents/fake-harness";
        assert!(is_stand_in(&format!(
            "node {agent} -p --mcp-config /h/c/oby-mcp.json"
        )));
        assert!(is_stand_in(&format!("/usr/bin/node {agent} -p")));
        assert!(is_stand_in(&format!("{agent} -p")));
        for launcher in [
            format!("/bin/bash -c ready=$1 shift {agent}"),
            format!("/bin/sh -c while [ ! -f \"$1\" ]; do sleep 0.05; done net-wait {agent}"),
            format!("/repo/core/target/debug/obid __session -- /bin/bash -c {agent}"),
            format!("/usr/bin/bwrap --ro-bind / / {agent}"),
        ] {
            assert!(!is_stand_in(&launcher), "{launcher}");
        }
    }
}
