//! The lab's machines (src/lab/mod.rs): a Linux computer of its own per workflow, made with
//! OrbStack and deleted at the workflow's end whatever it ended in. It stands in for "another
//! computer" (a Linux box, a server): isolated by default, so it shares no folder with this Mac,
//! and its user is `oby`, never this Mac's.
//!
//! A machine's name is `obid-lab-<workflow>-<name>`, `<workflow>` from this workflow's scratch
//! folder, so two workflows (or two runs) never share one, and only a name this workflow made is
//! ever deleted. Without OrbStack (not installed, not running) a step answers `skipped`.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::obid::Run;

pub const NODES: &[(&str, &str)] = &[
    ("lab.machine.up", "Makes a Linux machine of this workflow's own with OrbStack (ubuntu:noble by default; isolated, user oby), waits until it answers, and remembers it for the cleanup. Skipped without OrbStack."),
    ("lab.machine.run", "Runs a bash command on a machine this workflow made (as oby, or root), answering its exit status, stdout and stderr; with expect."),
    ("lab.machine.down", "Deletes a machine this workflow made, now rather than at the cleanup."),
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
        other => bail!("{other} isn't a machine node"),
    }
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
    Ok(json!({ "machine": machine, "distro": distro, "arch": arch, "user": USER }))
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

/// Deletes every machine this workflow made and hasn't deleted yet.
pub async fn cleanup() -> Value {
    let made: Vec<String> = MADE.lock().unwrap().clone();
    let mut deleted = vec![];
    for m in made {
        match delete(&m).await {
            Ok(()) => deleted.push(json!(m)),
            Err(e) => deleted.push(json!({ "machine": m, "error": format!("{e:#}") })),
        }
    }
    json!(deleted)
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
