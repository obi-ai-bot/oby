//! T3 Code's bugs as obid's map: each kind of worktree and git bug T3 Code fixed (a worktree
//! left behind or gone, two at once, branch names, dirty trees, lock files, a repository moved, a
//! base that moved, large and binary diffs, submodules, a restart mid-task) proven against the real
//! obid, as workflows under `workflows/t3/<kind>/` (the area `t3-<kind>`).
//!
//! A T3 workflow carries the `t3` tag and never `launch`; `obid-spec run` leaves it out unless
//! asked for (`--tag t3`, or `--only t3-<kind>`), and `coverage` doesn't count it
//! (`workflow::is_t3`): these are the map, not the launch's set.
//!
//! Its two nodes only act, never judge: they do what a crash, the owner's Finder or another
//! program does to obid's folders between two of obid's own steps. What obid then answers is
//! asserted with the suite's own nodes (`call`, `home-file`, `check`).
//!
//! - `t3.git`: git in a folder of a scratch obid's home (a job's worktree, obid's clone), as
//!   another program on the Mac would run it.
//! - `t3.fs`: removes, moves or writes a file or folder in a scratch obid's home or its person's
//!   home (a worktree deleted, a clone moved, a lock file a crash left, a binary file).

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The T3 nodes: their names and what they do.
pub const NODES: &[(&str, &str)] = &[
    ("t3.git", "Runs git in a folder of a scratch obid's home (`dir`: `worktrees/<job>`, `clones/<owner>/<repo>`), as another program would: {code, out, err}; fails on a non-zero exit unless `mayFail`."),
    ("t3.fs", "Does to a scratch obid's folders what a crash or the owner's Finder does: `op` remove (a file or folder), move (`path` to `to`) or write (`text`, `repeat` times, or `hex` bytes), in its home (`in`: home, the default) or its person's home: {op, path, existed}."),
];

pub async fn run_node(name: &str, input: &Value) -> Result<Value> {
    match name {
        "t3.git" => git(input),
        "t3.fs" => fs(input),
        other => bail!("{other} isn't a T3 node"),
    }
}

/// `rel` inside the obid's home (or its person's), never above it.
fn inside(input: &Value, rel: &str) -> Result<PathBuf> {
    let key = match input["in"].as_str().unwrap_or("home") {
        "home" => "home",
        "person" => "person",
        other => bail!("in: {other}? home or person"),
    };
    let base = input["obid"][key]
        .as_str()
        .with_context(|| format!("obid.{key}: an obid-start step's output"))?;
    anyhow::ensure!(
        !rel.is_empty() && !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
        "{rel:?} isn't a path inside that folder"
    );
    Ok(Path::new(base).join(rel))
}

fn git(input: &Value) -> Result<Value> {
    let dir = inside(input, input["dir"].as_str().context("dir")?)?;
    let args: Vec<String> = input["args"]
        .as_array()
        .context("args")?
        .iter()
        .map(|a| {
            a.as_str()
                .map(str::to_string)
                .context("an arg isn't a string")
        })
        .collect::<Result<_>>()?;
    let out = std::process::Command::new("/usr/bin/git")
        .arg("-C")
        .arg(&dir)
        .args(&args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", dir.parent().unwrap_or(&dir))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "obid-spec")
        .env("GIT_AUTHOR_EMAIL", "obid-spec@example.invalid")
        .env("GIT_COMMITTER_NAME", "obid-spec")
        .env("GIT_COMMITTER_EMAIL", "obid-spec@example.invalid")
        .output()
        .context("git didn't start")?;
    let code = out.status.code().unwrap_or(-1);
    let answer = json!({
        "code": code,
        "out": String::from_utf8_lossy(&out.stdout).trim(),
        "err": String::from_utf8_lossy(&out.stderr).trim(),
    });
    if code != 0 && input["mayFail"] != true {
        bail!("git {args:?} in {}: {answer}", dir.display());
    }
    Ok(answer)
}

fn fs(input: &Value) -> Result<Value> {
    let rel = input["path"].as_str().context("path")?;
    let path = inside(input, rel)?;
    let existed = path.symlink_metadata().is_ok();
    let op = input["op"].as_str().context("op: remove, move or write")?;
    match op {
        "remove" => {
            if path.is_dir() && !path.is_symlink() {
                std::fs::remove_dir_all(&path)?;
            } else if existed {
                std::fs::remove_file(&path)?;
            } else if input["mayBeGone"] != true {
                bail!("{rel} isn't there to remove");
            }
        }
        "move" => {
            let to = inside(input, input["to"].as_str().context("to")?)?;
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&path, &to).with_context(|| format!("moving {rel}"))?;
        }
        "write" => {
            let bytes = match (input["text"].as_str(), input["hex"].as_str()) {
                (Some(text), None) => text
                    .repeat(input["repeat"].as_u64().unwrap_or(1) as usize)
                    .into_bytes(),
                (None, Some(h)) => hex::decode(h).context("hex")?,
                _ => bail!("write takes text (with repeat) or hex"),
            };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, bytes)?;
        }
        other => bail!("op: {other}? remove, move or write"),
    }
    Ok(json!({"op": op, "path": rel, "existed": existed}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_t3_node_stays_inside_the_obids_folders() {
        let obid = json!({"home": "/tmp/h", "person": "/tmp/p"});
        for bad in ["", "/etc/passwd", "../x", "worktrees/../../x"] {
            assert!(inside(&json!({"obid": obid}), bad).is_err(), "{bad}");
        }
        assert!(inside(&json!({"obid": obid, "in": "elsewhere"}), "x").is_err());
        assert_eq!(
            inside(&json!({"obid": obid, "in": "person"}), "a/b").unwrap(),
            PathBuf::from("/tmp/p/a/b")
        );
    }
}
