//! Adding a computer over SSH from the window (docs/contracts/remote.md, COMPUTERS-25), the Mac's
//! side of `obid pair-computer` (COMPUTERS-7): the owner's signed act names a host as they'd type
//! it in a terminal, and obid runs their own ssh with COMPUTERS-4's options, once, to run
//! `obid pair-computer` there with this Mac's name and its device key on stdin (never on a command
//! line). The invitation it prints names that obid's gateway on its own 127.0.0.1: obid opens its
//! tunnel there (COMPUTERS-12) and redeems the invitation through it, then keeps the computer with
//! the faster direct routes its link holds, each pinned, and the SSH route last.
//!
//! Each step is told to the window as it happens: `computer.adding {"id", "step": "ssh" | "pair" |
//! "routes", "state": "doing" | "done" | "failed", "trouble"?}`. A failed step says why in words the
//! person can act on, a host key always theirs to accept (COMPUTERS-5), and keeps nothing: a
//! pairing made there that this Mac then can't keep is ended there again.
//!
//! It owns only the adding while it runs. The computer, once added, is crate::remote::computers'.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt as _;

use super::link::Link;
use super::route::{Route, rank};
use super::trouble::{SshTrouble, trouble};
use crate::api::Shared;

/// What runs there: obid's own pairing program, one plain command any login shell parses alike.
pub const PAIR_COMMAND: &str = "obid pair-computer";

/// How long ssh and `obid pair-computer` have, together.
const PAIRING: Duration = Duration::from_secs(60);

/// The id of the SSH route a computer added this way keeps.
const SSH_ROUTE: &str = "ssh";

/// What the owner asked: the host, and this Mac's device that signed, its key and this Mac's name.
pub struct Asked {
    pub destination: String,
    pub signer: String,
    pub key: String,
    pub name: String,
}

/// A step that failed, and why.
struct Failed {
    step: &'static str,
    trouble: Value,
}

fn failed(step: &'static str, kind: &str, say: impl Into<String>) -> Failed {
    Failed {
        step,
        trouble: json!({"trouble": kind, "say": say.into()}),
    }
}

fn ssh_failed(step: &'static str, t: &SshTrouble, destination: &str) -> Failed {
    let mut trouble = serde_json::to_value(t).unwrap_or_default();
    trouble["say"] = json!(t.say(destination));
    Failed { step, trouble }
}

/// Starts adding `asked.destination`; the answer is the adding's id, its steps told as events.
pub fn start(shared: Arc<Shared>, asked: Asked) -> Result<String, String> {
    let id = {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
        hex::encode(bytes)
    };
    let adding = id.clone();
    tokio::spawn(async move {
        let result = add(&shared, &adding, &asked).await;
        // Whatever it came to, the tunnel the adding used goes: the computer's own opens anew.
        shared.computers.tunnels.forget(&adding);
        match result {
            Ok(computer) => {
                tell(
                    &shared,
                    &adding,
                    "routes",
                    "done",
                    json!({"computer": computer}),
                );
                shared.events.send("computers", shared.computers.list());
            }
            Err(Failed { step, trouble }) => {
                tell(
                    &shared,
                    &adding,
                    step,
                    "failed",
                    json!({"trouble": trouble}),
                );
            }
        }
    });
    Ok(id)
}

fn tell(shared: &Shared, id: &str, step: &str, state: &str, more: Value) {
    let mut data = json!({"id": id, "step": step, "state": state});
    if let (Some(data), Some(more)) = (data.as_object_mut(), more.as_object()) {
        data.extend(more.clone());
    }
    shared.events.send("computer.adding", data);
}

async fn add(shared: &Arc<Shared>, id: &str, asked: &Asked) -> Result<Value, Failed> {
    let destination = asked.destination.as_str();
    tell(shared, id, "ssh", "doing", json!({}));
    let said = pair_there(asked).await?;
    tell(shared, id, "ssh", "done", json!({}));

    tell(shared, id, "pair", "doing", json!({}));
    let invite = &said["invite"];
    let secret = invite["secret"].as_str().unwrap_or_default();
    let remote = invite["url"]
        .as_str()
        .and_then(|url| url.rsplit(':').next())
        .and_then(|port| port.trim_end_matches('/').parse::<u16>().ok());
    let (Some(remote), false) = (remote, secret.is_empty()) else {
        return Err(failed(
            "pair",
            "refusedThere",
            format!("Oby on {destination} answered no invitation this Mac can use."),
        ));
    };
    let ssh_route =
        Route::ssh(SSH_ROUTE, destination, remote).map_err(|why| failed("pair", "other", why))?;
    let local = shared
        .computers
        .tunnels
        .port(&super::reach::ssh(), id, SSH_ROUTE, destination, remote)
        .await
        .map_err(|why| failed("pair", "unreachable", why))?;
    let way = super::reach::Way {
        client: super::reach::plain(true).map_err(|why| failed("pair", "other", why))?,
        base: format!("http://127.0.0.1:{local}"),
    };
    let redeemed = way
        .client
        .post(format!("{}/v1/pairing", way.base))
        .json(&json!({"credential": secret, "name": asked.name}))
        .send()
        .await
        .map_err(|e| {
            failed(
                "pair",
                "unreachable",
                format!(
                    "Oby on {destination} didn't answer through the SSH tunnel: {}",
                    super::pinned::said(&e)
                ),
            )
        })?;
    let ok = redeemed.status().is_success();
    let body: Value = redeemed.json().await.unwrap_or(Value::Null);
    let (true, Some(token), Some(pairing)) = (
        ok,
        body["token"].as_str().map(str::to_owned),
        body["deviceID"].as_str().map(str::to_owned),
    ) else {
        return Err(failed(
            "pair",
            "refusedThere",
            format!(
                "Oby on {destination} refused this Mac: {}",
                body["error"].as_str().unwrap_or("no reason given")
            ),
        ));
    };
    tell(shared, id, "pair", "done", json!({}));

    tell(shared, id, "routes", "doing", json!({}));
    let status = super::reach::status(&way, &token).await.ok();
    let link = invite["computerLink"]
        .as_str()
        .and_then(|l| Link::decode(l).ok());
    // Its own name, as its obid says: the link's is this Mac's, the invitation's guest.
    let name = status
        .as_ref()
        .and_then(|s| s["runner"]["host"].as_str())
        .filter(|h| !h.trim().is_empty())
        .unwrap_or(destination)
        .to_owned();
    let mut routes: Vec<Route> = link
        .map(|l| l.routes)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.id != SSH_ROUTE)
        .collect();
    routes.push(ssh_route);
    let paired = super::code::Paired {
        name,
        routes: rank(routes),
        route: SSH_ROUTE.into(),
        token: token.clone(),
        pairing,
        device: said["device"]["id"].as_str().map(str::to_owned),
    };
    let os = status.as_ref().and_then(super::reach::os);
    let computer = match shared.computers.add(paired, os, Some(asked.signer.clone())) {
        Ok(computer) => computer,
        Err(why) => {
            // Kept nothing here: the pairing there ends too, so no token of it is left anywhere.
            let _ = way
                .client
                .delete(format!("{}/v1/pairing", way.base))
                .bearer_auth(&token)
                .send()
                .await;
            return Err(failed("routes", "notKept", why));
        }
    };
    // Its direct routes are tried now, best first, so the window starts on the fastest that
    // answers; SSH, which just did, is what it starts on when none does.
    let mut answered = false;
    for route in computer.routes.iter().filter(|r| r.id != SSH_ROUTE) {
        let result = match super::reach::way(shared, &computer.id, route, true).await {
            Ok(way) => super::reach::status(&way, &token).await.map(|_| ()),
            Err(why) => Err(why),
        };
        answered = result.is_ok();
        shared.computers.observe(&computer.id, &route.id, result);
        if answered {
            break;
        }
    }
    if !answered {
        shared.computers.observe(&computer.id, SSH_ROUTE, Ok(()));
    }
    Ok(shared.computers.listed(&computer))
}

/// Runs `obid pair-computer` there over the person's own ssh, this Mac's name and key on stdin, and
/// reads the one line it prints.
async fn pair_there(asked: &Asked) -> Result<Value, Failed> {
    let destination = asked.destination.as_str();
    let ssh = super::reach::ssh();
    let args = ssh
        .command_args(destination, PAIR_COMMAND)
        .map_err(|why| failed("ssh", "other", why))?;
    let mut command = ssh.command(&args);
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().map_err(|e| {
        failed(
            "ssh",
            "other",
            format!("Oby couldn't run ssh on this Mac: {e}"),
        )
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin
            .write_all(format!("{}\n{}\n", asked.name, asked.key).as_bytes())
            .await;
    }
    let output = tokio::time::timeout(PAIRING, child.wait_with_output())
        .await
        .map_err(|_| ssh_failed("ssh", &SshTrouble::Unreachable, destination))?
        .map_err(|e| failed("ssh", "other", e.to_string()))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code();
    if code == Some(255) || code.is_none() {
        return Err(ssh_failed("ssh", &trouble(&stderr), destination));
    }
    if code == Some(127) || stderr.contains("command not found") {
        return Err(failed(
            "pair",
            "noObid",
            format!(
                "Oby isn't on {destination}, or `obid` isn't on the PATH its SSH gets: install Oby there, then try again."
            ),
        ));
    }
    if code != Some(0) {
        let said: String = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("it gave no reason")
            .chars()
            .filter(|c| !c.is_control())
            .take(300)
            .collect();
        return Err(failed(
            "pair",
            "refusedThere",
            format!("Oby on {destination} didn't pair: {said}"),
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|_| {
        failed(
            "pair",
            "refusedThere",
            format!("Oby on {destination} answered something this Mac can't read."),
        )
    })
}
