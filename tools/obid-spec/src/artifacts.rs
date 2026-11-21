//! obid's artifact store, proven from outside (docs/contracts/artifacts.md, ARTIFACTS-1 to 8): the
//! lab's artifact workflows (`workflows/lab/artifacts/`, LAB-ARTIFACTS-1 to 6 and 23) pointed at
//! the real obid. An agent publishes from its conversation's room, so the lab's fixtures (its own
//! Claude Design project, both canvas shapes, sparse files of any size) are written there, and the
//! conversation's `oby_artifact` publishes them through the stand-in harness. What obid then serves
//! is read back through its API and set beside what was written. Export and import (ARTIFACTS-9 to
//! 11) take the lab's import, export and round trip to the real obid: the import's bytes signed by
//! a paired device (WORDS-21), the export's zip read entry by entry. Its nodes are registered in
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

/// The lab's fixture, written into the conversation's room; with no `conversation`, into the run's
/// scratch folder (a project zip to import needs no room).
fn fixture(run: &Run, input: &Value) -> Result<Value> {
    if input.get("conversation").is_none() {
        return crate::lab::artifacts::fixture(run, input);
    }
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

// ─── Export and import (ARTIFACTS-9 to 11, WORDS-21): the lab's import, export and round trip
// (workflows/lab/artifacts/design.json, signed.json) pointed at the real obid. ───────────────────

/// What obid answered, its body as bytes (a zip is not text).
struct Raw {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Raw {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

/// One request to the scratch obid with its token, `body` as bytes; chunks joined.
async fn raw(
    obid: &Value,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<Raw> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let address = crate::http::loopback(str_of(obid, "base")?)?;
    let mut conn = tokio::net::TcpStream::connect(&address).await?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        str_of(obid, "token")?,
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    conn.write_all(head.as_bytes()).await?;
    conn.write_all(body).await?;
    let mut answer = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(120),
        conn.read_to_end(&mut answer),
    )
    .await??;
    let split = answer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("an answer with no head")?;
    let head = String::from_utf8_lossy(&answer[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .context("an answer with no status")?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let mut body = answer[split + 4..].to_vec();
    if headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.contains("chunked"))
    {
        let mut out = Vec::new();
        let mut rest = body.as_slice();
        loop {
            let line = rest
                .windows(2)
                .position(|w| w == b"\r\n")
                .context("a chunk")?;
            let size = usize::from_str_radix(std::str::from_utf8(&rest[..line])?.trim(), 16)?;
            if size == 0 {
                break;
            }
            out.extend_from_slice(&rest[line + 2..line + 2 + size]);
            rest = &rest[line + 4 + size..];
        }
        body = out;
    }
    Ok(Raw {
        status,
        headers,
        body,
    })
}

/// `X-Oby-Words` over `body`'s exact bytes, signed by `device` (a device-pair step's output).
fn words_over(device: &Value, method: &str, path: &str, body: &[u8]) -> Result<String> {
    use sha2::Digest as _;
    let id = str_of(device, "id")?;
    let key = crate::keys::DeviceKey::from_hex(str_of(device, "key")?)?;
    let at = crate::keys::now_millis();
    let digest = hex::encode(sha2::Sha256::digest(body));
    let signature = key.sign(&format!("oby-words\n{method}\n{path}\n{digest}\n{at}"));
    Ok(format!("{id} {at} {signature}"))
}

const IMPORT: &str = "/v1/artifacts/import";

/// `bytes` imported as `content_type`: `as` device (signed over them, the default), unsigned, or
/// other-bytes (signed over bytes that aren't these).
async fn import_bytes(input: &Value, bytes: &[u8], content_type: &str) -> Result<Raw> {
    let mut headers = vec![("Content-Type".to_string(), content_type.to_string())];
    match input["as"].as_str().unwrap_or("device") {
        "device" => headers.push((
            "X-Oby-Words".into(),
            words_over(&input["device"], "POST", IMPORT, bytes)?,
        )),
        "other-bytes" => headers.push((
            "X-Oby-Words".into(),
            words_over(&input["device"], "POST", IMPORT, b"other bytes")?,
        )),
        "unsigned" => {}
        other => anyhow::bail!("as: {other}? device, unsigned or other-bytes"),
    }
    raw(&input["obid"], "POST", IMPORT, &headers, bytes).await
}

/// `artifacts-import`: the bytes of `file` (`contentType`, default application/zip).
async fn import(input: &Value) -> Result<Value> {
    let file = str_of(input, "file")?;
    let bytes = std::fs::read(file).with_context(|| format!("can't read {file}"))?;
    let r = import_bytes(
        input,
        &bytes,
        input["contentType"].as_str().unwrap_or("application/zip"),
    )
    .await?;
    Ok(json!({"status": r.status, "body": r.json()}))
}

/// A zip's entries: their names, bytes and hashes, as they are (read with the lab's own reader, so
/// a link or a path out in an export would fail the step).
fn zip_entries(bytes: &[u8]) -> Result<Vec<Value>> {
    use sha2::Digest as _;
    let files = crate::lab::artifacts::unzip(bytes).map_err(|e| anyhow::anyhow!(e.message))?;
    Ok(files
        .iter()
        .map(|(name, b)| json!({"name": name, "bytes": b.len(), "sha256": hex::encode(sha2::Sha256::digest(b))}))
        .collect())
}

async fn export_of(obid: &Value, id: &str, version: u64, what: &str) -> Result<Raw> {
    raw(
        obid,
        "GET",
        &format!("/v1/artifacts/{id}/export/{version}?as={what}"),
        &[],
        b"",
    )
    .await
}

/// `artifacts-export`: one export, saved in the run's scratch folder and read back.
async fn export(run: &Run, input: &Value) -> Result<Value> {
    use sha2::Digest as _;
    let id = str_of(input, "id")?;
    let version = input["version"].as_u64().context("version")?;
    let what = input["as"].as_str().unwrap_or("zip");
    let r = export_of(&input["obid"], id, version, what).await?;
    if r.status != 200 {
        return Ok(json!({"status": r.status, "body": r.json()}));
    }
    let saved = run
        .scratch
        .join("exports")
        .join(format!("{id}-{version}.{what}"));
    std::fs::create_dir_all(saved.parent().context("a folder")?)?;
    std::fs::write(&saved, &r.body)?;
    let mut out = json!({
        "status": r.status,
        "type": r.header("content-type"),
        "disposition": r.header("content-disposition"),
        "policy": r.header("content-security-policy"),
        "bytes": r.body.len(),
        "sha256": hex::encode(sha2::Sha256::digest(&r.body)),
        "saved": saved,
    });
    if what == "zip" {
        out["entries"] = json!(zip_entries(&r.body)?);
    } else {
        let html = String::from_utf8_lossy(&r.body);
        out["outside"] = json!(crate::lab::artifacts::outside_references(&html));
        let manifest = html
            .split("<script type=\"application/json\" id=\"oby-artifact-files\">")
            .nth(1)
            .and_then(|rest| rest.split("</script>").next())
            .and_then(|m| serde_json::from_str::<Value>(&m.replace("<\\/", "</")).ok());
        out["files"] = match manifest {
            Some(m) => json!(m["files"]
                .as_object()
                .map(|f| f.keys().cloned().collect::<Vec<_>>())),
            None => Value::Null,
        };
        out["inlined"] = json!(html.matches("data:").count());
    }
    Ok(out)
}

/// `artifacts-roundtrip`: imports `zip` (signed by `device`), exports it as a zip and sets the two
/// side by side, entry by entry; then imports the export and exports that again.
async fn roundtrip(input: &Value) -> Result<Value> {
    let file = str_of(input, "zip")?;
    let source = std::fs::read(file).with_context(|| format!("can't read {file}"))?;
    let obid = &input["obid"];
    let first = import_bytes(input, &source, "application/zip").await?;
    if first.status != 200 {
        return Ok(json!({"imported": Value::Null, "status": first.status, "body": first.json()}));
    }
    let imported = first.json();
    let id = str_of(&imported, "id")?.to_string();
    let out = export_of(obid, &id, 1, "zip").await?;
    anyhow::ensure!(out.status == 200, "the export answered {}", out.status);
    let src = crate::lab::artifacts::unzip(&source).map_err(|e| anyhow::anyhow!(e.message))?;
    let got = crate::lab::artifacts::unzip(&out.body).map_err(|e| anyhow::anyhow!(e.message))?;
    let only_src: Vec<&String> = src.keys().filter(|k| !got.contains_key(*k)).collect();
    let only_got: Vec<&String> = got.keys().filter(|k| !src.contains_key(*k)).collect();
    let changed: Vec<&String> = src
        .iter()
        .filter(|(k, b)| got.get(*k).is_some_and(|g| g != *b))
        .map(|(k, _)| k)
        .collect();
    let again = import_bytes(input, &out.body, "application/zip").await?;
    let stable = if again.status == 200 {
        let second = export_of(obid, str_of(&again.json(), "id")?, 1, "zip").await?;
        second.status == 200 && second.body == out.body
    } else {
        false
    };
    Ok(json!({
        "imported": imported,
        "entries": src.len(),
        "same": only_src.is_empty() && only_got.is_empty() && changed.is_empty() && !src.is_empty(),
        "onlyInSource": only_src,
        "onlyInExport": only_got,
        "changed": changed,
        "stable": stable,
    }))
}

/// `artifacts-zip`: a zip of the suite's own, written into the run's scratch folder. Ones an import
/// must refuse whole: `kind` link (an entry that is a link), dotdot (`../escape.html`), absolute
/// (`/etc/escape.html`), oversize (an SVG one byte over 2 MB), many (513 files). One it takes:
/// export-file (a page with a file named `export`).
fn zip_fixture(run: &Run, input: &Value) -> Result<Value> {
    use std::io::Write as _;
    let kind = str_of(input, "kind")?;
    let mut entries: Vec<(String, Vec<u8>, bool)> =
        vec![("index.html".into(), b"<p>page</p>".to_vec(), false)];
    match kind {
        "link" => entries.push(("away".into(), b"/etc/hosts".to_vec(), true)),
        "dotdot" => entries.push(("../escape.html".into(), b"x".to_vec(), false)),
        "absolute" => entries.push(("/etc/escape.html".into(), b"x".to_vec(), false)),
        "oversize" => entries.push(("big.svg".into(), vec![b' '; 2 * 1024 * 1024 + 1], false)),
        "many" => entries.extend((0..513).map(|i| (format!("f{i:04}.txt"), b"x".to_vec(), false))),
        "export-file" => entries.push(("export".into(), b"plain".to_vec(), false)),
        other => {
            anyhow::bail!("kind {other}? link, dotdot, absolute, oversize, many or export-file")
        }
    }
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes, link) in &entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o644);
        if *link {
            w.add_symlink(name.as_str(), String::from_utf8_lossy(bytes), options)?;
        } else {
            w.start_file(name.as_str(), options)?;
            w.write_all(bytes)?;
        }
    }
    let bytes = w.finish()?.into_inner();
    let path = run.scratch.join("zips").join(format!("{kind}.zip"));
    std::fs::create_dir_all(path.parent().context("a folder")?)?;
    std::fs::write(&path, bytes)?;
    Ok(json!({"zip": path}))
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "artifacts-fixture" => fixture(run, input),
        "artifacts-same" => same(input).await,
        "artifacts-import" => import(input).await,
        "artifacts-export" => export(run, input).await,
        "artifacts-roundtrip" => roundtrip(input).await,
        "artifacts-zip" => zip_fixture(run, input),
        other => anyhow::bail!("no node {other}"),
    }
}
