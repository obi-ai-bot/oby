//! JOBSTATE-19 (docs/contracts/job-state.md): who writes a job's state, counted in obid's production
//! code. Each count may only go down: a change that adds a status write, a save of a job in
//! jobs.rs or a branch-carry comparison fails here, and one that removes some lowers its constant.

use std::path::Path;

/// `.status = ` (never `==`) in all of `src/`.
const STATUS_WRITES: usize = 18;
/// `store.save(` in `src/jobs.rs`.
const JOBS_RS_SAVES: usize = 3;
/// `j.branch == ` in all of `src/`: a follow-up carrying its first job's branch.
const BRANCH_CARRIES: usize = 6;

/// `text` without its test modules, by `production_lines`'s rule in scripts/oby-check.sh: from a
/// `#[cfg(test)]` in column 0 directly on a `mod name {` (blank lines between) to the next `}` in
/// column 0. A `#[cfg(test)]` on anything else, or on an out-of-line `mod name;`, counts.
fn production(text: &str) -> String {
    let mut kept = String::new();
    let mut held: Vec<&str> = Vec::new();
    let mut skip = false;
    for line in text.lines() {
        if skip {
            skip = !line.starts_with('}');
            continue;
        }
        if !held.is_empty() {
            if line.trim().is_empty() {
                held.push(line);
                continue;
            }
            if is_mod_open(line) {
                held.clear();
                skip = !line.trim_end().ends_with('}');
                continue;
            }
            for line in held.drain(..) {
                kept.push_str(line);
                kept.push('\n');
            }
        }
        if line.starts_with("#[cfg(test)]") {
            held.push(line);
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    for line in held {
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

/// `mod name {`, `pub mod name {` or `pub(…) mod name {` in column 0.
fn is_mod_open(line: &str) -> bool {
    let rest = match line.strip_prefix("pub") {
        Some(rest) if rest.starts_with('(') => match rest.find(") ") {
            Some(end) => &rest[end + 2..],
            None => return false,
        },
        Some(rest) => match rest.strip_prefix(' ') {
            Some(rest) => rest,
            None => return false,
        },
        None => line,
    };
    let Some(rest) = rest.strip_prefix("mod ") else {
        return false;
    };
    let name_end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    name_end > 0 && rest[name_end..].trim_start().starts_with('{')
}

/// Every `.rs` file under `dir`, as (its path, its production code).
fn sources(dir: &Path, found: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            found.push((path.display().to_string(), production(&text)));
        }
    }
}

#[test]
fn state_owner_counts_shrink_only() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    assert!(
        files.len() > 50,
        "{} files under {}",
        files.len(),
        src.display()
    );
    let count = |pattern: &str, only: Option<&str>| -> usize {
        files
            .iter()
            .filter(|(path, _)| only.is_none_or(|o| Path::new(path) == src.join(o)))
            .map(|(_, code)| code.matches(pattern).count())
            .sum()
    };
    let counts = [
        (
            "status writes (`.status = `)",
            count(".status = ", None),
            STATUS_WRITES,
        ),
        (
            "jobs.rs's saves (`store.save(`)",
            count("store.save(", Some("jobs.rs")),
            JOBS_RS_SAVES,
        ),
        (
            "branch-carry comparisons (`j.branch == `)",
            count("j.branch == ", None),
            BRANCH_CARRIES,
        ),
    ];
    for (what, now, limit) in counts {
        assert!(
            now <= limit,
            "{what}: {now}, past its limit of {limit}: it may only shrink: lower the constant when it drops"
        );
    }
}
