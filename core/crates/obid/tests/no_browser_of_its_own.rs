//! obid carries no browser of its own (BROWSER-23, the window's browser, docs/plans/browser-t3.md):
//! every agent tab is a `<webview>` in Oby's window, and obid neither downloads, pins, installs nor
//! runs a Chrome or agent-browser. The owner's word: "NO FALLBACK REMOVE CHROME ENTIRELY". A
//! headless Chrome may run only in obid-spec's stand-in for the window, which is test tooling.
//! This reads obid's sources and fails if the old driver or installer, or any pin, download host or
//! launch flag of theirs, comes back.

use std::path::{Path, PathBuf};

/// What only a browser of obid's own would need: its pinned downloads and how it was launched.
const NEEDLES: &[&str] = &[
    "chrome-for-testing",
    "chrome-headless-shell-",
    "agent-browser-0.",
    "--remote-debugging",
    "AGENT_BROWSER_",
    "github.com/vercel-labs/agent-browser",
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "toml") {
            out.push(path);
        }
    }
}

#[test]
fn browser_23_obid_carries_no_browser_of_its_own() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for gone in ["src/browser/driver", "src/browser/install"] {
        assert!(
            !crate_dir.join(gone).exists(),
            "{gone} is back: obid runs no browser of its own (docs/plans/browser-t3.md)"
        );
    }
    let mut files = vec![crate_dir.join("Cargo.toml")];
    sources(&crate_dir.join("src"), &mut files);
    assert!(
        files.len() > 100,
        "obid's sources weren't found: {}",
        files.len()
    );
    let mut found = vec![];
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        for needle in NEEDLES {
            if text.contains(needle) {
                found.push(format!("{} has {needle:?}", file.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "obid carries a browser of its own again (docs/plans/browser-t3.md):\n{}",
        found.join("\n")
    );
}
