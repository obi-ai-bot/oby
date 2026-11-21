//! Export and import (docs/contracts/artifacts.md, ARTIFACTS-9 to 11): a version out as a zip or as
//! one standalone HTML file, and a Claude Design project zip, a page or a document in as an
//! artifact of the owner's. The zip is deterministic (names sorted, one fixed time, mode 0644,
//! deflate), and a design goes out under `project/` at Claude Design's own paths, each upload back
//! at `project/_blob/<id>`, every byte as kept: so the export of an import is the import.
//!
//! An import is read from the zip in memory, checked entry by entry ([`unzip`]: no link, no path
//! out, nothing over Claude's limits), written into a scratch folder of obid's own and published
//! from there like an agent's folder, so every rule of the store holds for it as it is (the
//! kinds, a design's checks, its uploads, the limits, the secret scan). It owns no state of its
//! own: `origin` on artifact.json is the store's.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::blobs::BLOB_PREFIX;
use super::{Artifacts, Kind, Marks, POLICY, Publish, clean_title, limits};
use crate::api::{ApiError, Shared};
use crate::store::{make_private_dir, write_private};

/// A version's files, by their path inside it.
pub type Files = BTreeMap<String, Vec<u8>>;

/// The folder Claude Design's project zip holds its project in.
const PROJECT: &str = "project/";
/// The title an import gets when its `canvas.json` names none.
const IMPORTED: &str = "Imported";

/// The mark an import's version keeps (ARTIFACTS-25): its files came from outside.
const FROM_OUTSIDE: &str = "an imported artifact's files, which may hold anyone's words";

/// How an artifact came to be when no agent made it (add-only, left out for an agent's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// `import`.
    pub kind: String,
    /// What was imported: `claude-design-zip`, `zip`, `page` or `document`.
    pub from: String,
}

impl Origin {
    fn import(from: &str) -> Origin {
        Origin {
            kind: "import".into(),
            from: from.into(),
        }
    }
}

/// A version as its export's entries: a `design` under `project/` at Claude Design's paths, each
/// upload back at `project/_blob/<id>`; any other kind at the zip's root. Every byte as kept.
pub fn entries(kind: Kind, files: &Files, blobs: &BTreeMap<String, String>) -> Files {
    let back: BTreeMap<&str, &str> = blobs
        .iter()
        .map(|(id, kept)| (kept.as_str(), id.as_str()))
        .collect();
    let prefix = if kind == Kind::Design { PROJECT } else { "" };
    files
        .iter()
        .map(|(path, bytes)| {
            let name = match back.get(path.as_str()) {
                Some(id) => format!("{prefix}{BLOB_PREFIX}{id}"),
                None => format!("{prefix}{path}"),
            };
            (name, bytes.clone())
        })
        .collect()
}

/// A zip of `entries`, the same bytes every time: names sorted, one fixed time (1980-01-01),
/// mode 0644, deflate.
pub fn zip_of(entries: &Files) -> Result<Vec<u8>, String> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644)
        .large_file(false);
    for (name, bytes) in entries {
        w.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        w.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(w.finish().map_err(|e| e.to_string())?.into_inner())
}

/// Why an entry's name isn't a plain path inside the zip: `/` at its start, `..`, `.`, an empty
/// part, `\`, `:` or NUL. None when it is one.
fn not_plain(name: &str) -> bool {
    name.starts_with('/')
        || name.contains(['\\', '\0', ':'])
        || name
            .split('/')
            .any(|p| p.is_empty() || p == ".." || p == ".")
}

/// A zip's files, read in memory: every entry a plain path, never a link, each read only as far as
/// its limit allows (whatever size its header claims) and the whole no more than a version may be.
/// Hidden names and `__MACOSX/` are left out, as a folder's hidden names are. `Err` says which
/// entry and why, starting `Not published: `.
pub fn unzip(bytes: &[u8]) -> Result<Files, String> {
    let bad = |why: String| format!("Not published: {why}");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| bad(format!("that isn't a zip ({e}).")))?;
    if archive.len() > limits::MAX_ENTRIES {
        return Err(bad(format!(
            "the zip holds more than {} names, so obid didn't read it.",
            limits::MAX_ENTRIES
        )));
    }
    let mut out = Files::new();
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| bad(format!("the zip can't be read ({e}).")))?;
        let name = entry.name().to_string();
        // A link, whatever it points at, before anything else about it.
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(bad(format!(
                "{name:?} is a link, which obid never follows."
            )));
        }
        let trimmed = name.strip_suffix('/').unwrap_or(&name);
        if not_plain(trimmed) {
            return Err(bad(format!("{name:?} isn't a plain path inside the zip.")));
        }
        if entry.is_dir() {
            continue;
        }
        if trimmed
            .split('/')
            .any(|p| p.starts_with('.') || p == "__MACOSX")
        {
            continue;
        }
        let (most, _) = limits::most_for(trimmed);
        if let Some(why) = limits::file_too_big(trimmed, entry.size()) {
            return Err(bad(why));
        }
        // Never more than its limit, whatever its header said.
        let mut data = Vec::new();
        (&mut entry)
            .take(most + 1)
            .read_to_end(&mut data)
            .map_err(|e| bad(format!("{name:?} can't be read ({e}).")))?;
        if let Some(why) = limits::file_too_big(trimmed, data.len() as u64) {
            return Err(bad(why));
        }
        total += data.len() as u64;
        if total > limits::MAX_BYTES {
            return Err(bad(limits::version_too_big()));
        }
        out.insert(trimmed.to_string(), data);
        if let Some(why) = limits::too_many(out.len()) {
            return Err(bad(why));
        }
    }
    Ok(out)
}

/// What a zip holds: Claude Design's project zip (`project/canvas.json`: its `project/` folder is
/// the version), a zip of one folder (that folder), or files at its root.
pub fn project_of(files: Files) -> (Files, &'static str) {
    if files.contains_key("project/canvas.json") {
        let inside = files
            .into_iter()
            .filter_map(|(p, b)| p.strip_prefix(PROJECT).map(|p| (p.to_string(), b)))
            .collect();
        return (inside, "claude-design-zip");
    }
    let tops: BTreeSet<&str> = files
        .keys()
        .map(|p| p.split('/').next().unwrap_or(""))
        .collect();
    if tops.len() == 1 && files.keys().all(|p| p.contains('/')) {
        let top = format!("{}/", tops.iter().next().expect("one"));
        let inside = files
            .into_iter()
            .filter_map(|(p, b)| p.strip_prefix(&top).map(|p| (p.to_string(), b)))
            .collect();
        return (inside, "zip");
    }
    (files, "zip")
}

fn base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn data_url(path: &str, bytes: &[u8]) -> String {
    let mime = super::type_of(Path::new(path))
        .split(';')
        .next()
        .unwrap_or("");
    format!("data:{mime};base64,{}", base64(bytes))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The path a reference in `from` names inside the version, when it is one of its files.
fn local(from: &str, reference: &str, files: &Files) -> Option<String> {
    if reference.contains("://") || reference.starts_with("data:") || reference.starts_with('#') {
        return None;
    }
    let reference = reference.split(['?', '#']).next().unwrap_or("");
    let mut parts: Vec<&str> = if reference.starts_with('/') {
        vec![]
    } else {
        from.rsplit_once('/')
            .map_or(vec![], |(dir, _)| dir.split('/').collect())
    };
    for p in reference.trim_start_matches('/').split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    let path = parts.join("/");
    files.contains_key(&path).then_some(path)
}

/// `src="…"`, `href="…"` and `url(…)` naming a file of the version, each as a `data:` URL (a
/// stylesheet's own `url(…)`s first, four deep at most).
fn inline(from: &str, text: &str, files: &Files, depth: usize) -> String {
    static REF: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = REF.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)(\b(?:src|href)\s*=\s*")([^"]*)(")|(url\(\s*['"]?)([^'")]*)(['"]?\s*\))"#,
        )
        .expect("it compiles")
    });
    re.replace_all(text, |cap: &regex::Captures| {
        let (open, reference, close) = match cap.get(2) {
            Some(r) => (&cap[1], r.as_str(), &cap[3]),
            None => (&cap[4], &cap[5], &cap[6]),
        };
        match local(from, reference, files) {
            Some(path) => {
                let bytes = &files[&path];
                let url = if super::extension(&path) == "css" && depth < 4 {
                    let css = inline(&path, &String::from_utf8_lossy(bytes), files, depth + 1);
                    data_url(&path, css.as_bytes())
                } else {
                    data_url(&path, bytes)
                };
                format!("{open}{url}{close}")
            }
            None => cap[0].to_string(),
        }
    })
    .into_owned()
}

/// One standalone HTML file of a version: a page with its styles, scripts and pictures inlined as
/// `data:` URLs; a document, a picture or a PDF wrapped in one; a canvas or a design as every file,
/// each a `data:` URL in `<script type="application/json" id="oby-artifact-files">`, with its
/// boards listed by name.
pub fn html(kind: Kind, title: &str, files: &Files, blobs: &BTreeMap<String, String>) -> String {
    let head = |body: &str| {
        format!(
            "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title></head><body>\n{body}\n</body></html>\n",
            escape(title)
        )
    };
    let first = files.iter().next();
    match kind {
        Kind::Html => match files.get("index.html") {
            Some(index) => inline("index.html", &String::from_utf8_lossy(index), files, 0),
            None => head(""),
        },
        Kind::Markdown => {
            let text = first
                .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            head(&format!("<pre>{}</pre>", escape(&text)))
        }
        Kind::Image => match first {
            Some((p, b)) => head(&format!("<img alt=\"\" src=\"{}\">", data_url(p, b))),
            None => head(""),
        },
        Kind::Pdf => match first {
            Some((p, b)) => head(&format!(
                "<embed type=\"application/pdf\" style=\"width:100%;height:100vh\" src=\"{}\">",
                data_url(p, b)
            )),
            None => head(""),
        },
        Kind::Canvas | Kind::Design => {
            let all: BTreeMap<&str, String> = files
                .iter()
                .map(|(p, b)| (p.as_str(), data_url(p, b)))
                .collect();
            let canvas = files
                .get("canvas.json")
                .and_then(|t| serde_json::from_slice::<Value>(t).ok());
            let boards: Vec<String> = match canvas.as_ref().map(|c| &c["boards"]) {
                Some(Value::Object(b)) => b
                    .iter()
                    .map(|(k, v)| v["title"].as_str().unwrap_or(k).to_string())
                    .collect(),
                _ => match canvas.as_ref() {
                    Some(Value::Array(frames)) => frames
                        .iter()
                        .filter_map(|f| f["title"].as_str().map(str::to_owned))
                        .collect(),
                    _ => vec![],
                },
            };
            let manifest = json!({"kind": kind, "files": all, "blobs": blobs}).to_string();
            // `</` can't end the script early: JSON allows `<\/`.
            let manifest = manifest.replace("</", "<\\/");
            let list: String = boards
                .iter()
                .map(|b| format!("<li>{}</li>", escape(b)))
                .collect();
            head(&format!(
                "<script type=\"application/json\" id=\"oby-artifact-files\">{manifest}</script>\n<ol>{list}</ol>"
            ))
        }
    }
}

/// The name an export is saved as: the title's letters, digits and `-`, then `-v<version>`.
fn file_name(title: &str, version: u32, extension: &str) -> String {
    let name: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{name}-v{version}.{extension}")
}

/// What an import is, by its type: a zip (or anything that starts like one), a page or a document.
enum Import {
    Zip,
    Page,
    Document,
}

impl Import {
    fn of(content_type: &str, body: &[u8]) -> Option<Import> {
        let kind = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match kind.as_str() {
            "text/html" => Some(Import::Page),
            "text/markdown" => Some(Import::Document),
            "application/zip" | "application/x-zip-compressed" => Some(Import::Zip),
            _ if body.starts_with(b"PK\x03\x04") => Some(Import::Zip),
            _ => None,
        }
    }
}

impl Artifacts {
    /// Version `version` of `id` read whole: its kind, title, the version's uploads and its files.
    fn read_version(
        &self,
        id: &str,
        version: u32,
    ) -> Option<(Kind, String, BTreeMap<String, String>, Files)> {
        let artifact = self.get(id)?;
        let kept = artifact.versions.iter().find(|v| v.version == version)?;
        let folder = self.version_path(id, version);
        let mut files = Files::new();
        for name in &kept.files {
            let path = super::inside(&folder, name)?;
            files.insert(name.clone(), std::fs::read(path).ok()?);
        }
        Some((artifact.kind, artifact.title, kept.blobs.clone(), files))
    }

    /// An import (ARTIFACTS-11): `body` as `content_type` says it is, kept as a new artifact of the
    /// owner's with `origin: {kind: import, from}`, titled from its `canvas.json`. `Err` is one line
    /// starting `Not published: `, and `true` when the zip itself is what was refused.
    pub async fn import(
        &self,
        content_type: &str,
        body: &[u8],
    ) -> Result<(String, u32), (bool, String)> {
        let what = Import::of(content_type, body).ok_or((
            true,
            "Not published: an import is a zip, a page (text/html) or a document (text/markdown)."
                .to_owned(),
        ))?;
        let (files, from, name) = match what {
            Import::Page => (
                Files::from([("page.html".to_owned(), body.to_vec())]),
                "page",
                "page.html",
            ),
            Import::Document => (
                Files::from([("document.md".to_owned(), body.to_vec())]),
                "document",
                "document.md",
            ),
            Import::Zip => {
                let (files, from) = project_of(unzip(body).map_err(|why| (true, why))?);
                (files, from, "import")
            }
        };
        let title = files
            .get("canvas.json")
            .and_then(|t| serde_json::from_slice::<Value>(t).ok())
            .and_then(|c| c["title"].as_str().and_then(clean_title))
            .unwrap_or_else(|| IMPORTED.to_owned());
        // Written where nothing else is, then published from there like an agent's folder.
        let scratch = self
            .scratch
            .join(format!("import-{}", uuid::Uuid::new_v4().simple()));
        let written = (|| {
            make_private_dir(&self.scratch)?;
            make_private_dir(&scratch)?;
            let root = if matches!(what, Import::Zip) {
                let root = scratch.join(name);
                make_private_dir(&root)?;
                root
            } else {
                scratch.clone()
            };
            for (path, bytes) in &files {
                let to = root.join(path);
                if let Some(parent) = to.parent() {
                    let mut at = root.clone();
                    for part in parent.strip_prefix(&root).unwrap_or(Path::new("")) {
                        at.push(part);
                        make_private_dir(&at)?;
                    }
                }
                write_private(&to, bytes)?;
            }
            Ok::<(), std::io::Error>(())
        })();
        let result = match written {
            Err(e) => Err((
                false,
                format!("Not published: obid couldn't read the import ({e})."),
            )),
            Ok(()) if files.is_empty() => {
                Err((true, "Not published: the zip holds no file.".into()))
            }
            Ok(()) => self
                .publish_from(
                    Publish {
                        root: &scratch,
                        path: name,
                        title: &title,
                        kind: None,
                        conversation: None,
                        job: None,
                        artifact: None,
                        kept: None,
                        marks: Marks {
                            foreign: Some(FROM_OUTSIDE.into()),
                            ..Marks::default()
                        },
                    },
                    Some(Origin::import(from)),
                )
                .await
                .map_err(|why| (false, why)),
        };
        let _ = std::fs::remove_dir_all(&scratch);
        result
    }
}

#[derive(Deserialize)]
pub struct As {
    #[serde(rename = "as")]
    what: Option<String>,
}

/// `GET /v1/artifacts/{id}/export/{version}?as=zip|html` (ARTIFACTS-9, 10).
pub async fn export(
    State(shared): State<Arc<Shared>>,
    UrlPath((id, version)): UrlPath<(String, String)>,
    Query(As { what }): Query<As>,
) -> Result<Response, ApiError> {
    let no_version = || ApiError::new(StatusCode::NOT_FOUND, "There's no such version.");
    if shared.artifacts.get(&id).is_none() {
        return Err(super::no_artifact());
    }
    let n: u32 = version.parse().map_err(|_| no_version())?;
    let (kind, title, blobs, files) = shared
        .artifacts
        .read_version(&id, n)
        .ok_or_else(no_version)?;
    let (bytes, content_type, extension) = match what.as_deref() {
        Some("zip") => (
            zip_of(&entries(kind, &files, &blobs)).map_err(|e| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("The zip couldn't be made ({e})."),
                )
            })?,
            "application/zip",
            "zip",
        ),
        Some("html") => (
            html(kind, &title, &files, &blobs).into_bytes(),
            "text/html; charset=utf-8",
            "html",
        ),
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "An export is ?as=zip or ?as=html.",
            ));
        }
    };
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        file_name(&title, n, extension)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    Ok(response)
}

/// The most an import's body may be: a version's most, and room for the zip's own records.
pub fn import_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(limits::MAX_BYTES as usize + (1 << 20))
}

/// `POST /v1/artifacts/import` (ARTIFACTS-11, WORDS-21): the bytes of a Claude Design project zip,
/// a zip, a page or a document, signed by one of the owner's devices over those exact bytes.
pub async fn import(
    State(shared): State<Arc<Shared>>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if !crate::routes::typed_by_owner(&shared, "POST", &uri, &headers, &body) {
        return Err(crate::routes::needs_the_owners_app());
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match shared.artifacts.import(content_type, &body).await {
        Ok((id, _)) => {
            let artifact = shared.artifacts.get(&id).ok_or_else(super::no_artifact)?;
            Ok(axum::Json(artifact).into_response())
        }
        Err((true, why)) => Err(ApiError::new(StatusCode::BAD_REQUEST, why).because("bad_import")),
        Err((false, why)) => Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zip written entry by entry, those named in `links` links to what their bytes say.
    fn zip_with(entries: &[(&str, &[u8])], links: &[&str]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(0o644);
            if links.contains(name) {
                let target = String::from_utf8_lossy(bytes).into_owned();
                w.add_symlink(*name, target, options).unwrap();
            } else {
                w.start_file(*name, options).unwrap();
                w.write_all(bytes).unwrap();
            }
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn a_zip_is_the_same_bytes_every_time_and_reads_back() {
        let files = Files::from([
            ("b.txt".to_owned(), b"two".to_vec()),
            ("a/c.html".to_owned(), b"<p>one</p>".to_vec()),
        ]);
        let one = zip_of(&files).unwrap();
        assert_eq!(one, zip_of(&files).unwrap());
        assert_eq!(unzip(&one).unwrap(), files);
    }

    /// A link in the zip is refused, whatever it points at.
    #[test]
    fn artifacts_11_a_zip_with_a_link_is_refused() {
        let zip = zip_with(
            &[("index.html", b"<p>"), ("away", b"/etc/hosts")],
            &["away"],
        );
        let why = unzip(&zip).unwrap_err();
        assert_eq!(
            why,
            "Not published: \"away\" is a link, which obid never follows."
        );
    }

    /// `..` anywhere, an absolute path, `\` and `.` are refused, naming the entry.
    #[test]
    fn artifacts_11_a_zip_with_a_path_out_is_refused() {
        for name in [
            "../escape.html",
            "project/../../escape.html",
            "/etc/escape.html",
            "a\\..\\escape.html",
            "./index.html",
            "a//b.html",
        ] {
            let zip = zip_with(&[("index.html", b"<p>"), (name, b"x")], &[]);
            let why = unzip(&zip).unwrap_err();
            assert!(
                why.contains("isn't a plain path inside the zip"),
                "{name}: {why}"
            );
        }
    }

    /// An entry over its kind's limit is refused, whatever its header claims, and read no further
    /// than its limit; a zip of too many names is refused before any is read.
    #[test]
    fn artifacts_11_a_zip_over_the_limits_is_refused() {
        let svg = vec![b' '; 2 * 1024 * 1024 + 1];
        let why = unzip(&zip_with(&[("big.svg", &svg)], &[])).unwrap_err();
        assert_eq!(
            why,
            "Not published: big.svg is 2 MB and 1 byte, and an SVG may be 2 MB at most."
        );
        let many: Vec<(String, Vec<u8>)> = (0..=limits::MAX_FILES)
            .map(|i| (format!("f{i:04}.txt"), b"x".to_vec()))
            .collect();
        let refs: Vec<(&str, &[u8])> = many
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        let why = unzip(&zip_with(&refs, &[])).unwrap_err();
        assert_eq!(
            why,
            "Not published: a version holds at most 512 files, and this holds 513."
        );
        let names: Vec<(String, Vec<u8>)> = (0..=limits::MAX_ENTRIES)
            .map(|i| (format!(".h{i:05}"), Vec::new()))
            .collect();
        let refs: Vec<(&str, &[u8])> = names
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        assert!(
            unzip(&zip_with(&refs, &[]))
                .unwrap_err()
                .contains("more than 2000 names")
        );
    }

    /// A deflated entry whose header understates it is read only to its limit, then refused.
    #[test]
    fn artifacts_11_a_zip_bomb_stops_at_its_limit() {
        let mut bytes = zip_of(&Files::from([(
            "big.svg".to_owned(),
            vec![b'a'; 3 * 1024 * 1024],
        )]))
        .unwrap();
        // The local header's and the central directory's uncompressed size, both made 10 bytes.
        let real = (3u32 * 1024 * 1024).to_le_bytes();
        let small = 10u32.to_le_bytes();
        let mut i = 0;
        while let Some(at) = bytes[i..].windows(4).position(|w| w == real) {
            bytes[i + at..i + at + 4].copy_from_slice(&small);
            i += at + 4;
        }
        let why = unzip(&bytes).unwrap_err();
        assert!(
            why.contains("big.svg") || why.contains("can't be read"),
            "{why}"
        );
    }

    #[test]
    fn hidden_names_and_macosx_are_left_out_and_folders_unwrapped() {
        let zip = zip_with(
            &[
                ("site/index.html", b"<p>"),
                ("site/.DS_Store", b"x"),
                ("__MACOSX/site/._index.html", b"x"),
            ],
            &[],
        );
        let (files, from) = project_of(unzip(&zip).unwrap());
        assert_eq!(from, "zip");
        assert_eq!(files.keys().collect::<Vec<_>>(), ["index.html"]);
    }

    /// The standalone HTML of a page inlines its own files and leaves the web's alone.
    #[test]
    fn artifacts_10_a_page_inlines_its_own_files() {
        let files = Files::from([
            (
                "index.html".to_owned(),
                b"<link href=\"css/site.css\"><img src=\"logo.png\"><a href=\"#top\">".to_vec(),
            ),
            (
                "css/site.css".to_owned(),
                b"body{background:url('../logo.png')}".to_vec(),
            ),
            ("logo.png".to_owned(), b"\x89PNG".to_vec()),
        ]);
        let page = html(Kind::Html, "Page", &files, &BTreeMap::new());
        assert!(!page.contains("css/site.css"), "{page}");
        assert!(!page.contains("\"logo.png\""), "{page}");
        assert!(page.contains("href=\"#top\""));
        assert_eq!(page.matches("data:").count(), 2);
    }

    #[test]
    fn an_export_is_named_after_its_title() {
        assert_eq!(file_name("Lab design", 1, "zip"), "Lab-design-v1.zip");
    }
}
