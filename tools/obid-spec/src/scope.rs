//! `scope.json`'s one written form, so that pull requests adding to it touch different lines.
//!
//! Canonical means: two-space JSON as `serde_json` prints it, one entry per line, a newline at the
//! end, every list of strings (scenario ids, route prefixes, workflow files) and every
//! `elsewhere` map's keys in natural order (`X-2` before `X-10`). The order of areas, contracts
//! and triggers is kept as written: it is the order `coverage` prints them in. Sorting changes no
//! meaning: `coverage` reads each list as a set.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::cmp::Ordering;
use std::path::Path;

/// `X-2` before `X-10`: runs of digits compare as numbers, the rest as text.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(&cb) {
        let order = match (number(x), number(y)) {
            (Some(nx), Some(ny)) => nx
                .len()
                .cmp(&ny.len())
                .then_with(|| nx.cmp(ny))
                .then_with(|| x.len().cmp(&y.len())),
            _ => x.cmp(y),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    ca.len().cmp(&cb.len())
}

/// `X-10a` → `X-`, `10`, `a`.
fn chunks(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    for i in 1..=bytes.len() {
        if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[i - 1].is_ascii_digit() {
            out.push(&s[start..i]);
            start = i;
        }
    }
    out
}

/// A run of digits without its leading zeros, or `None` for text.
fn number(chunk: &str) -> Option<&str> {
    chunk
        .bytes()
        .all(|c| c.is_ascii_digit())
        .then(|| chunk.trim_start_matches('0'))
}

/// The same value with its lists of strings and its `elsewhere` maps in natural order.
pub fn canonical(value: &Value) -> Value {
    match value {
        Value::Array(items) => {
            let mut items: Vec<Value> = items.iter().map(canonical).collect();
            if items.iter().all(Value::is_string) {
                items.sort_by(|a, b| natural(a.as_str().unwrap(), b.as_str().unwrap()));
            }
            Value::Array(items)
        }
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> =
                map.iter().map(|(k, v)| (k.clone(), canonical(v))).collect();
            for (k, v) in entries.iter_mut() {
                if k == "elsewhere" {
                    if let Value::Object(inner) = v {
                        let mut sorted: Vec<(String, Value)> =
                            std::mem::take(inner).into_iter().collect();
                        sorted.sort_by(|(a, _), (b, _)| natural(a, b));
                        *inner = sorted.into_iter().collect();
                    }
                }
            }
            Value::Object(entries.into_iter().collect())
        }
        other => other.clone(),
    }
}

/// `scope.json`'s text in its one written form.
pub fn format(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(&canonical(value)).expect("JSON prints");
    text.push('\n');
    text
}

/// The scope, read from `path`, refused unless it is written in its canonical form.
pub fn read(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let value: Value =
        serde_json::from_str(&text).with_context(|| format!("{} isn't JSON", path.display()))?;
    if text != format(&value) {
        bail!(
            "{} isn't in its canonical form: one entry per line, each list of ids and each \
             `elsewhere` map in natural order (X-2 before X-10), so parallel pull requests add \
             different lines. Run `obid-spec fmt-scope` to rewrite it; nothing it means changes.",
            path.display()
        );
    }
    Ok(value)
}

/// Rewrites `path` in its canonical form; says whether it changed.
pub fn rewrite(path: &Path) -> Result<bool> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let value: Value =
        serde_json::from_str(&text).with_context(|| format!("{} isn't JSON", path.display()))?;
    let canonical = format(&value);
    if canonical == text {
        return Ok(false);
    }
    std::fs::write(path, canonical).with_context(|| path.display().to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_sort_by_their_numbers() {
        let mut ids = vec!["X-10", "X-2", "X-1", "AB-3", "A-30", "X-2a", "X-02", "X-3"];
        ids.sort_by(|a, b| natural(a, b));
        assert_eq!(
            ids,
            ["A-30", "AB-3", "X-1", "X-2", "X-2a", "X-02", "X-3", "X-10"]
        );
        let mut routes = vec!["/v1/jobs", "/v1/conversations", "/v1/jobs/{id}"];
        routes.sort_by(|a, b| natural(a, b));
        assert_eq!(routes, ["/v1/conversations", "/v1/jobs", "/v1/jobs/{id}"]);
    }

    #[test]
    fn lists_and_elsewhere_sort_and_the_rest_keeps_its_order() {
        let scope = json!({
            "areas": {
                "z": { "routes": ["/v1/b", "/v1/a"], "contracts": { "b.md": {
                    "obid": ["X-10", "X-2"], "elsewhere": { "X-11": "app", "X-3": "app" } } } },
                "a": { "routes": [] }
            }
        });
        let text = format(&scope);
        assert!(text.find("\"z\"").unwrap() < text.find("\"a\"").unwrap());
        assert!(text.find("\"X-2\"").unwrap() < text.find("\"X-10\"").unwrap());
        assert!(text.find("\"X-3\"").unwrap() < text.find("\"X-11\"").unwrap());
        assert!(text.find("/v1/a").unwrap() < text.find("/v1/b").unwrap());
        assert!(text.contains("\"X-2\",\n"), "one entry per line: {text}");
        assert_eq!(format(&serde_json::from_str(&text).unwrap()), text);
    }

    #[test]
    fn a_scope_out_of_its_form_is_refused_and_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scope.json");
        std::fs::write(&path, r#"{"areas": {"a": {"routes": ["/v1/b", "/v1/a"]}}}"#).unwrap();
        let why = read(&path).unwrap_err().to_string();
        assert!(why.contains("obid-spec fmt-scope"), "{why}");
        assert!(rewrite(&path).unwrap());
        assert_eq!(
            read(&path).unwrap(),
            json!({"areas": {"a": {"routes": ["/v1/a", "/v1/b"]}}})
        );
        assert!(!rewrite(&path).unwrap());
    }

    /// The suite's own scope.json is in its form: a hand edit that isn't fails here and in
    /// `coverage`, with the command that fixes it.
    #[test]
    fn the_suites_scope_is_canonical() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scope.json");
        read(&path).unwrap();
    }
}
