//! A design's uploads (docs/contracts/artifacts.md, ARTIFACTS-3 and 5): Claude Design names a
//! picture or a font it was given `/_blob/<id>`; obid keeps each under `assets/<name>.<ext>` (its
//! type from its bytes) with the version's map `blobs: {id: kept path}`, and serves it back at
//! `{version}/_blob/<id>` inside that version only. The boards' bytes are never rewritten, so an
//! export gives Claude back what it sent. An id that isn't a plain name, or looks like a token
//! (obid's `long_run`), is kept under the first 16 hex of its SHA-256 instead of refused. It moves
//! files inside the staged copy only.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use sha2::Digest;

use super::Version;
use super::design::plain_name;

/// Where a version keeps its uploads, and how Claude names them.
const ASSETS: &str = "assets";
pub(super) const BLOB_PREFIX: &str = "_blob/";

/// What an upload is, from its first bytes (Claude's names have no extension).
fn sniff(b: &[u8]) -> &'static str {
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
        let head = String::from_utf8_lossy(b);
        let t = head.trim_start();
        if t.starts_with("<svg") || (t.starts_with("<?xml") && head.contains("<svg")) {
            "svg"
        } else {
            "bin"
        }
    }
}

fn hashed(id: &str) -> String {
    hex::encode(sha2::Sha256::digest(id.as_bytes()))[..16].to_owned()
}

/// Moves each `_blob/<id>` of `staging` to `assets/<name>.<ext>`, `files` (sorted) and the map
/// following.
pub(super) fn ingest(
    staging: &Path,
    files: &mut Vec<String>,
) -> Result<BTreeMap<String, String>, String> {
    let ids: Vec<String> = files
        .iter()
        .filter_map(|f| f.strip_prefix(BLOB_PREFIX))
        .filter(|id| !id.is_empty() && !id.contains('/'))
        .map(str::to_owned)
        .collect();
    let mut map = BTreeMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    crate::store::make_private_dir(&staging.join(ASSETS))
        .map_err(|e| format!("obid couldn't keep its uploads ({e})."))?;
    for id in ids {
        let from = format!("{BLOB_PREFIX}{id}");
        let mut head = Vec::new();
        std::fs::File::open(staging.join(&from))
            .and_then(|f| f.take(512).read_to_end(&mut head))
            .map_err(|e| format!("{from} can't be read ({e})."))?;
        let ext = sniff(&head);
        let name = if plain_name(&id) && !super::long_run(&id) {
            id.clone()
        } else {
            hashed(&id)
        };
        let mut kept = format!("{ASSETS}/{name}.{ext}");
        if files.contains(&kept) {
            kept = format!("{ASSETS}/blob-{}.{ext}", hashed(&id));
        }
        std::fs::rename(staging.join(&from), staging.join(&kept))
            .map_err(|e| format!("obid couldn't keep {from} ({e})."))?;
        files.retain(|f| *f != from);
        files.push(kept.clone());
        map.insert(id, kept);
    }
    let _ = std::fs::remove_dir(staging.join(BLOB_PREFIX.trim_end_matches('/')));
    files.sort();
    Ok(map)
}

/// The file of `version` a request for `path` means: one of its files, or a `_blob/<id>` its map
/// holds. Anything else (another version's upload, an id it hasn't) is nothing.
pub(super) fn resolve(version: &Version, path: &str) -> Option<String> {
    if let Some(id) = path.strip_prefix(BLOB_PREFIX) {
        return version.blobs.get(id).cloned();
    }
    version
        .files
        .iter()
        .any(|f| f == path)
        .then(|| path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_upload_is_known_by_its_bytes() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), "png");
        assert_eq!(sniff(b"wOF2rest"), "woff2");
        assert_eq!(sniff(&[0, 1, 0, 0, 9]), "ttf");
        assert_eq!(sniff(b"\0\0\0\x18ftypmp42"), "mp4");
        assert_eq!(sniff(b"\0\0\0\x1cftypavif"), "avif");
        assert_eq!(sniff(b"  <svg xmlns=''>"), "svg");
        assert_eq!(sniff(b"plain"), "bin");
    }
}
