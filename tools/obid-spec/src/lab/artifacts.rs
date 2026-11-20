//! The lab's `artifacts` feature (src/lab/mod.rs): Oby Artifacts' store v2, export and import, and
//! the turn's artifact, the first lab PR of the plan https://claude.ai/artifact/VrzfijvgHL6VD8jm5Hbs2k
//! (§3.1, §3.5, §3.8; order §7.1). It extends obid's store (`core/crates/obid/src/artifacts.rs`,
//! `docs/api/artifacts.md`) rather than replacing it, speaking the contract in
//! [`super::artifacts_contract`]. Its nodes are `lab.artifacts.<verb>`. Owned by the session
//! building Oby Artifacts; nothing else edits this file.
//!
//! Like browser.rs and browserapi.rs, the file has two halves, and only the first is Oby's to port:
//!
//! 1. **The core** (pure Rust over a version's files in memory, no IO and no clock of its own):
//!    the `design` kind, Claude Design's `canvas.json` and boards checked as they are and kept byte
//!    for byte ([`check_design`]); obid's `canvas` in its own shape and Swift's `{"frames": […]}`
//!    ([`check_canvas`]); Claude's uploads (`_blob/<id>`) kept under `assets/` with a map, a long
//!    id given a hashed name ([`ingest_blobs`]), and served back at `_blob/<id>` inside the version
//!    only ([`resolve`]); the media types fonts and video need ([`type_of`]); Claude's limits, each
//!    refusal naming what is over ([`check_sizes`]); a new version from a follow-up job of the same
//!    conversation ([`may_version`]); the words of a turn landing in the open artifact's
//!    conversation ([`land_words`]); the project zip out and in, deterministic, the export giving
//!    back what the import took ([`export_entries`], [`zip_of`], [`unzip`], [`project_of`]); one
//!    standalone HTML file ([`export_html`]); and the owner's signed words ([`Words`]).
//! 2. **The lab** (from [`Store`] down): the store on disk under the workflow's scratch folder, an
//!    HTTP/1.1 server on 127.0.0.1 with a free port speaking the contract's routes, fixtures (a
//!    Claude Design project of the lab's own, obid's and Swift's canvases, a page, files of a size),
//!    and the thin `lab.artifacts.*` nodes.
//!
//! Each rule can be switched off, one at a time, only to watch its workflow go red:
//! `OBID_SPEC_LAB_ARTIFACTS_BREAK=design,frames,…` ([`Rules::broken`]). obid keeps [`Rules::ALL`].

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::Digest as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::artifacts_contract::{self as c, Artifact, ArtifactEvent, Kind, Origin, Version};
use crate::obid::Run;

// ════════════════════════════════════════════════════════════════════════════════════════════
// 1. The core: what obid takes as it is.
// ════════════════════════════════════════════════════════════════════════════════════════════

/// The rules the store keeps. Each is on in obid; the lab switches one off only to prove its
/// workflow goes red without it (off, each behaves as obid does today).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Claude Design's `canvas.json` object is the `design` kind, checked as it is. Off: it is read
    /// as obid's `canvas` and refused, as today.
    pub design: bool,
    /// obid's `canvas` also reads Swift's `{"frames": […]}`. Off: refused, as today.
    pub frames: bool,
    /// `_blob/<id>` resolves to the upload the version keeps. Off: `404`.
    pub alias: bool,
    /// An upload whose id looks like a token gets a hashed name. Off: the import is refused.
    pub names: bool,
    /// Fonts, video and AVIF are served with their types. Off: `application/octet-stream`.
    pub types: bool,
    /// Claude's limits. Off: obid's of today (2,000 files and 50 MB a version, nothing per file).
    pub limits: bool,
    /// A follow-up job in the artifact's conversation versions it. Off: only its own job may.
    pub crossjob: bool,
    /// A rename sends `artifact {renamed: true}`. Off: silent, as today.
    pub renamed: bool,
    /// Words with `artifact` land in its conversation. Off: in the one the app had open.
    pub turn: bool,
    /// The owner's acts need a device's signed words.
    pub signature: bool,
    /// Export gives back the bytes it kept. Off: `canvas.json` comes out re-serialized.
    pub exact: bool,
}

impl Rules {
    pub const ALL: Rules = Rules {
        design: true,
        frames: true,
        alias: true,
        names: true,
        types: true,
        limits: true,
        crossjob: true,
        renamed: true,
        turn: true,
        signature: true,
        exact: true,
    };

    /// All rules but the ones named (comma-separated): the lab's red proofs.
    pub fn broken(names: &str) -> Rules {
        let mut r = Rules::ALL;
        for n in names.split(',').map(str::trim) {
            match n {
                "design" => r.design = false,
                "frames" => r.frames = false,
                "alias" => r.alias = false,
                "names" => r.names = false,
                "types" => r.types = false,
                "limits" => r.limits = false,
                "crossjob" => r.crossjob = false,
                "renamed" => r.renamed = false,
                "turn" => r.turn = false,
                "signature" => r.signature = false,
                "exact" => r.exact = false,
                _ => {}
            }
        }
        r
    }
}

/// A version's files: path inside the version → bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// A refusal: obid's status, a reason ([`c::REASONS`]) and one line a person can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub status: u16,
    pub reason: &'static str,
    pub message: String,
}

impl Refused {
    pub fn new(status: u16, reason: &'static str, message: impl Into<String>) -> Refused {
        debug_assert!(
            c::REASONS.contains(&reason),
            "{reason} isn't a reason the contract lists"
        );
        Refused {
            status,
            reason,
            message: message.into(),
        }
    }

    /// A publish or an import refused: `Not published: …`, as obid says it.
    pub fn not_published(reason: &'static str, why: impl std::fmt::Display) -> Refused {
        Refused::new(422, reason, format!("Not published: {why}"))
    }

    pub fn body(&self) -> Value {
        json!({"error": self.message, "reason": self.reason})
    }
}

/// What obid says instead of a name that looks like a secret (obid's `NAME_REFUSED`).
pub const NAME_REFUSED: &str =
    "Not published: a name in it looks like a secret, so obid doesn't say it.";

fn ext(name: &str) -> String {
    let last = name.rsplit('/').next().unwrap_or(name);
    match last.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() => e.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// A text file, for its limit: the page, its styles and scripts, data, documents, SVG.
pub fn is_text(path: &str) -> bool {
    matches!(
        ext(path).as_str(),
        "html"
            | "htm"
            | "css"
            | "js"
            | "mjs"
            | "json"
            | "map"
            | "md"
            | "markdown"
            | "txt"
            | "csv"
            | "svg"
            | "xml"
    )
}

/// Whether `name` holds a run of 32 or more of `[A-Za-z0-9_+-]` with letters and digits in it,
/// other than a UUID or a run of lower-case hex alone: obid's `long_run`, the shape of a token.
pub fn long_run(name: &str) -> bool {
    static RUN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let run = RUN.get_or_init(|| regex::Regex::new(r"[A-Za-z0-9_+\-]{32,}").expect("it compiles"));
    run.find_iter(name).any(|found| {
        let text = found.as_str();
        text.bytes().any(|b| b.is_ascii_alphabetic())
            && text.bytes().any(|b| b.is_ascii_digit())
            && !(text.len() == 36 && uuid::Uuid::try_parse(text).is_ok())
            && !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

fn size_text(bytes: u64) -> String {
    let (mb, rest) = (bytes / c::MB, bytes % c::MB);
    let unit = |n: u64| if n == 1 { "byte" } else { "bytes" };
    match (mb, rest) {
        (0, r) => format!("{r} {}", unit(r)),
        (m, 0) => format!("{m} MB"),
        (m, r) => format!("{m} MB and {r} {}", unit(r)),
    }
}

/// Claude's limits (the owner's decision of 2026-11-20), checked on the sizes alone, before any
/// byte is read: one refusal naming what is over, and by how much.
pub fn check_sizes(sizes: &[(String, u64)], rules: &Rules) -> Result<(), Refused> {
    let total: u64 = sizes.iter().map(|(_, b)| b).sum();
    if !rules.limits {
        // obid's of today: 2,000 files and 50 MB a version, nothing per file.
        if sizes.len() > 2000 {
            return Err(Refused::not_published(
                "too_many",
                "a version holds at most 2000 files.",
            ));
        }
        if total > 50 * c::MB {
            return Err(Refused::not_published(
                "too_big",
                "a version may be 50 MB at most.",
            ));
        }
        return Ok(());
    }
    if sizes.len() > c::MAX_FILES {
        return Err(Refused::not_published(
            "too_many",
            format!(
                "a version holds at most {} files, and this holds {}.",
                c::MAX_FILES,
                sizes.len()
            ),
        ));
    }
    for (path, bytes) in sizes {
        let (most, what) = if ext(path) == "svg" {
            (c::MAX_SVG_BYTES, "an SVG")
        } else if is_text(path) {
            (c::MAX_TEXT_BYTES, "a text file")
        } else {
            (
                c::MAX_BINARY_BYTES,
                "a binary file (an image, a font, a video, a PDF)",
            )
        };
        if *bytes > most {
            return Err(Refused::not_published(
                "too_big",
                format!(
                    "{path} is {}, and {what} may be {} at most.",
                    size_text(*bytes),
                    size_text(most)
                ),
            ));
        }
    }
    if total > c::MAX_VERSION_BYTES {
        return Err(Refused::not_published(
            "too_big",
            format!(
                "this version is {}, and a version may be {} at most.",
                size_text(total),
                size_text(c::MAX_VERSION_BYTES)
            ),
        ));
    }
    Ok(())
}

/// What an upload is, from its first bytes (Claude's `/_blob/<id>` has no extension).
pub fn sniff(b: &[u8]) -> &'static str {
    let starts = |m: &[u8]| b.starts_with(m);
    if starts(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if starts(&[0xff, 0xd8, 0xff]) {
        "jpg"
    } else if starts(b"GIF8") {
        "gif"
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "webp"
    } else if starts(b"wOF2") {
        "woff2"
    } else if starts(b"wOFF") {
        "woff"
    } else if starts(&[0, 1, 0, 0]) {
        "ttf"
    } else if starts(b"OTTO") {
        "otf"
    } else if starts(b"%PDF") {
        "pdf"
    } else if b.len() >= 12 && &b[4..8] == b"ftyp" {
        if &b[8..12] == b"avif" || &b[8..12] == b"avis" {
            "avif"
        } else {
            "mp4"
        }
    } else if starts(&[0x1a, 0x45, 0xdf, 0xa3]) {
        "webm"
    } else {
        let head = String::from_utf8_lossy(&b[..b.len().min(512)]);
        let t = head.trim_start();
        if t.starts_with("<svg") || (t.starts_with("<?xml") && head.contains("<svg")) {
            "svg"
        } else {
            "bin"
        }
    }
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Claude's uploads, `_blob/<id>` at the version's root, move to `assets/<name>.<ext>` (the type
/// from their bytes), and the map `{id: kept path}` is what [`resolve`] serves them by. The boards'
/// bytes are never rewritten, so an export gives Claude back what it sent ([`export_entries`]). An
/// id that isn't a plain name, or looks like a token ([`long_run`]), is kept under the first 16 hex
/// of its SHA-256 instead of refused.
pub fn ingest_blobs(files: &mut Files, rules: &Rules) -> BTreeMap<String, String> {
    let ids: Vec<String> = files
        .keys()
        .filter_map(|p| p.strip_prefix(c::BLOB_PREFIX))
        .filter(|id| !id.is_empty() && !id.contains('/'))
        .map(str::to_string)
        .collect();
    let mut map = BTreeMap::new();
    for id in ids {
        let Some(bytes) = files.remove(&format!("{}{id}", c::BLOB_PREFIX)) else {
            continue;
        };
        let kind = sniff(&bytes);
        let hashed = || sha_hex(id.as_bytes())[..16].to_string();
        let name = if !rules.names || (plain_name(&id) && !long_run(&id)) {
            id.clone()
        } else {
            hashed()
        };
        let mut kept = format!("assets/{name}.{kind}");
        if files.get(&kept).is_some_and(|b| *b != bytes) {
            kept = format!("assets/blob-{}.{kind}", hashed());
        }
        files.insert(kept.clone(), bytes);
        map.insert(id, kept);
    }
    map
}

/// obid's name scan, the part the lab keeps: a path segment, or the title, that looks like a token
/// refuses the whole publish, saying no name.
pub fn check_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<(), Refused> {
    for name in names {
        if name.split(['/', '.']).any(long_run) || long_run(name) {
            return Err(Refused::new(422, "not_published", NAME_REFUSED));
        }
    }
    Ok(())
}

/// Every field must be there to pass; only `file` is read further.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct Frame {
    title: String,
    file: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// obid's `canvas`: a list of frames, each an HTML file of the folder; with [`Rules::frames`], also
/// Swift's `{"frames": […]}`, so obid, Swift and Kotlin read the same files.
pub fn check_canvas(canvas: &Value, files: &Files, rules: &Rules) -> Result<(), Refused> {
    let bad = |why: String| Refused::not_published("bad_canvas", why);
    let shape = || {
        bad("canvas.json is a list of frames, each {\"title\", \"file\", \"x\", \"y\", \"w\", \"h\"}.".into())
    };
    let list = match canvas {
        Value::Array(_) => canvas.clone(),
        Value::Object(o) if rules.frames && o.get("frames").is_some_and(Value::is_array) => {
            o["frames"].clone()
        }
        _ => return Err(shape()),
    };
    let frames: Vec<Frame> = serde_json::from_value(list).map_err(|_| shape())?;
    if frames.is_empty() {
        return Err(bad("canvas.json has no frame.".into()));
    }
    match frames
        .iter()
        .find(|f| ext(&f.file) != "html" || !files.contains_key(&f.file))
    {
        Some(f) => Err(bad(format!(
            "canvas.json's frame {:?} isn't an HTML file of the canvas.",
            f.file
        ))),
        None => Ok(()),
    }
}

/// Whether a `canvas.json` is Claude Design's object rather than obid's or Swift's frames.
pub fn is_design(canvas: &Value) -> bool {
    canvas.is_object() && (canvas.get("boards").is_some() || canvas.get("v").is_some())
}

/// How many `<script type="text/x-dc" data-dc-script>` a board holds.
fn dc_scripts(text: &str) -> usize {
    static TAG: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let tag = TAG.get_or_init(|| {
        regex::Regex::new(r#"(?i)<script\b[^>]*\btype\s*=\s*["']text/x-dc["'][^>]*>"#)
            .expect("it compiles")
    });
    tag.find_iter(text)
        .filter(|m| m.as_str().contains("data-dc-script"))
        .count()
}

/// Claude Design's `canvas.json` and its boards, checked as they are (the plan's §3.1); nothing is
/// derived into them. Each refusal names what is wrong.
pub fn check_design(canvas: &Value, files: &Files) -> Result<(), Refused> {
    let bad = |why: String| Refused::not_published("bad_design", why);
    if canvas["v"] != json!(3) {
        return Err(bad(format!(
            "canvas.json's v is 3 in Claude Design's format, and here it is {}.",
            canvas["v"]
        )));
    }
    let boards = canvas["boards"]
        .as_object()
        .filter(|b| !b.is_empty())
        .ok_or_else(|| bad("canvas.json has no board.".into()))?;
    for name in boards.keys() {
        if !name.ends_with(".dc.html") {
            return Err(bad(format!(
                "canvas.json's board {name:?} isn't a .dc.html file."
            )));
        }
        let Some(text) = files.get(name) else {
            return Err(bad(format!(
                "canvas.json's board {name:?} isn't a file of the design."
            )));
        };
        let text = String::from_utf8_lossy(text);
        if !text.contains("<x-dc>") {
            return Err(bad(format!("{name} has no <x-dc>.")));
        }
        let scripts = dc_scripts(&text);
        if scripts != 1 {
            return Err(bad(format!(
                "{name} has {scripts} <script type=\"text/x-dc\" data-dc-script> blocks, and a board has one."
            )));
        }
    }
    if let Some(order) = canvas.get("order").filter(|o| !o.is_null()) {
        let order = order
            .as_array()
            .ok_or_else(|| bad("canvas.json's order is a list of boards.".into()))?;
        if let Some(stray) = order
            .iter()
            .find(|b| !b.as_str().is_some_and(|b| boards.contains_key(b)))
        {
            return Err(bad(format!(
                "canvas.json's order names {stray}, which isn't one of its boards."
            )));
        }
    }
    static PAGE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let page_id =
        PAGE.get_or_init(|| regex::Regex::new(r"^[A-Za-z0-9_-]{1,40}$").expect("it compiles"));
    let mut pages = BTreeSet::new();
    for page in canvas["pages"].as_array().into_iter().flatten() {
        let id = page["id"].as_str().unwrap_or_default();
        if !page_id.is_match(id) {
            return Err(bad(format!(
                "canvas.json's page id {:?} isn't 1 to 40 letters, digits, _ or -.",
                page["id"]
                    .as_str()
                    .map_or(page["id"].to_string(), str::to_string)
            )));
        }
        pages.insert(id.to_string());
    }
    if let Some(launch) = canvas.get("launch").filter(|l| !l.is_null()) {
        let file = launch["file"].as_str();
        let page = launch["page"].as_str();
        let stray = file
            .filter(|f| !boards.contains_key(*f))
            .or(page.filter(|p| !pages.contains(*p)));
        if let Some(stray) = stray {
            return Err(bad(format!(
                "canvas.json's launch names {stray:?}, which isn't one of its pages or boards."
            )));
        }
    }
    if let Some(notes) = canvas.get("notes") {
        if notes.to_string().len() as u64 > c::MAX_NOTES_BYTES {
            return Err(bad(format!(
                "canvas.json's notes are over {}.",
                size_text(c::MAX_NOTES_BYTES)
            )));
        }
    }
    for system in canvas["designSystems"].as_array().into_iter().flatten() {
        let ns = system["namespace"].as_str().unwrap_or_default();
        let folder = format!("ds/{ns}/");
        if !plain_name(ns) || !files.keys().any(|f| f.starts_with(&folder)) {
            return Err(bad(format!(
                "canvas.json's design system {ns:?} has no ds/{ns}/ folder."
            )));
        }
    }
    Ok(())
}

/// The kind a version's files are (a `folder`, or one file), when it is `wanted` too.
pub fn kind_of(
    files: &Files,
    folder: bool,
    wanted: Option<Kind>,
    rules: &Rules,
) -> Result<Kind, Refused> {
    let kind = if folder {
        if let Some(text) = files.get("canvas.json") {
            let canvas: Value = serde_json::from_slice(text)
                .map_err(|_| Refused::not_published("bad_canvas", "canvas.json isn't JSON."))?;
            if rules.design && is_design(&canvas) {
                check_design(&canvas, files)?;
                Kind::Design
            } else {
                check_canvas(&canvas, files, rules)?;
                Kind::Canvas
            }
        } else if files.contains_key("index.html") {
            Kind::Html
        } else {
            return Err(Refused::not_published(
                "not_published",
                "a folder is a page with an index.html, a canvas with a canvas.json, or a Claude Design project.",
            ));
        }
    } else {
        let name = files.keys().next().map_or("", String::as_str);
        match ext(name).as_str() {
            "html" | "htm" => Kind::Html,
            "md" | "markdown" => Kind::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => Kind::Image,
            "pdf" => Kind::Pdf,
            _ => {
                return Err(Refused::not_published(
                    "not_published",
                    format!("{name} isn't a page (.html), a document (.md), an image (.png, .jpg, .gif, .webp, .svg) or a PDF."),
                ))
            }
        }
    };
    if let Some(wanted) = wanted.filter(|w| *w != kind) {
        return Err(Refused::not_published(
            "not_published",
            format!(
                "it was asked for as {}, and it is {}.",
                wanted.as_str(),
                kind.as_str()
            ),
        ));
    }
    Ok(kind)
}

/// The media type a file is served with, by its extension: obid's table, and with
/// [`Rules::types`] the fonts, video and AVIF a design needs. `.jsx` and `.ts` stay
/// `application/octet-stream`: the format needs neither.
pub fn type_of(path: &str, rules: &Rules) -> &'static str {
    let e = ext(path);
    if rules.types {
        match e.as_str() {
            "ttf" => return "font/ttf",
            "otf" => return "font/otf",
            "mp4" => return "video/mp4",
            "webm" => return "video/webm",
            "avif" => return "image/avif",
            _ => {}
        }
    }
    match e.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// A path as `files` lists it: no empty part, no `.` or `..`, no hidden name, no `\`, `:` or NUL.
pub fn plain_path(path: &str) -> bool {
    path.split('/')
        .all(|p| !p.is_empty() && !p.starts_with('.') && !p.contains(['\\', '\0', ':']))
}

/// The file of `version` a request names: one of its files, or with [`Rules::alias`] a
/// `_blob/<id>` its map holds. Anything else (another version's upload, `..`, a hidden name) is
/// nothing, which the route answers `404`.
pub fn resolve(version: &Version, path: &str, rules: &Rules) -> Option<String> {
    if !plain_path(path) {
        return None;
    }
    if let Some(id) = path.strip_prefix(c::BLOB_PREFIX) {
        if rules.alias {
            return version.blobs.get(id).cloned();
        }
    }
    version
        .files
        .iter()
        .any(|f| f == path)
        .then(|| path.to_string())
}

/// Whether a publish from `conversation` and `job` may give `artifact` a new version: the one that
/// made it, or, with [`Rules::crossjob`], any job of the conversation it belongs to (the chat's
/// follow-up is a new job in the same conversation, TASKCONV-3).
pub fn may_version(
    artifact: &Artifact,
    conversation: Option<&str>,
    job: Option<&str>,
    rules: &Rules,
) -> Result<(), Refused> {
    let maker = artifact.conversation.as_deref() == conversation && artifact.job.as_deref() == job;
    let same_conversation = rules.crossjob
        && artifact.conversation.is_some()
        && artifact.conversation.as_deref() == conversation;
    if maker || same_conversation {
        Ok(())
    } else {
        Err(Refused::not_published(
            "made_elsewhere",
            format!(
                "artifact {} belongs to another conversation, so only there can it get a new version.",
                artifact.id
            ),
        ))
    }
}

/// Where the owner's words go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landing {
    /// An existing conversation.
    In(String),
    /// A new conversation, and it becomes the open artifact's (TASKCONV-4's rule).
    NewForArtifact,
    /// A new conversation, nothing open.
    New,
}

/// With [`Rules::turn`], words said while an artifact was open land in its conversation, whatever
/// conversation the app had open; an artifact with none gets one.
pub fn land_words(open: Option<&str>, artifact: Option<&Artifact>, rules: &Rules) -> Landing {
    match artifact {
        Some(a) if rules.turn => match &a.conversation {
            Some(conversation) => Landing::In(conversation.clone()),
            None => Landing::NewForArtifact,
        },
        _ => match open {
            Some(conversation) => Landing::In(conversation.to_string()),
            None => Landing::New,
        },
    }
}

/// A version as its export's entries: a `design` under `project/` at Claude Design's paths, each
/// upload back at `project/_blob/<id>`, every byte as kept; any other kind at the zip's root.
pub fn export_entries(
    kind: Kind,
    files: &Files,
    blobs: &BTreeMap<String, String>,
    rules: &Rules,
) -> Files {
    let back: BTreeMap<&str, &str> = blobs
        .iter()
        .map(|(id, kept)| (kept.as_str(), id.as_str()))
        .collect();
    let prefix = if kind == Kind::Design { "project/" } else { "" };
    files
        .iter()
        .map(|(path, bytes)| {
            let name = match back.get(path.as_str()) {
                Some(id) => format!("{prefix}{}{id}", c::BLOB_PREFIX),
                None => format!("{prefix}{path}"),
            };
            let bytes = if !rules.exact && path == "canvas.json" {
                serde_json::from_slice::<Value>(bytes)
                    .ok()
                    .and_then(|v| serde_json::to_vec_pretty(&v).ok())
                    .unwrap_or_else(|| bytes.clone())
            } else {
                bytes.clone()
            };
            (name, bytes)
        })
        .collect()
}

/// A zip of `entries`, the same bytes every time: names sorted, one fixed time (1980-01-01),
/// mode 0644, deflate.
pub fn zip_of(entries: &Files) -> Result<Vec<u8>> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for (name, bytes) in entries {
        w.start_file(name.as_str(), options)?;
        w.write_all(bytes)?;
    }
    Ok(w.finish()?.into_inner())
}

/// A zip's files: every entry a plain path (no `/` at its start, no `..`, no `\`), never a link,
/// each read only as far as its limit allows; hidden names and `__MACOSX/` left out.
pub fn unzip(bytes: &[u8]) -> Result<Files, Refused> {
    let bad = |why: String| Refused::new(400, "bad_zip", format!("Not published: {why}"));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| bad(format!("that isn't a zip ({e}).")))?;
    let mut out = Files::new();
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| bad(format!("the zip can't be read ({e}).")))?;
        let name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(bad(format!(
                "{name:?} is a link, which obid never follows."
            )));
        }
        if name.starts_with('/')
            || name.contains(['\\', '\0', ':'])
            || name.split('/').any(|p| p.is_empty() || p == "..")
        {
            return Err(bad(format!("{name:?} isn't a plain path inside the zip.")));
        }
        if name
            .split('/')
            .any(|p| p.starts_with('.') || p == "__MACOSX")
        {
            continue;
        }
        let cap = c::MAX_TEXT_BYTES.max(c::MAX_BINARY_BYTES) + 1;
        let mut data = Vec::new();
        (&mut entry)
            .take(cap)
            .read_to_end(&mut data)
            .map_err(|e| bad(format!("{name:?} can't be read ({e}).")))?;
        total += data.len() as u64;
        out.insert(name, data);
        if out.len() > c::MAX_FILES || total > c::MAX_VERSION_BYTES {
            // One more than allowed is enough for check_sizes to say which.
            break;
        }
    }
    Ok(out)
}

/// What a zip holds: Claude Design's project archive (`project/canvas.json`: its `project/`
/// folder is the version), a zip of one folder (that folder), or files at its root.
pub fn project_of(files: Files) -> (Files, &'static str) {
    if files.contains_key("project/canvas.json") {
        let inside = files
            .into_iter()
            .filter_map(|(p, b)| p.strip_prefix("project/").map(|p| (p.to_string(), b)))
            .collect();
        return (inside, "claude-design-zip");
    }
    let tops: BTreeSet<&str> = files
        .keys()
        .map(|p| p.split('/').next().unwrap_or(""))
        .collect();
    if tops.len() == 1 && files.keys().all(|p| p.contains('/')) {
        let top = format!("{}/", tops.iter().next().unwrap());
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
    let mime = type_of(path, &Rules::ALL).split(';').next().unwrap_or("");
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
/// stylesheet's own `url(…)`s first).
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
                let url = if ext(&path) == "css" && depth < 4 {
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
/// each a `data:` URL in `<script type="application/json" id="oby-artifact-files">`, for Oby's
/// runtime to draw (it comes with the `artifactview` PR, which puts itself in this file).
pub fn export_html(
    kind: Kind,
    title: &str,
    files: &Files,
    blobs: &BTreeMap<String, String>,
) -> String {
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
            let boards: Vec<String> = files
                .get("canvas.json")
                .and_then(|t| serde_json::from_slice::<Value>(t).ok())
                .map(|c| {
                    let names: Vec<String> = match &c["boards"] {
                        Value::Object(b) => b
                            .iter()
                            .map(|(k, v)| v["title"].as_str().unwrap_or(k).to_string())
                            .collect(),
                        _ => vec![],
                    };
                    names
                })
                .unwrap_or_default();
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

/// References in an exported HTML file that reach outside it (anything but `data:` and `#`).
pub fn outside_references(html: &str) -> Vec<String> {
    static REF: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = REF.get_or_init(|| {
        regex::Regex::new(r#"(?i)\b(?:src|href)\s*=\s*"([^"]*)"|url\(\s*['"]?([^'")]*)"#)
            .expect("it compiles")
    });
    re.captures_iter(html)
        .filter_map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()))
        .filter(|r| !r.is_empty() && !r.starts_with("data:") && !r.starts_with('#'))
        .collect()
}

/// The title trimmed, when it is one line of 1 to 100 characters (obid's `clean_title`).
pub fn clean_title(title: &str) -> Option<String> {
    let title = title.trim();
    let fine = !title.is_empty()
        && title.chars().count() <= 100
        && !title
            .chars()
            .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}');
    fine.then(|| title.to_owned())
}

/// The owner's devices and the signatures already used: every owner's act needs a fresh signature
/// by one of them over the request's exact body (obid's words, as the browser's API checks them).
#[derive(Default)]
pub struct Words {
    devices: BTreeMap<String, Vec<u8>>,
    seen: BTreeSet<String>,
}

impl Words {
    pub fn add(&mut self, id: &str, public_b64: &str) -> Result<(), Refused> {
        use base64::Engine as _;
        let key = base64::engine::general_purpose::STANDARD
            .decode(public_b64)
            .ok()
            .filter(|k| p256::ecdsa::VerifyingKey::from_sec1_bytes(k).is_ok())
            .ok_or_else(|| Refused::new(400, "bad_request", "That isn't a P-256 public key."))?;
        self.devices.insert(id.into(), key);
        Ok(())
    }

    /// The device that signed the request, or why its words don't count. `now_ms` is the caller's.
    pub fn verify(
        &mut self,
        header: Option<&str>,
        method: &str,
        path: &str,
        body: &[u8],
        now_ms: i64,
        rules: &Rules,
    ) -> Result<String, Refused> {
        if !rules.signature {
            return Ok("owner".into());
        }
        let header = header.ok_or_else(|| {
            Refused::new(
                401,
                "unsigned",
                "The owner's acts on artifacts are signed by one of their devices.",
            )
        })?;
        let parts: Vec<&str> = header.split(' ').collect();
        let [device, at, signature] = parts.as_slice() else {
            return Err(Refused::new(
                401,
                "bad_signature",
                "X-Oby-Words is `<device> <signedAt> <signature>`.",
            ));
        };
        let key = self.devices.get(*device).ok_or_else(|| {
            Refused::new(
                401,
                "unknown_device",
                "That device isn't one of the owner's.",
            )
        })?;
        let signed_at = chrono::DateTime::parse_from_rfc3339(at)
            .map_err(|_| Refused::new(401, "bad_signature", "The signature's time isn't a time."))?
            .timestamp_millis();
        if (now_ms - signed_at).abs() > c::WORDS_MAX_SKEW_MS {
            return Err(Refused::new(
                401,
                "bad_signature",
                "The signature is too old, or from the future.",
            ));
        }
        if self.seen.contains(*signature) {
            return Err(Refused::new(
                401,
                "bad_signature",
                "That signature was already used.",
            ));
        }
        use base64::Engine as _;
        use p256::ecdsa::signature::Verifier as _;
        let message = super::browserapi::words_message(method, path, body, at);
        let ok = (|| {
            let vk = p256::ecdsa::VerifyingKey::from_sec1_bytes(key).ok()?;
            let der = base64::engine::general_purpose::STANDARD
                .decode(signature)
                .ok()?;
            let sig = p256::ecdsa::Signature::from_der(&der).ok()?;
            vk.verify(message.as_bytes(), &sig).ok()
        })();
        if ok.is_none() {
            return Err(Refused::new(
                401,
                "bad_signature",
                "The signature isn't the device's over this request.",
            ));
        }
        self.seen.insert(signature.to_string());
        Ok(device.to_string())
    }
}

// ════════════════════════════════════════════════════════════════════════════════════════════
// 2. The lab: the store on disk, the server, fixtures, the nodes.
// ════════════════════════════════════════════════════════════════════════════════════════════

pub const NODES: &[(&str, &str)] = &[
    ("lab.artifacts.up", "Starts the artifacts' store and its API (the contract's routes) on 127.0.0.1 (a free port), the store under the workflow's scratch folder: {url, port, token, home}. One per workflow."),
    ("lab.artifacts.device", "Registers a new device key of the owner's: {id, key, publicKey}, for `as: device` calls."),
    ("lab.artifacts.fixture", "Writes a fixture into the scratch folder: `name` claude-design (a Claude Design project of the lab's own: canvas.json v3, three boards, a design system, fonts, video, and three uploads under _blob/, one with a token-like id), canvas (the Oby Artifacts canvas, lab-ui/artifacts/design), broken-design (a board that throws, one that doesn't parse), canvas-obid, canvas-swift, page, document, sized (`files`: [{path, bytes}], sparse) or many (`count` small files); `break` spoils a claude-design one way; `zip: true` also writes it as Claude Design's project zip. {path, zip, files, blobs}."),
    ("lab.artifacts.publish", "Publishes `path` as an agent's oby_artifact does (in-process, no route): `title`, `kind`, `conversation`, `job`, `artifact` (a new version); `write` ({name: text}) changes files of the source first; `repeat` publishes that many more versions, each with a changed version.txt. {ok, id, version, reason, error, artifact, sameAsSource}; with expect."),
    ("lab.artifacts.call", "One request to the API: `method`, `path`, `body`; `as` owner (the default), none, wrong, device, unsigned, badsig, unknown-device. {status, body}; with expect."),
    ("lab.artifacts.import", "POST /v1/artifacts/import with the bytes of `file` (`contentType`, default application/zip), signed by `device` unless `as` says otherwise: {status, body}; with expect."),
    ("lab.artifacts.serve", "GET one of a version's files (`id`, `version`, `path`): {status, type, csp, nosniff, bytes, sha256, same (against `sameAs`, a local file)}; with expect."),
    ("lab.artifacts.export", "GET /v1/artifacts/{id}/export/{version}?as=…: {status, type, bytes, sha256, saved, entries (a zip's: [{name, sha256, bytes}]), outside (an HTML file's references past itself), files (the HTML file's embedded files)}; with expect."),
    ("lab.artifacts.roundtrip", "Imports `zip` (signed by `device`), exports it as a zip, and compares: {imported, same, onlyInSource, onlyInExport, changed, stable (a second import and export gives the same zip bytes)}; with expect."),
    ("lab.artifacts.turn", "POST /v1/plans with `text`, `conversation` and `artifact` ({id, version}), signed by `device`: {status, body}; with expect."),
    ("lab.artifacts.events", "The `artifact` events the API sent so far: {events, count}; with expect."),
    ("lab.artifacts.alive", "Whether the API still answers on its port: {answers}; with expect."),
    ("lab.artifacts.down", "Stops the API now rather than at the cleanup: {stopped}."),
];

fn rules_here() -> Rules {
    match std::env::var("OBID_SPEC_LAB_ARTIFACTS_BREAK") {
        Ok(names) if !names.is_empty() => Rules::broken(&names),
        _ => Rules::ALL,
    }
}

fn now_text() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn is_id(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::try_parse(id).is_ok() && id == id.to_lowercase()
}

/// What to keep: a version's files, read from an agent's folder or a zip, and who made it.
struct Keep {
    files: Files,
    folder: bool,
    title: String,
    kind: Option<Kind>,
    conversation: Option<String>,
    job: Option<String>,
    artifact: Option<String>,
    origin: Origin,
}

/// The lab's store: obid's `home/artifacts/<id>/{artifact.json,<version>/…}` under the scratch
/// folder, the owner's devices, the events sent and the conversations the turns made.
struct Store {
    dir: PathBuf,
    rules: Rules,
    token: String,
    words: Words,
    events: Vec<Value>,
    turns: Vec<Value>,
}

impl Store {
    fn get(&self, id: &str) -> Option<Artifact> {
        if !is_id(id) {
            return None;
        }
        let text = std::fs::read(self.dir.join(id).join("artifact.json")).ok()?;
        serde_json::from_slice(&text).ok()
    }

    fn list(&self, conversation: Option<&str>, job: Option<&str>) -> Vec<Artifact> {
        let mut all: Vec<Artifact> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| self.get(e.file_name().to_str()?))
            .filter(|a| conversation.is_none_or(|c| a.conversation.as_deref() == Some(c)))
            .filter(|a| job.is_none_or(|j| a.job.as_deref() == Some(j)))
            .collect();
        all.sort_by(|a, b| (&b.created, &b.id).cmp(&(&a.created, &a.id)));
        all
    }

    fn save(&self, a: &Artifact) -> std::io::Result<()> {
        let folder = self.dir.join(&a.id);
        std::fs::create_dir_all(&folder)?;
        let tmp = folder.join(".artifact.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(a)?)?;
        std::fs::rename(tmp, folder.join("artifact.json"))
    }

    fn version_files(&self, a: &Artifact, version: u32) -> Option<Files> {
        let v = a.versions.iter().find(|v| v.version == version)?;
        let folder = self.dir.join(&a.id).join(version.to_string());
        v.files
            .iter()
            .map(|f| Some((f.clone(), std::fs::read(folder.join(f)).ok()?)))
            .collect()
    }

    fn event(&mut self, a: &Artifact, version: Option<u32>, renamed: bool) {
        let e = ArtifactEvent {
            id: a.id.clone(),
            version,
            title: a.title.clone(),
            kind: a.kind,
            conversation: a.conversation.clone(),
            job: a.job.clone(),
            renamed: renamed.then_some(true),
        };
        self.events
            .push(json!({"type": "artifact", "data": serde_json::to_value(e).unwrap_or_default()}));
    }

    /// The store's half of a publish or an import: checked, then kept as a new version.
    fn keep(&mut self, mut what: Keep) -> Result<(String, u32), Refused> {
        let rules = self.rules;
        let title = clean_title(&what.title).ok_or_else(|| {
            Refused::new(
                422,
                "bad_title",
                "Not published: the title is one line of 1 to 100 characters.",
            )
        })?;
        let before = match &what.artifact {
            Some(id) => {
                let a = self.get(id).ok_or_else(|| {
                    Refused::new(
                        404,
                        "not_found",
                        "Not published: there's no artifact by that id.",
                    )
                })?;
                may_version(
                    &a,
                    what.conversation.as_deref(),
                    what.job.as_deref(),
                    &rules,
                )?;
                if a.versions.len() >= c::MAX_VERSIONS as usize {
                    return Err(Refused::not_published(
                        "full",
                        format!(
                            "artifact {} has {} versions, the most obid keeps; publish it as a new one.",
                            a.id,
                            c::MAX_VERSIONS
                        ),
                    ));
                }
                Some(a)
            }
            None => None,
        };
        let sizes: Vec<(String, u64)> = what
            .files
            .iter()
            .map(|(p, b)| (p.clone(), b.len() as u64))
            .collect();
        check_sizes(&sizes, &rules)?;
        let kind = kind_of(
            &what.files,
            what.folder,
            what.kind.or(before.as_ref().map(|a| a.kind)),
            &rules,
        )?;
        let blobs = if kind == Kind::Design {
            ingest_blobs(&mut what.files, &rules)
        } else {
            BTreeMap::new()
        };
        check_names(
            what.files
                .keys()
                .map(String::as_str)
                .chain([title.as_str()]),
        )?;
        if let Some(before) = &before {
            if before.kind != kind {
                return Err(Refused::not_published(
                    "not_published",
                    format!(
                        "artifact {} is {}, and this is {}.",
                        before.id,
                        before.kind.as_str(),
                        kind.as_str()
                    ),
                ));
            }
            let latest = before.versions.last().map_or(0, |v| v.version);
            if self.version_files(before, latest).as_ref() == Some(&what.files) {
                return Ok((before.id.clone(), latest));
            }
        }
        let bytes: u64 = what.files.values().map(|b| b.len() as u64).sum();
        // A job's artifacts, or a conversation's own, hold at most so much in all.
        let theirs: Vec<Artifact> = match (&what.job, &what.conversation) {
            (Some(job), _) => self.list(None, Some(job)),
            (None, Some(conv)) => self
                .list(Some(conv), None)
                .into_iter()
                .filter(|a| a.job.is_none())
                .collect(),
            (None, None) => vec![],
        };
        let held: u64 = theirs
            .iter()
            .flat_map(|a| &a.versions)
            .map(|v| v.bytes)
            .sum();
        if held + bytes > c::MAX_OWNER_BYTES {
            return Err(Refused::not_published(
                "too_big",
                format!(
                    "what made it holds {} of artifacts, and {} is the most it may.",
                    size_text(held),
                    size_text(c::MAX_OWNER_BYTES)
                ),
            ));
        }
        let now = now_text();
        let mut artifact = before.unwrap_or_else(|| Artifact {
            id: uuid::Uuid::new_v4().to_string().to_lowercase(),
            title: title.clone(),
            kind,
            conversation: what.conversation.clone(),
            job: what.job.clone(),
            created: now.clone(),
            versions: vec![],
            profile: None,
            origin: what.origin.clone(),
            shares: vec![],
            public: None,
        });
        let version = artifact.versions.last().map_or(0, |v| v.version) + 1;
        let folder = self.dir.join(&artifact.id).join(version.to_string());
        let write = || -> std::io::Result<()> {
            for (path, data) in &what.files {
                let to = folder.join(path);
                std::fs::create_dir_all(to.parent().unwrap_or(&folder))?;
                std::fs::write(to, data)?;
            }
            Ok(())
        };
        write().map_err(|e| {
            Refused::not_published("not_published", format!("obid couldn't keep it ({e})."))
        })?;
        artifact.versions.push(Version {
            version,
            created: now,
            files: what.files.keys().cloned().collect(),
            bytes,
            blobs,
            job: what.job.clone(),
        });
        self.save(&artifact).map_err(|e| {
            Refused::not_published("not_published", format!("obid couldn't keep it ({e})."))
        })?;
        self.event(&artifact, Some(version), false);
        Ok((artifact.id, version))
    }
}

/// One version of a stored artifact, read from the store's folder (`home/artifacts`): the artifact,
/// that version and its files. The viewer (src/lab/artifactview.rs) serves from it.
pub(crate) fn read_version(
    dir: &Path,
    id: &str,
    version: u32,
) -> Option<(Artifact, Version, Files)> {
    if !is_id(id) {
        return None;
    }
    let a: Artifact =
        serde_json::from_slice(&std::fs::read(dir.join(id).join("artifact.json")).ok()?).ok()?;
    let v = a.versions.iter().find(|v| v.version == version)?.clone();
    let folder = dir.join(id).join(version.to_string());
    let files = v
        .files
        .iter()
        .map(|f| Some((f.clone(), std::fs::read(folder.join(f)).ok()?)))
        .collect::<Option<Files>>()?;
    Some((a, v, files))
}

/// A stored artifact as `artifact.json` says it.
pub(crate) fn read_artifact(dir: &Path, id: &str) -> Option<Artifact> {
    if !is_id(id) {
        return None;
    }
    serde_json::from_slice(&std::fs::read(dir.join(id).join("artifact.json")).ok()?).ok()
}

/// An agent's file or folder as a version's files, never following a link: the sizes checked
/// first ([`check_sizes`]), before a byte is read. Hidden names are left out.
pub(crate) fn read_source(path: &Path, rules: &Rules) -> Result<(Files, bool), Refused> {
    let shown = |p: &Path| {
        p.strip_prefix(path)
            .ok()
            .filter(|r| !r.as_os_str().is_empty())
            .unwrap_or(p)
            .display()
            .to_string()
    };
    let link = |p: &Path| {
        Refused::not_published(
            "not_published",
            format!("{} is a link, which obid never follows.", shown(p)),
        )
    };
    let io = |e: std::io::Error| {
        Refused::not_published("not_published", format!("it can't be read ({e})."))
    };
    let meta = std::fs::symlink_metadata(path).map_err(io)?;
    if meta.file_type().is_symlink() {
        return Err(link(path));
    }
    if meta.is_file() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let name = if matches!(ext(&name).as_str(), "html" | "htm") {
            "index.html".to_string()
        } else {
            name
        };
        check_sizes(&[(name.clone(), meta.len())], rules)?;
        let bytes = std::fs::read(path).map_err(io)?;
        return Ok((Files::from([(name, bytes)]), false));
    }
    let mut sizes = vec![];
    let mut stack = vec![(path.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > 32 {
            return Err(Refused::not_published(
                "not_published",
                "it is more than 32 folders deep.",
            ));
        }
        let mut entries: Vec<_> = std::fs::read_dir(&dir).map_err(io)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let p = e.path();
            let m = std::fs::symlink_metadata(&p).map_err(io)?;
            if m.file_type().is_symlink() {
                return Err(link(&p));
            }
            if m.is_dir() {
                stack.push((p, depth + 1));
            } else if m.is_file() {
                sizes.push((shown(&p), m.len()));
            } else {
                return Err(Refused::not_published(
                    "not_published",
                    format!("{} isn't a plain file or folder.", shown(&p)),
                ));
            }
        }
        if sizes.len() > c::MAX_FILES.max(2000) {
            break;
        }
    }
    check_sizes(&sizes, rules)?;
    let mut files = Files::new();
    for (rel, _) in sizes {
        files.insert(rel.clone(), std::fs::read(path.join(&rel)).map_err(io)?);
    }
    Ok((files, true))
}

// ── HTTP, just enough ────────────────────────────────────────────────────────────────────────

struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

async fn read_request(conn: &mut tokio::net::TcpStream) -> Result<Request> {
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        anyhow::ensure!(buf.len() < 64 * 1024, "the request's head is too long");
        let n = conn.read(&mut chunk).await?;
        anyhow::ensure!(n > 0, "the connection closed");
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let first: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let headers: BTreeMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    anyhow::ensure!(
        len as u64 <= c::MAX_VERSION_BYTES + c::MB,
        "the body is too long"
    );
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = conn.read(&mut chunk).await?;
        anyhow::ensure!(n > 0, "the connection closed mid-body");
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(len);
    Ok(Request {
        method: first.first().copied().unwrap_or("").into(),
        path: first.get(1).copied().unwrap_or("/").into(),
        headers,
        body,
    })
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn json(status: u16, body: &Value) -> Answer {
        Answer {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: if body.is_null() {
                vec![]
            } else {
                body.to_string().into_bytes()
            },
        }
    }
    fn refused(r: Refused) -> Answer {
        Answer::json(r.status, &r.body())
    }
}

async fn write_answer(conn: &mut tokio::net::TcpStream, a: Answer) {
    let reason = match a.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        422 => "Unprocessable Entity",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        a.status,
        a.body.len()
    );
    for (k, v) in &a.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = conn.write_all(head.as_bytes()).await;
    let _ = conn.write_all(&a.body).await;
    let _ = conn.shutdown().await;
}

pub(crate) fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn query_of(path: &str) -> BTreeMap<String, String> {
    path.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .filter_map(|(k, v)| Some((k.to_string(), percent_decode(v)?)))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Route {
    List,
    Get(String),
    Rename(String),
    Delete(String),
    File(String, String, String),
    Export(String, String),
    Import,
    Plans,
    NotFound,
}

impl Route {
    fn signed(&self) -> bool {
        matches!(
            self,
            Route::Rename(_) | Route::Delete(_) | Route::Import | Route::Plans
        )
    }
}

fn route(method: &str, path: &str) -> Route {
    let path = path.split('?').next().unwrap_or("");
    if path == c::PATH_PLANS {
        return if method == "POST" {
            Route::Plans
        } else {
            Route::NotFound
        };
    }
    if path == c::PATH_IMPORT && method == "POST" {
        return Route::Import;
    }
    let Some(rest) = path.strip_prefix(c::PATH_ARTIFACTS) else {
        return Route::NotFound;
    };
    let parts: Vec<&str> = rest.trim_start_matches('/').splitn(3, '/').collect();
    match (method, parts.as_slice()) {
        ("GET", [""]) => Route::List,
        ("GET", [id]) => Route::Get(id.to_string()),
        ("PATCH", [id]) => Route::Rename(id.to_string()),
        ("DELETE", [id]) => Route::Delete(id.to_string()),
        ("GET", [id, verb, version]) if *verb == c::VERB_EXPORT => {
            Route::Export(id.to_string(), version.to_string())
        }
        ("GET", [id, version, file]) => {
            Route::File(id.to_string(), version.to_string(), file.to_string())
        }
        _ => Route::NotFound,
    }
}

fn no_artifact() -> Refused {
    Refused::new(404, "not_found", "There's no such artifact.")
}

fn handle(store: &Mutex<Store>, req: &Request) -> Answer {
    let r = route(&req.method, &req.path);
    if r == Route::NotFound {
        return Answer::refused(Refused::new(404, "not_found", "There's no such route."));
    }
    let mut s = store.lock().unwrap();
    let bearer = req
        .header("authorization")
        .and_then(|a| a.strip_prefix("Bearer "))
        .map(str::trim);
    match bearer {
        None => return Answer::refused(Refused::new(401, "no_token", "obid's token is needed.")),
        Some(t) if t != s.token => {
            return Answer::refused(Refused::new(401, "bad_token", "That isn't obid's token."))
        }
        _ => {}
    }
    if r.signed() {
        let rules = s.rules;
        let path = req.path.split('?').next().unwrap_or("");
        if let Err(e) = s.words.verify(
            req.header(c::WORDS_HEADER),
            &req.method,
            path,
            &req.body,
            chrono::Utc::now().timestamp_millis(),
            &rules,
        ) {
            return Answer::refused(e);
        }
    }
    let rules = s.rules;
    let query = query_of(&req.path);
    match r {
        Route::List => {
            let all = s.list(
                query.get("conversation").map(String::as_str),
                query.get("job").map(String::as_str),
            );
            Answer::json(200, &serde_json::to_value(all).unwrap_or_default())
        }
        Route::Get(id) => match s.get(&id) {
            Some(a) => Answer::json(200, &serde_json::to_value(a).unwrap_or_default()),
            None => Answer::refused(no_artifact()),
        },
        Route::Rename(id) => {
            let Some(mut a) = s.get(&id) else {
                return Answer::refused(no_artifact());
            };
            let body: c::TitleBody = match serde_json::from_slice(&req.body) {
                Ok(b) => b,
                Err(e) => {
                    return Answer::refused(Refused::new(
                        400,
                        "bad_request",
                        format!("The body is {{\"title\": \"…\"}} ({e})."),
                    ))
                }
            };
            let Some(title) = clean_title(&body.title) else {
                return Answer::refused(Refused::new(
                    422,
                    "bad_title",
                    "A title is one line of 1 to 100 characters.",
                ));
            };
            a.title = title;
            if s.save(&a).is_err() {
                return Answer::refused(Refused::new(
                    404,
                    "not_found",
                    "It was deleted meanwhile.",
                ));
            }
            if rules.renamed {
                s.event(&a, None, true);
            }
            Answer::json(200, &serde_json::to_value(a).unwrap_or_default())
        }
        Route::Delete(id) => {
            if s.get(&id).is_none() {
                return Answer::refused(no_artifact());
            }
            // The sharing PR revokes its shares and its public link here, first (§3.6, §3.7).
            let _ = std::fs::remove_dir_all(s.dir.join(&id));
            Answer::json(204, &Value::Null)
        }
        Route::File(id, version, path) => {
            let Some(a) = s.get(&id) else {
                return Answer::refused(no_artifact());
            };
            let found = version.parse::<u32>().ok().and_then(|n| {
                let v = a.versions.iter().find(|v| v.version == n)?;
                let path = percent_decode(&path)?;
                let kept = resolve(v, &path, &rules)?;
                let bytes =
                    std::fs::read(s.dir.join(&a.id).join(n.to_string()).join(&kept)).ok()?;
                Some((kept, bytes))
            });
            match found {
                Some((kept, bytes)) => Answer {
                    status: 200,
                    headers: vec![
                        ("Content-Type".into(), type_of(&kept, &rules).into()),
                        ("Content-Security-Policy".into(), c::POLICY.into()),
                        ("X-Content-Type-Options".into(), "nosniff".into()),
                        ("Referrer-Policy".into(), "no-referrer".into()),
                    ],
                    body: bytes,
                },
                None => Answer::refused(Refused::new(404, "not_found", "There's no such file.")),
            }
        }
        Route::Export(id, version) => {
            let Some(a) = s.get(&id) else {
                return Answer::refused(no_artifact());
            };
            let n = version.parse::<u32>().unwrap_or(0);
            let (Some(v), Some(files)) = (
                a.versions.iter().find(|v| v.version == n),
                s.version_files(&a, n),
            ) else {
                return Answer::refused(Refused::new(404, "not_found", "There's no such version."));
            };
            let name: String = a
                .title
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' {
                        ch
                    } else {
                        '-'
                    }
                })
                .collect();
            match query.get("as").map(String::as_str) {
                Some("zip") => match zip_of(&export_entries(a.kind, &files, &v.blobs, &rules)) {
                    Ok(bytes) => Answer {
                        status: 200,
                        headers: vec![
                            ("Content-Type".into(), "application/zip".into()),
                            (
                                "Content-Disposition".into(),
                                format!("attachment; filename=\"{name}-v{n}.zip\""),
                            ),
                        ],
                        body: bytes,
                    },
                    Err(e) => Answer::refused(Refused::new(
                        422,
                        "no_export",
                        format!("The zip couldn't be made ({e})."),
                    )),
                },
                Some("html") => Answer {
                    status: 200,
                    headers: vec![
                        ("Content-Type".into(), "text/html; charset=utf-8".into()),
                        (
                            "Content-Disposition".into(),
                            format!("attachment; filename=\"{name}-v{n}.html\""),
                        ),
                    ],
                    body: export_html(a.kind, &a.title, &files, &v.blobs).into_bytes(),
                },
                _ => Answer::refused(Refused::new(
                    400,
                    "no_export",
                    "An export is ?as=zip or ?as=html; PDF and PNG come with the renderer.",
                )),
            }
        }
        Route::Import => {
            let kind = req.header("content-type").unwrap_or("application/zip");
            let made = if kind.starts_with("text/html") {
                Ok((
                    Files::from([("index.html".to_string(), req.body.clone())]),
                    false,
                    "html",
                ))
            } else if kind.starts_with("text/markdown") {
                Ok((
                    Files::from([("document.md".to_string(), req.body.clone())]),
                    false,
                    "markdown",
                ))
            } else if kind.starts_with("application/zip") || req.body.starts_with(b"PK\x03\x04") {
                unzip(&req.body).map(|f| {
                    let (files, from) = project_of(f);
                    (files, true, from)
                })
            } else {
                Err(Refused::new(
                    400,
                    "bad_request",
                    "An import is a zip, a page (text/html) or a document (text/markdown).",
                ))
            };
            let (files, folder, from) = match made {
                Ok(m) => m,
                Err(e) => return Answer::refused(e),
            };
            let title = files
                .get("canvas.json")
                .and_then(|t| serde_json::from_slice::<Value>(t).ok())
                .and_then(|c| c["title"].as_str().and_then(clean_title))
                .unwrap_or_else(|| "Imported".to_string());
            let kept = s.keep(Keep {
                files,
                folder,
                title,
                kind: None,
                conversation: None,
                job: None,
                artifact: None,
                origin: Origin::Import { from: from.into() },
            });
            match kept.map(|(id, _)| s.get(&id)) {
                Ok(Some(a)) => Answer::json(200, &serde_json::to_value(a).unwrap_or_default()),
                Ok(None) => Answer::refused(no_artifact()),
                Err(e) => Answer::refused(e),
            }
        }
        Route::Plans => {
            let body: c::TurnBody = match serde_json::from_slice(&req.body) {
                Ok(b) => b,
                Err(e) => {
                    return Answer::refused(Refused::new(
                        400,
                        "bad_request",
                        format!("The body isn't a turn's words ({e})."),
                    ))
                }
            };
            let artifact = match &body.artifact {
                Some(t) => match s.get(&t.id) {
                    Some(a) => Some(a),
                    None => return Answer::refused(no_artifact()),
                },
                None => None,
            };
            let landing = land_words(body.conversation.as_deref(), artifact.as_ref(), &rules);
            let fresh = || {
                format!(
                    "conversation-{}",
                    &uuid::Uuid::new_v4().simple().to_string()[..12]
                )
            };
            let (conversation, created) = match landing {
                Landing::In(c) => (c, false),
                Landing::New => (fresh(), true),
                Landing::NewForArtifact => {
                    let c = fresh();
                    if let Some(mut a) = artifact.clone() {
                        a.conversation = Some(c.clone());
                        let _ = s.save(&a);
                    }
                    (c, true)
                }
            };
            s.turns
                .push(json!({"text": body.text, "conversation": conversation}));
            let answer = c::TurnAnswer {
                conversation,
                created,
                artifact: body.artifact,
            };
            Answer::json(200, &serde_json::to_value(answer).unwrap_or_default())
        }
        Route::NotFound => {
            Answer::refused(Refused::new(404, "not_found", "There's no such route."))
        }
    }
}

struct Server {
    store: Arc<Mutex<Store>>,
    port: u16,
    token: String,
    accept: tokio::task::JoinHandle<()>,
}

static SERVER: Mutex<Option<Server>> = Mutex::new(None);

fn store() -> Result<Arc<Mutex<Store>>> {
    SERVER
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.store.clone())
        .context("the artifacts' API isn't up: a lab.artifacts.up step first")
}

async fn serve(listener: tokio::net::TcpListener, store: Arc<Mutex<Store>>) {
    loop {
        let Ok((mut conn, _)) = listener.accept().await else {
            continue;
        };
        let store = store.clone();
        tokio::spawn(async move {
            let Ok(req) = read_request(&mut conn).await else {
                return;
            };
            let Ok(answer) = tokio::task::spawn_blocking(move || handle(&store, &req)).await else {
                return;
            };
            write_answer(&mut conn, answer).await;
        });
    }
}

async fn up(run: &Run) -> Result<Value> {
    if let Some(s) = SERVER.lock().unwrap().as_ref() {
        return Ok(
            json!({"url": format!("http://127.0.0.1:{}", s.port), "port": s.port, "token": s.token}),
        );
    }
    let home = run.scratch.join("artifacts-lab");
    let dir = home.join("artifacts");
    crate::obid::private_dir(&dir)?;
    let token = hex::encode(rand::random::<[u8; 16]>());
    let store = Arc::new(Mutex::new(Store {
        dir,
        rules: rules_here(),
        token: token.clone(),
        words: Words::default(),
        events: vec![],
        turns: vec![],
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let accept = tokio::spawn(serve(listener, store.clone()));
    *SERVER.lock().unwrap() = Some(Server {
        store,
        port,
        token: token.clone(),
        accept,
    });
    Ok(
        json!({"url": format!("http://127.0.0.1:{port}"), "port": port, "token": token, "home": home}),
    )
}

pub async fn cleanup() -> Value {
    let server = SERVER.lock().unwrap().take();
    match server {
        Some(s) => {
            s.accept.abort();
            let _ = s.accept.await;
            json!({"stopped": true})
        }
        None => json!({"stopped": false}),
    }
}

// ── The nodes' client ────────────────────────────────────────────────────────────────────────

struct Raw {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

impl Raw {
    fn json(&self) -> Value {
        if self.body.is_empty() {
            return Value::Null;
        }
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&self.body).into_owned()))
    }
}

async fn raw_http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    content_type: &str,
) -> Result<Raw> {
    let mut conn = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    conn.write_all(head.as_bytes()).await?;
    conn.write_all(body).await?;
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(120), conn.read_to_end(&mut raw)).await??;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("an answer with no head")?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .context("an answer with no status")?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Ok(Raw {
        status,
        headers,
        body: raw[split + 4..].to_vec(),
    })
}

fn port_of(input: &Value) -> Result<u16> {
    Ok(input["api"]["port"]
        .as_u64()
        .context("api: a lab.artifacts.up step's output")? as u16)
}

async fn send(
    input: &Value,
    method: &str,
    path: &str,
    body: &[u8],
    content_type: &str,
) -> Result<Raw> {
    let headers = super::browserapi::auth(input, method, path, body)?;
    raw_http(port_of(input)?, method, path, &headers, body, content_type).await
}

pub(crate) async fn call(input: &Value) -> Result<Value> {
    let method = input["method"].as_str().unwrap_or("GET").to_uppercase();
    let path = input["path"].as_str().context("path")?;
    let body = match input.get("body") {
        Some(Value::String(s)) => s.clone().into_bytes(),
        Some(v) => v.to_string().into_bytes(),
        None if method == "GET" || method == "DELETE" => vec![],
        None => b"{}".to_vec(),
    };
    let r = send(input, &method, path, &body, "application/json").await?;
    Ok(json!({"status": r.status, "body": r.json()}))
}

fn signed_by_default(input: &Value) -> Value {
    let mut input = input.clone();
    if input.get("as").is_none() {
        input["as"] = json!("device");
    }
    input
}

async fn import(input: &Value) -> Result<Value> {
    let file = input["file"].as_str().context("file")?;
    let bytes = std::fs::read(file).with_context(|| format!("can't read {file}"))?;
    let kind = input["contentType"].as_str().unwrap_or("application/zip");
    let r = send(
        &signed_by_default(input),
        "POST",
        c::PATH_IMPORT,
        &bytes,
        kind,
    )
    .await?;
    Ok(json!({"status": r.status, "body": r.json()}))
}

async fn serve_file(input: &Value) -> Result<Value> {
    let id = input["id"].as_str().context("id")?;
    let version = input["version"].as_u64().context("version")?;
    let path = input["path"].as_str().context("path")?;
    let r = send(
        input,
        "GET",
        &format!("{}/{id}/{version}/{path}", c::PATH_ARTIFACTS),
        b"",
        "application/json",
    )
    .await?;
    let sha = sha_hex(&r.body);
    let same = match input["sameAs"].as_str() {
        Some(local) => json!(std::fs::read(local).map(|b| sha_hex(&b)).ok() == Some(sha.clone())),
        None => Value::Null,
    };
    Ok(json!({
        "status": r.status,
        "type": r.headers.get("content-type"),
        "csp": r.headers.get("content-security-policy"),
        "nosniff": r.headers.get("x-content-type-options").map(|v| v == "nosniff"),
        "bytes": r.body.len(),
        "sha256": sha,
        "same": same,
        "body": if r.status == 200 { Value::Null } else { r.json() },
    }))
}

fn zip_entries(bytes: &[u8]) -> Result<Vec<Value>> {
    let files = unzip(bytes).map_err(|e| anyhow::anyhow!(e.message))?;
    Ok(files
        .iter()
        .map(|(n, b)| json!({"name": n, "sha256": sha_hex(b), "bytes": b.len()}))
        .collect())
}

async fn export(run: &Run, input: &Value) -> Result<Value> {
    let id = input["id"].as_str().context("id")?;
    let version = input["version"].as_u64().context("version")?;
    let what = input["as"].as_str().unwrap_or("zip");
    let r = send(
        &json!({"api": input["api"]}),
        "GET",
        &format!(
            "{}/{id}/{}/{version}?as={what}",
            c::PATH_ARTIFACTS,
            c::VERB_EXPORT
        ),
        b"",
        "application/json",
    )
    .await?;
    if r.status != 200 {
        return Ok(json!({"status": r.status, "body": r.json()}));
    }
    let saved = run
        .scratch
        .join("exports")
        .join(format!("{id}-{version}.{what}"));
    std::fs::create_dir_all(saved.parent().unwrap())?;
    std::fs::write(&saved, &r.body)?;
    let mut out = json!({
        "status": r.status,
        "type": r.headers.get("content-type"),
        "disposition": r.headers.get("content-disposition"),
        "bytes": r.body.len(),
        "sha256": sha_hex(&r.body),
        "saved": saved,
    });
    if what == "zip" {
        out["entries"] = json!(zip_entries(&r.body)?);
    } else {
        let html = String::from_utf8_lossy(&r.body);
        out["outside"] = json!(outside_references(&html));
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

async fn roundtrip(input: &Value) -> Result<Value> {
    let file = input["zip"].as_str().context("zip")?;
    let source = std::fs::read(file).with_context(|| format!("can't read {file}"))?;
    let signed = signed_by_default(input);
    let export_of = |id: String, version: u64| {
        let api = json!({"api": input["api"]});
        async move {
            send(
                &api,
                "GET",
                &format!(
                    "{}/{id}/{}/{version}?as=zip",
                    c::PATH_ARTIFACTS,
                    c::VERB_EXPORT
                ),
                b"",
                "application/json",
            )
            .await
        }
    };
    let first = send(&signed, "POST", c::PATH_IMPORT, &source, "application/zip").await?;
    if first.status != 200 {
        return Ok(json!({"imported": Value::Null, "status": first.status, "body": first.json()}));
    }
    let a = first.json();
    let id = a["id"].as_str().context("the import's id")?.to_string();
    let out = export_of(id.clone(), 1).await?;
    anyhow::ensure!(out.status == 200, "the export answered {}", out.status);
    let src = unzip(&source).map_err(|e| anyhow::anyhow!(e.message))?;
    let got = unzip(&out.body).map_err(|e| anyhow::anyhow!(e.message))?;
    let only_src: Vec<&String> = src.keys().filter(|k| !got.contains_key(*k)).collect();
    let only_got: Vec<&String> = got.keys().filter(|k| !src.contains_key(*k)).collect();
    let changed: Vec<&String> = src
        .iter()
        .filter(|(k, b)| got.get(*k).is_some_and(|g| g != *b))
        .map(|(k, _)| k)
        .collect();
    // Again, from the export: the second export's bytes are the first's.
    let again = send(
        &signed_by_default(input),
        "POST",
        c::PATH_IMPORT,
        &out.body,
        "application/zip",
    )
    .await?;
    let stable = if again.status == 200 {
        let id2 = again.json()["id"].as_str().unwrap_or_default().to_string();
        let out2 = export_of(id2, 1).await?;
        json!(out2.status == 200 && out2.body == out.body)
    } else {
        json!({"status": again.status, "body": again.json()})
    };
    Ok(json!({
        "imported": a,
        "same": only_src.is_empty() && only_got.is_empty() && changed.is_empty(),
        "onlyInSource": only_src,
        "onlyInExport": only_got,
        "changed": changed,
        "stable": stable,
    }))
}

async fn turn(input: &Value) -> Result<Value> {
    let mut body = json!({"text": input["text"].as_str().unwrap_or("Make the title bigger.")});
    if let Some(conv) = input["conversation"].as_str() {
        body["conversation"] = json!(conv);
    }
    if input["artifact"].is_object() {
        body["artifact"] = input["artifact"].clone();
    }
    let body = body.to_string().into_bytes();
    let r = send(
        &signed_by_default(input),
        "POST",
        c::PATH_PLANS,
        &body,
        "application/json",
    )
    .await?;
    Ok(json!({"status": r.status, "body": r.json()}))
}

/// What a stored version holds against its source folder: byte for byte, with each `_blob/<id>`
/// of the source found where the map keeps it.
fn same_as_source(store: &Store, id: &str, version: u32, source: &Path) -> Value {
    let Some(a) = store.get(id) else {
        return Value::Null;
    };
    let (Some(v), Some(kept)) = (
        a.versions.iter().find(|v| v.version == version),
        store.version_files(&a, version),
    ) else {
        return Value::Null;
    };
    let Ok((src, _)) = read_source(source, &Rules::ALL) else {
        return Value::Null;
    };
    let mut expected = Files::new();
    for (path, bytes) in src {
        let path = match path
            .strip_prefix(c::BLOB_PREFIX)
            .and_then(|id| v.blobs.get(id))
        {
            Some(k) => k.clone(),
            None if !source.is_dir() && matches!(ext(&path).as_str(), "html" | "htm") => {
                "index.html".into()
            }
            None => path,
        };
        expected.insert(path, bytes);
    }
    json!(expected == kept)
}

async fn publish(input: &Value) -> Result<Value> {
    let path = PathBuf::from(input["path"].as_str().context("path")?);
    let st = store()?;
    let repeat = input["repeat"].as_u64().unwrap_or(0);
    // `write`: files changed in the source before it is published (a follow-up job's change).
    for (name, text) in input["write"].as_object().into_iter().flatten() {
        std::fs::write(path.join(name), text.as_str().unwrap_or_default())?;
    }
    let once = |artifact: Option<String>| -> Result<(String, u32), Refused> {
        let mut s = st.lock().unwrap();
        let rules = s.rules;
        let (files, folder) = read_source(&path, &rules)?;
        s.keep(Keep {
            files,
            folder,
            title: input["title"].as_str().unwrap_or("Lab artifact").into(),
            kind: input["kind"].as_str().and_then(Kind::parse),
            conversation: input["conversation"].as_str().map(str::to_string),
            job: input["job"].as_str().map(str::to_string),
            artifact,
            origin: Origin::Own,
        })
    };
    let mut result = once(input["artifact"].as_str().map(str::to_string));
    let mut made = 0u64;
    while made < repeat {
        let Ok((id, _)) = &result else { break };
        let id = id.clone();
        std::fs::write(path.join("version.txt"), format!("{}", made + 1))?;
        result = once(Some(id));
        if result.is_ok() {
            made += 1;
        }
    }
    let s = st.lock().unwrap();
    Ok(match result {
        Ok((id, version)) => json!({
            "ok": true,
            "id": id,
            "version": version,
            "repeated": made,
            "artifact": s.get(&id),
            "sameAsSource": if repeat == 0 { same_as_source(&s, &id, version, &path) } else { Value::Null },
        }),
        Err(e) => {
            json!({"ok": false, "repeated": made, "reason": e.reason, "error": e.message, "status": e.status})
        }
    })
}

// ── Fixtures ────────────────────────────────────────────────────────────────────────────────

/// An upload id Claude could give, which looks like a token to obid's name rule.
pub const LONG_BLOB_ID: &str = "Kq7Zt2mW9xR4vB8nL3pF6hJ1sD5gA0cYeU7iO2wT";
pub const FONT_BLOB_ID: &str = "font-7c1e";
pub const VIDEO_BLOB_ID: &str = "clip_3";

/// A PNG of one pixel.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn board(title: &str, body: &str, logic: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n<script src=\"./support.js\"></script>\n<link rel=\"stylesheet\" href=\"lab.css\">\n</head>\n<body>\n<x-dc>\n<helmet>\n<style>\nbody{{margin:0}}\n@font-face{{font-family:Lab;src:url(/_blob/{FONT_BLOB_ID})}}\n</style>\n</helmet>\n{body}\n</x-dc>\n<script type=\"text/x-dc\" data-dc-script data-props='{{\"$preview\":{{\"width\":800,\"height\":600}}}}'>\nclass Component extends DCLogic {{\n{logic}\n}}\n</script>\n</body>\n</html>\n"
    )
}

/// The lab's own Claude Design project: three boards, a design system, fonts, video, and three
/// uploads (`_blob/<id>`, one with a token-like id), as Claude Design writes one.
fn claude_design() -> Files {
    let canvas = json!({
        "v": 3,
        "createdOnFiles": {"v": 1, "at": "2026-11-20T12:00:00Z"},
        "title": "Lab design",
        "launch": {"view": "focused", "file": "Main.dc.html"},
        "pages": [{"id": "design", "name": "Design"}, {"id": "notes_2", "name": "Notes"}],
        "boards": {
            "Main.dc.html": {"x": 0, "y": 0, "w": 800, "h": 600, "title": "Main", "page": "design", "is_interactive": true},
            "Card.dc.html": {"x": 900, "y": 0, "w": 400, "h": 300, "title": "Card", "page": "design"},
            "Media.dc.html": {"x": 0, "y": 700, "w": 800, "h": 600, "title": "Media", "page": "notes_2", "expand": "fill"}
        },
        "order": ["Main.dc.html", "Card.dc.html", "Media.dc.html"],
        "notes": {"idea": {"x": 0, "y": -200, "w": 400, "text": "A lab design: its words are the lab's own.", "fill": "blue", "size": "s", "page": "design"}},
        "designSystems": [{"title": "Lab", "namespace": "lab", "version": "1"}]
    });
    // Claude's own spacing, kept as it is: the store never re-serializes it.
    let canvas = format!(
        "{}\n",
        serde_json::to_string_pretty(&canvas)
            .unwrap()
            .replace("  ", " ")
    );
    let main = board(
        "Main",
        &format!("<div style=\"padding: 32px; font-family: Lab\">\n  <h1>{{{{title}}}}</h1>\n  <img alt=\"\" src=\"/_blob/{LONG_BLOB_ID}\" width=\"64\" height=\"64\">\n  <dc-import name=\"Card\" hint-size=\"400px,300px\"></dc-import>\n  <a href=\"Media.dc.html\">Media</a>\n  <a href=\"https://example.com/more\">More</a>\n  <a href=\"#end\">End</a>\n  <p id=\"end\">The end</p>\n</div>"),
        "  renderVals() { return { title: this.props.title ?? 'Lab design' }; }",
    );
    let card = board(
        "Card",
        "<div class=\"card\" style=\"width: 400px; height: 300px; display: flow-root\">\n  <sc-for list=\"{{items}}\" as=\"item\" hint-placeholder-count=\"2\"><p>{{item.label}}</p></sc-for>\n</div>",
        "  renderVals() { return { items: [{ label: 'One' }, { label: 'Two' }] }; }",
    );
    let media = board(
        "Media",
        &format!("<div style=\"padding: 32px\">\n  <video src=\"/_blob/{VIDEO_BLOB_ID}\" muted></video>\n  <sc-if value=\"{{{{open}}}}\" hint-placeholder-val=\"{{{{ false }}}}\"><p>Open</p></sc-if>\n</div>"),
        "  constructor(props) { super(props); this.state = { open: false }; }\n  renderVals() { return { open: this.state.open }; }",
    );
    let mut files = Files::new();
    let mut put = |p: &str, b: Vec<u8>| {
        files.insert(p.to_string(), b);
    };
    put("canvas.json", canvas.into_bytes());
    put("Main.dc.html", main.into_bytes());
    put("Card.dc.html", card.into_bytes());
    put("Media.dc.html", media.into_bytes());
    put(
        "lab.css",
        b".card{border-radius:14px;background:#161617}\n".to_vec(),
    );
    put(
        "ds/lab/tokens.json",
        br##"{"name":"Lab","color":{"tokens":[{"name":"ink","value":"#ececec"}]}}"##.to_vec(),
    );
    put(
        "ds/lab/components/bundle.css",
        b".lab-button{padding:8px 16px}\n".to_vec(),
    );
    put("fonts/Lab.ttf", [&[0u8, 1, 0, 0][..], b"lab ttf"].concat());
    put("fonts/Lab.otf", b"OTTOlab otf".to_vec());
    put("fonts/Lab.woff2", b"wOF2lab woff2".to_vec());
    put(
        "media/loop.webm",
        [&[0x1au8, 0x45, 0xdf, 0xa3][..], b"lab webm"].concat(),
    );
    put(
        "media/still.avif",
        [&[0u8, 0, 0, 0x1c][..], b"ftypavif", b"lab avif"].concat(),
    );
    put("data.json", br#"{"rows":[1,2,3]}"#.to_vec());
    put("app.js", b"window.lab = 1;\n".to_vec());
    put(
        "widget.jsx",
        b"export const Widget = () => null;\n".to_vec(),
    );
    put(&format!("_blob/{LONG_BLOB_ID}"), PNG.to_vec());
    put(
        &format!("_blob/{FONT_BLOB_ID}"),
        b"wOF2lab upload font".to_vec(),
    );
    put(
        &format!("_blob/{VIDEO_BLOB_ID}"),
        [&[0u8, 0, 0, 0x18][..], b"ftypmp42", b"lab mp4"].concat(),
    );
    files
}

fn spoil(files: &mut Files, how: &str) -> Result<()> {
    let mut canvas: Value = serde_json::from_slice(&files["canvas.json"])?;
    match how {
        "missing-board" => {
            canvas["boards"]["Gone.dc.html"] = json!({"x": 0, "y": 0, "w": 10, "h": 10, "title": "Gone"});
        }
        "bad-page" => canvas["pages"][1]["id"] = json!("not a page id"),
        "bad-order" => canvas["order"] = json!(["Main.dc.html", "Elsewhere.dc.html"]),
        "bad-launch" => canvas["launch"]["file"] = json!("Elsewhere.dc.html"),
        "no-ds" => canvas["designSystems"][0]["namespace"] = json!("absent"),
        "v2" => canvas["v"] = json!(2),
        "no-xdc" => {
            let b = String::from_utf8_lossy(&files["Card.dc.html"]).replace("<x-dc>", "<div>").replace("</x-dc>", "</div>");
            files.insert("Card.dc.html".into(), b.into_bytes());
        }
        "two-scripts" => {
            let b = String::from_utf8_lossy(&files["Card.dc.html"]).replace(
                "</body>",
                "<script type=\"text/x-dc\" data-dc-script>class Component extends DCLogic {}</script>\n</body>",
            );
            files.insert("Card.dc.html".into(), b.into_bytes());
        }
        "big-notes" => {
            canvas["notes"]["big"] = json!({"x": 0, "y": 0, "text": "n".repeat(c::MAX_NOTES_BYTES as usize)});
        }
        "link" => {} // made on disk, below
        other => bail!("break: {other}? missing-board, bad-page, bad-order, bad-launch, no-ds, v2, no-xdc, two-scripts, big-notes or link"),
    }
    files.insert("canvas.json".into(), serde_json::to_vec_pretty(&canvas)?);
    Ok(())
}

static FIXTURES: Mutex<u32> = Mutex::new(0);

fn write_files(dir: &Path, files: &Files) -> Result<()> {
    for (p, b) in files {
        let to = dir.join(p);
        std::fs::create_dir_all(to.parent().unwrap())?;
        std::fs::write(to, b)?;
    }
    Ok(())
}

fn fixture(run: &Run, input: &Value) -> Result<Value> {
    let name = input["name"].as_str().context("name")?;
    let n = {
        let mut n = FIXTURES.lock().unwrap();
        *n += 1;
        *n
    };
    let dir = run.scratch.join("fixtures").join(format!("{name}-{n}"));
    std::fs::create_dir_all(&dir)?;
    let frame = |file: &str, x: u32| json!({"title": file, "file": file, "x": x, "y": 0, "w": 390, "h": 844});
    let page = |title: &str| {
        format!("<!doctype html><html><head><title>{title}</title></head><body><h1>{title}</h1></body></html>\n").into_bytes()
    };
    let mut files = match name {
        "claude-design" => claude_design(),
        "canvas-obid" => Files::from([
            ("canvas.json".into(), serde_json::to_vec(&json!([frame("phone.html", 0), frame("tablet.html", 420)]))?),
            ("phone.html".into(), page("Phone")),
            ("tablet.html".into(), page("Tablet")),
        ]),
        "canvas-swift" => Files::from([
            ("canvas.json".into(), serde_json::to_vec(&json!({"frames": [frame("phone.html", 0), frame("tablet.html", 420)]}))?),
            ("phone.html".into(), page("Phone")),
            ("tablet.html".into(), page("Tablet")),
        ]),
        "page" => Files::from([
            ("index.html".into(), b"<!doctype html><html><head><title>Lab page</title><link rel=\"stylesheet\" href=\"style.css\"><script src=\"app.js\"></script></head><body><h1>Lab page</h1><img alt=\"\" src=\"img/pic.png\"><a href=\"#top\">Top</a></body></html>\n".to_vec()),
            ("style.css".into(), b"body{background:url(img/pic.png)}\n".to_vec()),
            ("app.js".into(), b"document.title += '.';\n".to_vec()),
            ("img/pic.png".into(), PNG.to_vec()),
        ]),
        "document" => Files::from([("notes.md".into(), b"# Lab notes\n\nOne <b>line</b>.\n".to_vec())]),
        // The Oby Artifacts canvas itself (lab-ui/artifacts/design/project), people's names replaced.
        "canvas" => {
            let from = run.root.join("tools/obid-spec/lab-ui/artifacts/design/project");
            read_source(&from, &Rules::ALL)
                .map_err(|e| anyhow::anyhow!("{}: {}", from.display(), e.message))?
                .0
        }
        // Three boards for oby_artifact_check: one fine, one whose renderVals throws, one whose
        // logic doesn't parse.
        "broken-design" => {
            let canvas = json!({"v": 3, "title": "Broken", "boards": {
                "Good.dc.html": {"x": 0, "y": 0, "w": 400, "h": 300, "title": "Good"},
                "Throws.dc.html": {"x": 500, "y": 0, "w": 400, "h": 300, "title": "Throws"},
                "Unparsed.dc.html": {"x": 1000, "y": 0, "w": 400, "h": 300, "title": "Unparsed"}}});
            Files::from([
                ("canvas.json".into(), serde_json::to_vec_pretty(&canvas)?),
                ("Good.dc.html".into(), board("Good", "<p class=\"good\">{{word}}</p>", "  renderVals() { return { word: 'Drawn' }; }").into_bytes()),
                ("Throws.dc.html".into(), board("Throws", "<p>{{word}}</p>", "  renderVals() { throw new Error('the lab board broke'); }").into_bytes()),
                ("Unparsed.dc.html".into(), board("Unparsed", "<p>{{word}}</p>", "  renderVals( { return {}; }").into_bytes()),
            ])
        }
        "sized" => {
            // Sparse files of the sizes asked for: nothing is written but their length.
            for f in input["files"].as_array().context("files")? {
                let p = dir.join(f["path"].as_str().context("files[].path")?);
                std::fs::create_dir_all(p.parent().unwrap())?;
                std::fs::File::create(&p)?.set_len(f["bytes"].as_u64().context("files[].bytes")?)?;
            }
            Files::new()
        }
        "many" => (0..input["count"].as_u64().context("count")?)
            .map(|i| (format!("f/{i:04}.txt"), format!("{i}\n").into_bytes()))
            .chain([("index.html".to_string(), page("Many"))])
            .collect(),
        other => bail!("fixture {other}? claude-design, canvas, broken-design, canvas-obid, canvas-swift, page, document, sized or many"),
    };
    if let Some(how) = input["break"].as_str() {
        spoil(&mut files, how)?;
    }
    write_files(&dir, &files)?;
    if input["break"].as_str() == Some("link") {
        std::os::unix::fs::symlink(dir.join("lab.css"), dir.join("linked.css"))?;
    }
    let mut out = json!({
        "path": dir,
        "files": files.keys().collect::<Vec<_>>(),
        "blobs": {"long": LONG_BLOB_ID, "font": FONT_BLOB_ID, "video": VIDEO_BLOB_ID},
        "blobLocal": dir.join(c::BLOB_PREFIX).join(LONG_BLOB_ID),
    });
    if name == "document" {
        out["path"] = json!(dir.join("notes.md"));
    }
    if input["zip"] == true {
        let entries: Files = files
            .iter()
            .map(|(p, b)| (format!("project/{p}"), b.clone()))
            .collect();
        let zip = dir.with_extension("zip");
        std::fs::write(&zip, zip_of(&entries)?)?;
        out["zip"] = json!(zip);
        out["zipEntries"] = json!(entries.len());
    }
    Ok(out)
}

async fn alive(input: &Value) -> Result<Value> {
    let port = port_of(input)?;
    let answers = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .is_ok_and(|c| c.is_ok());
    Ok(json!({"answers": answers}))
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "lab.artifacts.up" => up(run).await,
        "lab.artifacts.device" => {
            let st = store()?;
            let key = crate::keys::DeviceKey::generate();
            let id = format!("device-{}", hex::encode(rand::random::<[u8; 4]>()));
            st.lock()
                .unwrap()
                .words
                .add(&id, &key.public())
                .map_err(|e| anyhow::anyhow!(e.message))?;
            Ok(json!({"id": id, "key": key.secret_hex(), "publicKey": key.public()}))
        }
        "lab.artifacts.fixture" => fixture(run, input),
        "lab.artifacts.publish" => publish(input).await,
        "lab.artifacts.call" => call(input).await,
        "lab.artifacts.import" => import(input).await,
        "lab.artifacts.serve" => serve_file(input).await,
        "lab.artifacts.export" => export(run, input).await,
        "lab.artifacts.roundtrip" => roundtrip(input).await,
        "lab.artifacts.turn" => turn(input).await,
        "lab.artifacts.events" => {
            let events = store()?.lock().unwrap().events.clone();
            Ok(json!({"count": events.len(), "events": events}))
        }
        "lab.artifacts.alive" => alive(input).await,
        "lab.artifacts.down" => Ok(cleanup().await),
        _ => bail!("{name} isn't a lab.artifacts node"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design() -> Files {
        claude_design()
    }

    #[test]
    fn a_claude_design_is_the_design_kind_and_its_uploads_get_a_map() {
        let mut files = design();
        assert_eq!(kind_of(&files, true, None, &Rules::ALL), Ok(Kind::Design));
        let map = ingest_blobs(&mut files, &Rules::ALL);
        assert_eq!(map.len(), 3);
        let long = &map[LONG_BLOB_ID];
        assert!(
            long.starts_with("assets/") && long.ends_with(".png"),
            "{long}"
        );
        assert!(
            !long.contains(LONG_BLOB_ID),
            "a token-like id gets a hashed name"
        );
        assert_eq!(map[FONT_BLOB_ID], format!("assets/{FONT_BLOB_ID}.woff2"));
        assert_eq!(map[VIDEO_BLOB_ID], format!("assets/{VIDEO_BLOB_ID}.mp4"));
        assert!(files.keys().all(|k| !k.starts_with(c::BLOB_PREFIX)));
        assert!(check_names(files.keys().map(String::as_str)).is_ok());
        // Without the hashed name, the token-like id refuses the whole publish.
        let mut raw = design();
        ingest_blobs(&mut raw, &Rules::broken("names"));
        assert!(check_names(raw.keys().map(String::as_str)).is_err());
    }

    #[test]
    fn each_spoiled_design_is_refused_saying_which() {
        for (how, says) in [
            ("missing-board", "Gone.dc.html"),
            ("bad-page", "not a page id"),
            ("bad-order", "Elsewhere.dc.html"),
            ("bad-launch", "Elsewhere.dc.html"),
            ("no-ds", "absent"),
            ("v2", "v is 3"),
            ("no-xdc", "Card.dc.html has no <x-dc>"),
            ("two-scripts", "Card.dc.html has 2"),
            ("big-notes", "notes are over"),
        ] {
            let mut files = design();
            spoil(&mut files, how).unwrap();
            let e = kind_of(&files, true, None, &Rules::ALL).unwrap_err();
            assert_eq!(e.reason, "bad_design", "{how}");
            assert!(e.message.contains(says), "{how}: {}", e.message);
        }
        // Without the design kind, a Claude canvas is refused as obid refuses it today.
        let e = kind_of(&design(), true, None, &Rules::broken("design")).unwrap_err();
        assert_eq!(e.reason, "bad_canvas");
    }

    #[test]
    fn both_canvas_shapes_publish() {
        let frames = json!([{"title": "P", "file": "p.html", "x": 0, "y": 0, "w": 1, "h": 1}]);
        let files = Files::from([("p.html".to_string(), b"<p>".to_vec())]);
        assert!(check_canvas(&frames, &files, &Rules::ALL).is_ok());
        assert!(check_canvas(&json!({"frames": frames}), &files, &Rules::ALL).is_ok());
        assert!(
            check_canvas(&json!({"frames": frames}), &files, &Rules::broken("frames")).is_err()
        );
    }

    #[test]
    fn limits_refuse_one_over_saying_which() {
        let at = |p: &str, b: u64| vec![(p.to_string(), b)];
        assert!(check_sizes(&at("a.html", c::MAX_TEXT_BYTES), &Rules::ALL).is_ok());
        let e = check_sizes(&at("a.html", c::MAX_TEXT_BYTES + 1), &Rules::ALL).unwrap_err();
        assert_eq!(
            e.message,
            "Not published: a.html is 16 MB and 1 byte, and a text file may be 16 MB at most."
        );
        assert!(check_sizes(&at("a.png", c::MAX_BINARY_BYTES + 1), &Rules::ALL).is_err());
        assert!(check_sizes(&at("a.svg", c::MAX_SVG_BYTES + 1), &Rules::ALL).is_err());
        let many: Vec<(String, u64)> = (0..=c::MAX_FILES).map(|i| (format!("{i}"), 1)).collect();
        assert_eq!(
            check_sizes(&many, &Rules::ALL).unwrap_err().reason,
            "too_many"
        );
        assert!(check_sizes(&many[..c::MAX_FILES], &Rules::ALL).is_ok());
    }

    #[test]
    fn a_blob_resolves_inside_its_version_only() {
        let v = Version {
            version: 1,
            created: String::new(),
            files: vec!["assets/x.png".into(), "Main.dc.html".into()],
            bytes: 0,
            blobs: BTreeMap::from([("abc".into(), "assets/x.png".into())]),
            job: None,
        };
        assert_eq!(
            resolve(&v, "_blob/abc", &Rules::ALL).as_deref(),
            Some("assets/x.png")
        );
        assert_eq!(resolve(&v, "_blob/other", &Rules::ALL), None);
        assert_eq!(resolve(&v, "_blob/../Main.dc.html", &Rules::ALL), None);
        assert_eq!(resolve(&v, "_blob/abc", &Rules::broken("alias")), None);
        assert_eq!(
            resolve(&v, "Main.dc.html", &Rules::ALL).as_deref(),
            Some("Main.dc.html")
        );
    }

    #[test]
    fn the_export_gives_back_what_the_import_took() {
        let source: Files = design()
            .into_iter()
            .map(|(p, b)| (format!("project/{p}"), b))
            .collect();
        let zip = zip_of(&source).unwrap();
        let (mut files, from) = project_of(unzip(&zip).unwrap());
        assert_eq!(from, "claude-design-zip");
        let blobs = ingest_blobs(&mut files, &Rules::ALL);
        let back = export_entries(Kind::Design, &files, &blobs, &Rules::ALL);
        assert_eq!(back, source);
        assert_eq!(zip_of(&back).unwrap(), zip, "the same bytes every time");
        assert_ne!(
            export_entries(Kind::Design, &files, &blobs, &Rules::broken("exact")),
            source
        );
    }

    #[test]
    fn a_zip_with_a_way_out_is_refused() {
        for name in ["../x.html", "/abs.html", "a\\b.html"] {
            let zip = zip_of(&Files::from([(name.to_string(), b"x".to_vec())])).unwrap();
            assert_eq!(unzip(&zip).unwrap_err().reason, "bad_zip", "{name}");
        }
    }

    #[test]
    fn a_page_exports_as_one_file_that_reaches_nothing_outside() {
        let files = Files::from([
            (
                "index.html".to_string(),
                b"<link rel=\"stylesheet\" href=\"s.css\"><img src=\"p.png\"><a href=\"#x\">x</a>"
                    .to_vec(),
            ),
            ("s.css".to_string(), b"body{background:url(p.png)}".to_vec()),
            ("p.png".to_string(), PNG.to_vec()),
        ]);
        let html = export_html(Kind::Html, "t", &files, &BTreeMap::new());
        assert_eq!(outside_references(&html), Vec::<String>::new(), "{html}");
        let design_html = export_html(Kind::Design, "t", &design(), &BTreeMap::new());
        assert_eq!(outside_references(&design_html), Vec::<String>::new());
    }

    #[test]
    fn words_land_in_the_artifacts_conversation() {
        let mut a = Artifact {
            id: "a".into(),
            title: "t".into(),
            kind: Kind::Html,
            conversation: Some("c1".into()),
            job: Some("j1".into()),
            created: String::new(),
            versions: vec![],
            profile: None,
            origin: Origin::Own,
            shares: vec![],
            public: None,
        };
        assert_eq!(
            land_words(Some("c2"), Some(&a), &Rules::ALL),
            Landing::In("c1".into())
        );
        assert_eq!(
            land_words(Some("c2"), Some(&a), &Rules::broken("turn")),
            Landing::In("c2".into())
        );
        assert!(may_version(&a, Some("c1"), Some("j2"), &Rules::ALL).is_ok());
        assert!(may_version(&a, Some("c2"), Some("j3"), &Rules::ALL).is_err());
        assert!(may_version(&a, Some("c1"), Some("j2"), &Rules::broken("crossjob")).is_err());
        a.conversation = None;
        assert_eq!(
            land_words(Some("c2"), Some(&a), &Rules::ALL),
            Landing::NewForArtifact
        );
    }

    #[test]
    fn types_for_fonts_and_video() {
        for (p, t) in [
            ("a.ttf", "font/ttf"),
            ("a.otf", "font/otf"),
            ("a.woff2", "font/woff2"),
            ("a.mp4", "video/mp4"),
            ("a.webm", "video/webm"),
            ("a.avif", "image/avif"),
            ("a.jsx", "application/octet-stream"),
        ] {
            assert_eq!(type_of(p, &Rules::ALL), t, "{p}");
        }
        assert_eq!(
            type_of("a.ttf", &Rules::broken("types")),
            "application/octet-stream"
        );
    }

    #[test]
    fn route_shapes() {
        assert_eq!(route("GET", "/v1/artifacts?job=j"), Route::List);
        assert_eq!(route("POST", "/v1/artifacts/import"), Route::Import);
        assert_eq!(
            route("GET", "/v1/artifacts/a/export/2?as=zip"),
            Route::Export("a".into(), "2".into())
        );
        assert_eq!(
            route("GET", "/v1/artifacts/a/2/export"),
            Route::File("a".into(), "2".into(), "export".into())
        );
        assert_eq!(
            route("GET", "/v1/artifacts/a/1/_blob/x"),
            Route::File("a".into(), "1".into(), "_blob/x".into())
        );
        assert_eq!(route("POST", "/v1/plans"), Route::Plans);
    }
}
