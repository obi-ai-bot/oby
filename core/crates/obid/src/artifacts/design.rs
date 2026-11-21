//! A folder's `canvas.json`, checked as it is (docs/contracts/artifacts.md, ARTIFACTS-1 and 2):
//! obid's `canvas` (a list of frames, or Swift's `{"frames": […]}`, so obid, Swift and Kotlin read
//! the same files) and Claude Design's `design` (its `canvas.json` object and `.dc.html` boards).
//! Nothing is derived into either: a design is kept byte for byte, so its export gives Claude back
//! what it sent. It reads the staged copy only; it never writes, and keeps nothing.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::extension;

/// How big a design's `notes` may be, as JSON.
const MAX_NOTES_BYTES: usize = 1024 * 1024;

/// Every field must be there to pass; only `file` is read.
#[derive(Deserialize)]
#[allow(dead_code)]
struct Frame {
    title: String,
    file: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Whether a `canvas.json` is Claude Design's object rather than obid's or Swift's frames.
pub(super) fn is_design(canvas: &Value) -> bool {
    canvas.is_object() && (canvas.get("boards").is_some() || canvas.get("v").is_some())
}

/// obid's canvas: a list of frames, each an HTML file of the folder, or Swift's `{"frames": […]}`.
pub(super) fn check_canvas(canvas: &Value, files: &[String]) -> Result<(), String> {
    let shape = || {
        "canvas.json is a list of frames, each {\"title\", \"file\", \"x\", \"y\", \"w\", \"h\"}."
            .to_owned()
    };
    let list = match canvas {
        Value::Array(_) => canvas.clone(),
        Value::Object(o) if o.get("frames").is_some_and(Value::is_array) => o["frames"].clone(),
        _ => return Err(shape()),
    };
    let frames: Vec<Frame> = serde_json::from_value(list).map_err(|_| shape())?;
    if frames.is_empty() {
        return Err("canvas.json has no frame.".into());
    }
    match frames
        .iter()
        .find(|f| extension(&f.file) != "html" || !files.contains(&f.file))
    {
        Some(frame) => Err(format!(
            "canvas.json's frame {:?} isn't an HTML file of the canvas.",
            frame.file
        )),
        None => Ok(()),
    }
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

/// A plain name: a design system's folder, or an upload's own name.
pub(super) fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Claude Design's `canvas.json` and its boards (`staging` holds the files): `v` is 3; every board
/// is a `.dc.html` file of the folder holding `<x-dc>` and one logic script; `order` and `launch`
/// name known boards or pages; a page's id is 1 to 40 letters, digits, `_` or `-`; `notes` under
/// 1 MB; each design system's `ds/<namespace>/` folder is there. Each refusal names what is wrong.
pub(super) fn check_design(canvas: &Value, files: &[String], staging: &Path) -> Result<(), String> {
    if canvas["v"] != json!(3) {
        return Err(format!(
            "canvas.json's v is 3 in Claude Design's format, and here it is {}.",
            canvas["v"]
        ));
    }
    let boards = canvas["boards"]
        .as_object()
        .filter(|b| !b.is_empty())
        .ok_or("canvas.json has no board.")?;
    for name in boards.keys() {
        if !name.ends_with(".dc.html") {
            return Err(format!(
                "canvas.json's board {name:?} isn't a .dc.html file."
            ));
        }
        if !files.contains(name) {
            return Err(format!(
                "canvas.json's board {name:?} isn't a file of the design."
            ));
        }
        let bytes = std::fs::read(staging.join(name))
            .map_err(|e| format!("{name} can't be read ({e})."))?;
        let text = String::from_utf8_lossy(&bytes);
        if !text.contains("<x-dc>") {
            return Err(format!("{name} has no <x-dc>."));
        }
        let scripts = dc_scripts(&text);
        if scripts != 1 {
            return Err(format!(
                "{name} has {scripts} <script type=\"text/x-dc\" data-dc-script> blocks, and a board has one."
            ));
        }
    }
    if let Some(order) = canvas.get("order").filter(|o| !o.is_null()) {
        let order = order
            .as_array()
            .ok_or("canvas.json's order is a list of boards.")?;
        if let Some(stray) = order
            .iter()
            .find(|b| !b.as_str().is_some_and(|b| boards.contains_key(b)))
        {
            return Err(format!(
                "canvas.json's order names {stray}, which isn't one of its boards."
            ));
        }
    }
    static PAGE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let page_id =
        PAGE.get_or_init(|| regex::Regex::new(r"^[A-Za-z0-9_-]{1,40}$").expect("it compiles"));
    let mut pages = Vec::new();
    for page in canvas["pages"].as_array().into_iter().flatten() {
        let id = page["id"].as_str().unwrap_or_default();
        if !page_id.is_match(id) {
            let shown = page["id"]
                .as_str()
                .map_or(page["id"].to_string(), str::to_owned);
            return Err(format!(
                "canvas.json's page id {shown:?} isn't 1 to 40 letters, digits, _ or -."
            ));
        }
        pages.push(id);
    }
    if let Some(launch) = canvas.get("launch").filter(|l| !l.is_null()) {
        let stray = launch["file"]
            .as_str()
            .filter(|f| !boards.contains_key(*f))
            .or(launch["page"].as_str().filter(|p| !pages.contains(p)));
        if let Some(stray) = stray {
            return Err(format!(
                "canvas.json's launch names {stray:?}, which isn't one of its pages or boards."
            ));
        }
    }
    if canvas
        .get("notes")
        .is_some_and(|n| n.to_string().len() > MAX_NOTES_BYTES)
    {
        return Err("canvas.json's notes are over 1 MB.".into());
    }
    for system in canvas["designSystems"].as_array().into_iter().flatten() {
        let ns = system["namespace"].as_str().unwrap_or_default();
        let folder = format!("ds/{ns}/");
        if !plain_name(ns) || !files.iter().any(|f| f.starts_with(&folder)) {
            return Err(format!(
                "canvas.json's design system {ns:?} has no ds/{ns}/ folder."
            ));
        }
    }
    Ok(())
}
