//! obid's artifact store, proven from outside (docs/contracts/artifacts.md, ARTIFACTS-1 to 8): the
//! lab's artifact workflows (`workflows/lab/artifacts/`, LAB-ARTIFACTS-1 to 6 and 23) pointed at
//! the real obid. An agent publishes from its conversation's room, so the lab's fixtures (its own
//! Claude Design project, both canvas shapes, sparse files of any size) are written there, and the
//! conversation's `oby_artifact` publishes them through the stand-in harness. What obid then serves
//! is read back through its API and set beside what was written. Its nodes are registered in
//! src/nodes.rs, `artifacts-*`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::obid::Run;
use crate::sphere::{obid_call, str_of};

/// `<home>/conversations/<conversation>/room/<into>`, `into` a plain path inside the room.
fn in_room(input: &Value, key: &str) -> Result<(PathBuf, String)> {
    let home = str_of(&input["obid"], "home")?;
    let conversation = str_of(input, "conversation")?;
    let into = str_of(input, key)?;
    anyhow::ensure!(
        uuid::Uuid::try_parse(conversation).is_ok(),
        "conversation {conversation:?} isn't one of obid's ids"
    );
    anyhow::ensure!(
        !into.is_empty()
            && !into.starts_with('/')
            && !into
                .split('/')
                .any(|p| p.is_empty() || p == ".." || p == "."),
        "{key} {into:?} isn't a path inside the room"
    );
    let room = Path::new(home)
        .join("conversations")
        .join(conversation)
        .join("room");
    Ok((room.join(into), into.to_string()))
}

/// The lab's fixture, written into the conversation's room.
fn fixture(run: &Run, input: &Value) -> Result<Value> {
    let (dir, into) = in_room(input, "into")?;
    anyhow::ensure!(!dir.exists(), "{into} is already in the room");
    std::fs::create_dir_all(&dir)?;
    let mut lab = input.clone();
    lab["into"] = json!(dir);
    let mut out = crate::lab::artifacts::fixture(run, &lab)?;
    // `drop`: files of it taken out again (a design's later version without one of its uploads).
    for file in input["drop"].as_array().into_iter().flatten() {
        let file = file.as_str().context("drop[]")?;
        anyhow::ensure!(!file.split('/').any(|p| p == ".."), "drop {file:?}");
        std::fs::remove_file(dir.join(file)).with_context(|| format!("drop {file}"))?;
    }
    out["path"] = json!(into);
    Ok(out)
}

/// Every file under `dir`, relative to it, sorted; a link is listed as it is, never followed.
fn files_under(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).with_context(|| folder.display().to_string())? {
            let path = entry?.path();
            if std::fs::symlink_metadata(&path)?.is_dir() {
                folders.push(path);
            } else {
                let rel = path.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Each file of the room's `from` (a design's `_blob/<id>` uploads too, through the version's
/// alias) read back from obid as `{artifact}/{version}/<file>`: its status, length and text set
/// beside what was written, with the type and the policy each was served with.
async fn same(input: &Value) -> Result<Value> {
    let (dir, _) = in_room(input, "from")?;
    let artifact = str_of(input, "artifact")?;
    let version = input["version"].as_u64().context("version")?;
    let mut differ = Vec::new();
    let mut types = serde_json::Map::new();
    let mut policies = std::collections::BTreeSet::new();
    let files = files_under(&dir)?;
    for file in &files {
        let written = std::fs::read(dir.join(file))?;
        let path = format!("/v1/artifacts/{artifact}/{version}/{file}");
        let answer = obid_call(&input["obid"], "GET", &path, None).await?;
        let header = |name: &str| {
            answer
                .headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        let length = header("content-length").and_then(|v| v.parse::<usize>().ok());
        types.insert(file.clone(), json!(header("content-type")));
        policies.insert(header("content-security-policy").unwrap_or_default());
        if answer.status != 200 {
            differ.push(format!("{file}: {}", answer.status));
        } else if length != Some(written.len()) || answer.text != String::from_utf8_lossy(&written)
        {
            differ.push(format!(
                "{file}: {:?} bytes served, {} written",
                length,
                written.len()
            ));
        }
    }
    Ok(
        json!({"same": differ.is_empty() && !files.is_empty(), "compared": files.len(),
               "differ": differ, "types": types, "policies": policies}),
    )
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "artifacts-fixture" => fixture(run, input),
        "artifacts-same" => same(input).await,
        other => anyhow::bail!("no node {other}"),
    }
}
