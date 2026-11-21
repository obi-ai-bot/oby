//! How much an artifact holds (docs/contracts/artifacts.md, ARTIFACTS-8): Claude's limits, the
//! owner's decision of 2026-11-20, so whatever Claude Design makes fits as it is. A text file 16 MB,
//! a binary file (an image, a font, a video, a PDF) 15 MB, an SVG 2 MB; a version 512 files and
//! 256 MB; an artifact 50 versions; and no cap on the store, only on what one job or conversation
//! holds. Each refusal names what is over, and by how much. It decides only; the copy counts.

use super::extension;

const MB: u64 = 1024 * 1024;
/// The most a text file holds (a page, a script, a style sheet, JSON, Markdown).
const MAX_TEXT_BYTES: u64 = 16 * MB;
/// The most a binary file holds: an image, a font, a video, a PDF, an upload.
const MAX_BINARY_BYTES: u64 = 15 * MB;
/// The most an SVG holds: text a browser draws, so less.
const MAX_SVG_BYTES: u64 = 2 * MB;
/// The most one version holds, in bytes.
pub const MAX_BYTES: u64 = 256 * MB;
/// The most files one version keeps (hidden names, left out, don't count).
pub const MAX_FILES: usize = 512;
/// The most names obid reads in one version's folders, hidden ones too, before it stops: a guard
/// on the read, past which nothing is published.
pub const MAX_ENTRIES: usize = 2000;
/// The most bytes one job's artifacts hold, every version of each counted, and as much for a
/// conversation's own.
pub const MAX_JOB_BYTES: u64 = 4096 * MB;

fn is_text(name: &str) -> bool {
    matches!(
        extension(name).as_str(),
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
            | "xml"
    )
}

/// The most the file `name` may hold, and what it is, said.
pub(super) fn most_for(name: &str) -> (u64, &'static str) {
    if extension(name) == "svg" {
        (MAX_SVG_BYTES, "an SVG")
    } else if is_text(name) {
        (MAX_TEXT_BYTES, "a text file")
    } else {
        (
            MAX_BINARY_BYTES,
            "a binary file (an image, a font, a video, a PDF)",
        )
    }
}

/// `bytes` as said: whole MB, then what is left over.
pub(super) fn size_text(bytes: u64) -> String {
    let (mb, rest) = (bytes / MB, bytes % MB);
    let unit = |n: u64| if n == 1 { "byte" } else { "bytes" };
    match (mb, rest) {
        (0, r) => format!("{r} {}", unit(r)),
        (m, 0) => format!("{m} MB"),
        (m, r) => format!("{m} MB and {r} {}", unit(r)),
    }
}

/// Why the file `name`, of `bytes`, isn't kept; none when it fits.
pub(super) fn file_too_big(name: &str, bytes: u64) -> Option<String> {
    let (most, what) = most_for(name);
    (bytes > most).then(|| {
        format!(
            "{name} is {}, and {what} may be {} at most.",
            size_text(bytes),
            size_text(most)
        )
    })
}

/// Why a version this big isn't kept.
pub(super) fn version_too_big() -> String {
    format!(
        "this version is more than {}, the most a version may be.",
        size_text(MAX_BYTES)
    )
}

/// Why a version of `files` files isn't kept; none when it fits.
pub(super) fn too_many(files: usize) -> Option<String> {
    (files > MAX_FILES)
        .then(|| format!("a version holds at most {MAX_FILES} files, and this holds {files}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_file_has_the_limit_of_its_kind() {
        assert!(file_too_big("a.html", MAX_TEXT_BYTES).is_none());
        assert_eq!(
            file_too_big("a.html", MAX_TEXT_BYTES + 1).unwrap(),
            "a.html is 16 MB and 1 byte, and a text file may be 16 MB at most."
        );
        assert!(file_too_big("a.png", MAX_BINARY_BYTES).is_none());
        assert!(file_too_big("a.png", MAX_BINARY_BYTES + 1).is_some());
        assert!(file_too_big("_blob/abc", MAX_BINARY_BYTES + 1).is_some());
        assert!(file_too_big("a.svg", MAX_SVG_BYTES + 1).is_some());
        assert!(too_many(MAX_FILES).is_none());
        assert_eq!(
            too_many(MAX_FILES + 1).unwrap(),
            "a version holds at most 512 files, and this holds 513."
        );
        assert_eq!(size_text(2), "2 bytes");
    }
}
