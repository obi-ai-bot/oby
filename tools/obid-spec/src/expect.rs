//! What a step expects of an answer or a value, and every way it fell short.
//!
//! ```json
//! {"status": 422,
//!  "body": {"state": "joined"},
//!  "paths": {"card.version": "0.3.0+card-runtime", "error": {"$contains": "owner"},
//!            "items": {"$len": 2}, "delegation.jwt": {"$exists": false}},
//!  "text": {"$contains": "…"}}
//! ```
//!
//! `body` is a subset: every key it names must be there with a matching value (objects compared
//! the same way, arrays by length and element by element). A value in `paths` or `body` that is an
//! object with one `$` key is a matcher: `$exists`, `$ne`, `$contains` (a substring, or an array
//! element that matches), `$len`, `$type`, `$matches` (a regular expression), `$any` (an array
//! element matching a subset), `$none` (no array element matching a subset), `$oneOf`, `$count` (`[subset, n]`: exactly n elements match), `$not`, `$sorted` (`true`: an array of strings in ascending order, whatever it holds).

use serde_json::Value;

pub fn check_status(expected: &Value, status: u16) -> Option<String> {
    let ok = match expected {
        Value::Number(n) => n.as_u64() == Some(status as u64),
        Value::Array(list) => list.iter().any(|n| n.as_u64() == Some(status as u64)),
        Value::Null => true,
        _ => false,
    };
    (!ok).then(|| format!("status {status}, expected {expected}"))
}

/// Every top-level key an `expect` may have, whichever node reads it: an answer's `status`, the
/// subset `body`, `paths` and `text` (this file), a home file's `exists`, `mode` and `json`.
pub const KEYS: &[&str] = &["status", "body", "paths", "text", "exists", "mode", "json"];

/// `expect` as the node `node` reads it: an object whose top-level keys are among `judged`, the
/// keys that node judges. Any other key would be skipped in silence and the step pass asserting
/// nothing of it, so it's an error naming the key, the node and what it judges.
pub fn judged_by(node: &str, expect: &Value, judged: &[&str]) -> Result<(), String> {
    let Some(map) = expect.as_object() else {
        return Err(format!(
            "expect must be an object, not {expect}: {node} judges {}",
            list(judged)
        ));
    };
    if judged.is_empty() {
        return Err(format!(
            "{node} takes no expect, so this one would assert nothing: check its answer in a check step (a node that does read one names its keys in nodes::judges)"
        ));
    }
    let unknown: Vec<&str> = map
        .keys()
        .map(String::as_str)
        .filter(|k| !judged.contains(k))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    let hint = unknown
        .iter()
        .map(|k| format!("\"paths\": {{\"{k}\": …}}"))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "expect has {} {} that {node} doesn't judge, so it would assert nothing: {node} judges {}; to assert a field of its answer, write {hint}",
        if unknown.len() == 1 { "a key" } else { "keys" },
        list(&unknown),
        list(judged),
    ))
}

fn list(keys: &[&str]) -> String {
    keys.iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every way `actual` falls short of `expect` (`body`, `paths`, `text`); none when it doesn't. A
/// key no node reads (not in [`KEYS`]) is a failure too, so a nested expectation (`until`'s) can't
/// skip one in silence either.
pub fn check(expect: &Value, actual: &Value, text: &str) -> Vec<String> {
    let mut failures = vec![];
    if let Some(map) = expect.as_object() {
        for k in map.keys().filter(|k| !KEYS.contains(&k.as_str())) {
            failures.push(format!(
                "expect has a key `{k}` nothing reads (it reads {}): write \"paths\": {{\"{k}\": …}}",
                list(KEYS)
            ));
        }
    } else if !expect.is_null() {
        failures.push(format!("expect must be an object, not {expect}"));
    }
    if let Some(body) = expect.get("body") {
        matches("body", body, actual, &mut failures);
    }
    if let Some(paths) = expect.get("paths").and_then(Value::as_object) {
        for (path, want) in paths {
            let got = at(actual, path);
            matches(path, want, got.unwrap_or(&Value::Null), &mut failures);
            if got.is_none() && !is_absent_ok(want) {
                failures.push(format!("{path}: missing"));
                failures.dedup();
            }
        }
    }
    if let Some(want) = expect.get("text") {
        matches(
            "text",
            want,
            &Value::String(text.to_string()),
            &mut failures,
        );
    }
    failures
}

fn is_absent_ok(want: &Value) -> bool {
    matcher(want).is_some_and(|(op, arg)| {
        (op == "$exists" && arg == &Value::Bool(false)) || op == "$ne" || op == "$oneOf"
    }) || want.is_null()
}

/// The value at a dotted path (`a.b.0.c`); `$` alone is the whole value.
pub fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path == "$" || path.is_empty() {
        return Some(value);
    }
    let mut here = value;
    for part in path.split('.') {
        here = match here {
            Value::Object(map) => map.get(part)?,
            Value::Array(list) => list.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(here)
}

fn matcher(want: &Value) -> Option<(&str, &Value)> {
    let map = want.as_object()?;
    if map.len() != 1 {
        return None;
    }
    let (k, v) = map.iter().next()?;
    k.starts_with('$').then_some((k.as_str(), v))
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 300 {
        format!("{}…", s.chars().take(300).collect::<String>())
    } else {
        s
    }
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub fn matches(path: &str, want: &Value, got: &Value, failures: &mut Vec<String>) {
    if let Some((op, arg)) = matcher(want) {
        let ok = match op {
            "$exists" => (got != &Value::Null) == arg.as_bool().unwrap_or(true),
            "$ne" => got != arg,
            "$type" => arg.as_str() == Some(type_of(got)),
            "$len" => {
                let len = match got {
                    Value::Array(a) => Some(a.len()),
                    Value::String(s) => Some(s.chars().count()),
                    Value::Object(m) => Some(m.len()),
                    _ => None,
                };
                len.map(|l| l as u64) == arg.as_u64()
            }
            "$contains" => match (got, arg) {
                (Value::String(s), Value::String(needle)) => s.contains(needle.as_str()),
                (Value::Array(items), want) => items.iter().any(|i| {
                    let mut f = vec![];
                    matches(path, want, i, &mut f);
                    f.is_empty()
                }),
                _ => false,
            },
            "$any" | "$none" => {
                let any = got.as_array().is_some_and(|items| {
                    items.iter().any(|i| {
                        let mut f = vec![];
                        matches(path, arg, i, &mut f);
                        f.is_empty()
                    })
                });
                (op == "$any") == any
            }
            "$oneOf" => arg.as_array().is_some_and(|a| a.contains(got)),
            // An add-only list (`capabilities`) proven in order without copying it: `{"$sorted": true}`.
            "$sorted" => {
                let strings: Option<Vec<&str>> = got
                    .as_array()
                    .and_then(|a| a.iter().map(Value::as_str).collect());
                let sorted = strings.is_some_and(|s| s.windows(2).all(|w| w[0] <= w[1]));
                sorted == arg.as_bool().unwrap_or(true)
            }
            // How many array elements match a subset: `{"$count": [{…}, 1]}`.
            "$count" => match (got.as_array(), arg.as_array().map(Vec::as_slice)) {
                (Some(items), Some([want, n])) => {
                    let found = items
                        .iter()
                        .filter(|i| {
                            let mut f = vec![];
                            matches(path, want, i, &mut f);
                            f.is_empty()
                        })
                        .count();
                    n.as_u64() == Some(found as u64)
                }
                _ => false,
            },
            // Passes when the matcher inside fails (a text that must not hold a secret).
            "$not" => {
                let mut f = vec![];
                matches(path, arg, got, &mut f);
                !f.is_empty()
            }
            "$matches" => match (got.as_str(), arg.as_str()) {
                (Some(s), Some(pattern)) => regex::Regex::new(pattern).is_ok_and(|r| r.is_match(s)),
                _ => false,
            },
            other => {
                failures.push(format!("{path}: unknown matcher {other}"));
                return;
            }
        };
        if !ok {
            failures.push(format!("{path}: {} is not {op} {}", short(got), short(arg)));
        }
        return;
    }
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => {
            for (k, v) in w {
                let sub = format!("{path}.{k}");
                match g.get(k) {
                    Some(gv) => matches(&sub, v, gv, failures),
                    None if is_absent_ok(v) => matches(&sub, v, &Value::Null, failures),
                    None => failures.push(format!("{sub}: missing")),
                }
            }
        }
        (Value::Array(w), Value::Array(g)) => {
            if w.len() != g.len() {
                failures.push(format!(
                    "{path}: {} items, expected {} ({})",
                    g.len(),
                    w.len(),
                    short(got)
                ));
                return;
            }
            for (i, (wv, gv)) in w.iter().zip(g).enumerate() {
                matches(&format!("{path}.{i}"), wv, gv, failures);
            }
        }
        _ if want == got => {}
        _ => failures.push(format!("{path}: {} expected {}", short(got), short(want))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn obidspec_6_a_key_no_node_reads_is_a_failure_never_skipped() {
        let got = json!({"count": 1});
        let f = check(&json!({"count": 0}), &got, "");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].contains("`count`") && f[0].contains("paths"), "{f:?}");
        assert!(!check(&json!([0]), &got, "").is_empty());
        assert!(check(&json!({"paths": {"count": 1}}), &got, "").is_empty());
        assert!(check(&Value::Null, &got, "").is_empty());
    }

    #[test]
    fn obidspec_6_a_node_judges_only_its_own_keys() {
        let value = ["body", "paths", "text"];
        assert!(judged_by("lab.x", &json!({"paths": {"status": 200}}), &value).is_ok());
        let e = judged_by("lab.x", &json!({"status": 200, "paths": {}}), &value).unwrap_err();
        assert!(
            e.contains("`status`") && e.contains("lab.x") && e.contains("\"paths\": {\"status\""),
            "{e}"
        );
        assert!(judged_by("t3.fs", &json!({"paths": {}}), &[])
            .unwrap_err()
            .contains("takes no expect"));
        assert!(judged_by("call", &json!("200"), &value).is_err());
    }

    #[test]
    fn a_subset_matches_and_a_wrong_value_says_where() {
        let got = json!({"state": "joined", "card": {"version": "0.3.0"}, "list": [1, 2]});
        assert!(check(&json!({"body": {"state": "joined"}}), &got, "").is_empty());
        let f = check(&json!({"body": {"card": {"version": "0.2.0"}}}), &got, "");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].starts_with("body.card.version"), "{f:?}");
        assert!(!check(&json!({"body": {"list": [1]}}), &got, "").is_empty());
    }

    #[test]
    fn matchers() {
        let got = json!({"a": "hello world", "n": null, "items": [{"k": 1}, {"k": 2}], "id": "d-0123456789ab"});
        let ok = json!({"paths": {
            "a": {"$contains": "world"}, "n": {"$exists": false}, "missing": {"$exists": false},
            "items": {"$len": 2}, "items.1.k": 2, "id": {"$matches": "^d-[0-9a-f]{12}$"},
            "$": {"$type": "object"}}});
        assert_eq!(check(&ok, &got, ""), Vec::<String>::new());
        assert_eq!(
            check(&json!({"body": {"items": {"$any": {"k": 2}}}}), &got, ""),
            Vec::<String>::new()
        );
        assert_eq!(
            check(&json!({"body": {"items": {"$none": {"k": 3}}}}), &got, ""),
            Vec::<String>::new()
        );
        assert!(!check(&json!({"paths": {"absent": 1}}), &got, "").is_empty());
        assert!(check(
            &json!({"body": {"items": {"$count": [{"k": 1}, 1]}}}),
            &got,
            ""
        )
        .is_empty());
        assert!(!check(
            &json!({"body": {"items": {"$count": [{"k": 1}, 2]}}}),
            &got,
            ""
        )
        .is_empty());
        assert!(!check(&json!({"paths": {"a": {"$contains": "nope"}}}), &got, "").is_empty());
        assert!(!check(&json!({"text": {"$contains": "x"}}), &got, "y").is_empty());
    }

    #[test]
    fn sorted_passes_any_ascending_list_and_fails_one_out_of_order() {
        let sorted = json!({"body": {"caps": {"$sorted": true}}});
        for ok in [
            json!([]),
            json!(["a-v1", "b-v1"]),
            json!(["a-v1", "b-v1", "c-v1"]),
        ] {
            assert!(check(&sorted, &json!({"caps": ok}), "").is_empty(), "{ok}");
        }
        for bad in [
            json!(["b-v1", "a-v1"]),
            json!(["a-v1", 1]),
            json!("a-v1"),
            json!(null),
        ] {
            assert!(
                !check(&sorted, &json!({"caps": bad}), "").is_empty(),
                "{bad}"
            );
        }
    }

    #[test]
    fn status_one_or_several() {
        assert!(check_status(&json!(200), 200).is_none());
        assert!(check_status(&json!([200, 202]), 202).is_none());
        assert!(check_status(&json!(409), 200).is_some());
    }
}

/// An answer changed at one place an expectation looks at: what proves that assertion could
/// fail (docs/contracts/obid-spec.md, OBIDSPEC-6). One mutant per leaf of `body`, `paths` and
/// `text`, and one for `status`, each the very answer obid gave with that one place made wrong.
#[derive(Clone, Debug)]
pub struct Mutant {
    pub what: String,
    pub status: u16,
    pub body: Value,
    pub text: String,
}

const MUTATED: &str = "obid-spec: mutated";

fn sentinel() -> Value {
    serde_json::json!({ "obid-spec": "mutated" })
}

/// A value that matches `want`, for a mutant that must hold what a `$none` refuses: literals as
/// they are, `$contains` its text, `$oneOf` its first, other matchers a string.
fn materialize(want: &Value) -> Value {
    if let Some((op, arg)) = matcher(want) {
        return match op {
            "$contains" | "$matches" if arg.is_string() => arg.clone(),
            "$oneOf" => arg
                .as_array()
                .and_then(|a| a.first())
                .cloned()
                .unwrap_or(Value::Null),
            "$any" => Value::Array(vec![materialize(arg)]),
            "$type" if arg == "array" => Value::Array(vec![]),
            "$type" if arg == "object" => serde_json::json!({}),
            "$type" if arg == "number" => serde_json::json!(1),
            "$type" if arg == "boolean" => Value::Bool(true),
            _ => Value::String(MUTATED.into()),
        };
    }
    match want {
        Value::Object(m) => {
            Value::Object(m.iter().map(|(k, v)| (k.clone(), materialize(v))).collect())
        }
        other => other.clone(),
    }
}

/// A value the leaf `want` refuses.
fn refused_by(want: &Value) -> Value {
    if let Some((op, arg)) = matcher(want) {
        return match op {
            "$exists" if arg == &Value::Bool(true) => Value::Null,
            "$ne" => arg.clone(),
            "$type" if arg == "object" => Value::String(MUTATED.into()),
            "$count" => match arg.as_array().map(Vec::as_slice) {
                Some([w, n]) if n.as_u64() == Some(0) => Value::Array(vec![w.clone()]),
                _ => Value::Array(vec![]),
            },
            "$none" => Value::Array(vec![materialize(arg)]),
            "$any" => Value::Array(vec![]),
            // One more than it wants, whatever it counts.
            "$len" => Value::Array(vec![Value::Null; arg.as_u64().unwrap_or(0) as usize + 1]),
            "$not" => match matcher(arg) {
                Some(("$contains", Value::String(s))) => Value::String(s.clone()),
                Some(_) => sentinel(),
                None => arg.clone(),
            },
            _ => sentinel(),
        };
    }
    match want {
        Value::Object(_) => Value::String(MUTATED.into()),
        _ => sentinel(),
    }
}

fn is_leaf(want: &Value) -> bool {
    matcher(want).is_some() || !want.is_object() || want.as_object().is_some_and(|m| m.is_empty())
}

fn body_leaves(prefix: &[String], want: &Value, out: &mut Vec<(Vec<String>, Value)>) {
    if is_leaf(want) {
        out.push((prefix.to_vec(), want.clone()));
        return;
    }
    for (k, v) in want.as_object().into_iter().flatten() {
        let mut p = prefix.to_vec();
        p.push(k.clone());
        body_leaves(&p, v, out);
    }
}

fn set_at(value: &mut Value, path: &[String], to: Value) {
    let Some((first, rest)) = path.split_first() else {
        *value = to;
        return;
    };
    if let (Value::Array(items), Ok(i)) = (&mut *value, first.parse::<usize>()) {
        if let Some(slot) = items.get_mut(i) {
            return set_at(slot, rest, to);
        }
    }
    if !value.is_object() {
        *value = Value::Object(Default::default());
    }
    let slot = value
        .as_object_mut()
        .unwrap()
        .entry(first.clone())
        .or_insert(Value::Null);
    set_at(slot, rest, to);
}

pub fn mutants(expect: &Value, status: u16, body: &Value, text: &str) -> Vec<Mutant> {
    let mut out = vec![];
    let one = |what: String, status: u16, body: Value, text: String| Mutant {
        what,
        status,
        body,
        text,
    };
    if expect.get("status").is_some_and(|s| !s.is_null()) {
        out.push(one("status 599".into(), 599, body.clone(), text.into()));
    }
    let mut leaves = vec![];
    if let Some(b) = expect.get("body") {
        body_leaves(&[], b, &mut leaves);
    }
    for (path, want) in expect
        .get("paths")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let segments: Vec<String> = if path == "$" || path.is_empty() {
            vec![]
        } else {
            path.split('.').map(str::to_string).collect()
        };
        leaves.push((segments, want.clone()));
    }
    for (path, want) in leaves {
        let mut changed = body.clone();
        set_at(&mut changed, &path, refused_by(&want));
        let name = if path.is_empty() {
            "$".into()
        } else {
            path.join(".")
        };
        out.push(one(
            format!("{name} made wrong"),
            status,
            changed,
            text.into(),
        ));
    }
    if let Some(want) = expect.get("text") {
        let changed = match refused_by(want) {
            Value::String(s) => s,
            _ => String::new(),
        };
        out.push(one("text made wrong".into(), status, body.clone(), changed));
    }
    out
}

#[cfg(test)]
mod mutant_tests {
    use super::*;
    use serde_json::json;

    fn judge(expect: &Value, m: &Mutant) -> Vec<String> {
        let mut f: Vec<String> = expect
            .get("status")
            .and_then(|s| check_status(s, m.status))
            .into_iter()
            .collect();
        f.extend(check(expect, &m.body, &m.text));
        f
    }

    #[test]
    fn obidspec_6_every_mutant_of_a_real_assertion_fails() {
        let body = json!({"state": "joined", "list": [{"k": 1}], "missing": null, "n": 3, "s": "hello", "one": [1], "nothing": [{"t": "#4"}]});
        let expect = json!({"status": 200, "body": {"state": "joined", "list": {"$any": {"k": 1}}, "one": {"$len": 1}},
            "paths": {"gone": {"$exists": false}, "n": {"$ne": 4}, "s": {"$contains": "ell"}, "list": {"$count": [{"k": 1}, 1]}, "nothing": {"$none": {"t": {"$contains": "#5"}}}},
            "text": {"$not": {"$contains": "secret"}}});
        assert!(judge(
            &expect,
            &Mutant {
                what: String::new(),
                status: 200,
                body: body.clone(),
                text: "ok".into()
            }
        )
        .is_empty());
        let all = mutants(&expect, 200, &body, "ok");
        assert_eq!(all.len(), 10);
        for m in &all {
            assert!(!judge(&expect, m).is_empty(), "{} passed", m.what);
        }
    }

    #[test]
    fn obidspec_6_a_vacuous_assertion_is_caught() {
        // Any status at all: the mutant 599 passes too.
        let expect = json!({"status": [200, 599]});
        let m = &mutants(&expect, 200, &json!({}), "")[0];
        assert!(judge(&expect, m).is_empty());
    }
}
