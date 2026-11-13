//! The route guard (docs/contracts/ci.md, CI-9 to 11): obid's API stays add-only, so a route that
//! goes, a client call to a path obid doesn't serve and a gateway entry with no route each fail the
//! check. The routes are read from the `.route(` calls in obid's source, and listed in
//! docs/api/routes.txt; the client calls from the Electron app, the shared Swift client and
//! oby-mcp's server; the gateway's entries from `ROUTES` in gateway.rs. Each checker also runs on
//! inline fixtures, one that must fail and one that must pass, so the guard is seen red.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;

/// The repository's root, from obid's crate folder.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative))
        .unwrap_or_else(|e| panic!("can't read {relative}: {e}"))
}

/// Every file under `relative` (repository-relative, `/`-separated) that `keep` keeps, sorted.
fn files_under(relative: &str, keep: &dyn Fn(&str) -> bool) -> Vec<String> {
    fn walk(dir: &Path, relative: &str, keep: &dyn Fn(&str) -> bool, out: &mut Vec<String>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("can't list {relative}: {e}"));
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{relative}/{name}");
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), &path, keep, out);
            } else if keep(&path) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join(relative), relative, keep, &mut out);
    out.sort();
    out
}

/// A file with what it holds, as the checkers take it.
fn with_source(files: Vec<String>) -> Vec<(String, String)> {
    files
        .into_iter()
        .map(|f| {
            let source = read(&f);
            (f, source)
        })
        .collect()
}

// The routes

const ROUTE_LIST: &str = "docs/api/routes.txt";
const SOURCE: &str = "core/crates/obid/src";
const GATEWAY: &str = "core/crates/obid/src/gateway.rs";
const HEADER: &str = "\
# The routes obid serves: one line per method and path, sorted by path, then method
# (docs/contracts/ci.md, CI-9). core/crates/obid/tests/routes_guard.rs rebuilds this list from
# the `.route(` calls under core/crates/obid/src and fails when it differs: commit a new route's
# line with the route. A route that goes shows here as a deleted line.
";
const GATEWAY_HEADING: &str =
    "# The gateway's own listener (gateway.rs, `router`), not the API's router:";
/// The method routers a route may use; a call after the path to anything else but a layer fails.
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];
const LAYERS: [&str; 2] = ["layer", "route_layer"];

/// Routes as (path, method), so they sort by path, then method.
#[derive(Debug, Default, PartialEq)]
struct Routes {
    api: BTreeSet<(String, String)>,
    gateway: BTreeSet<(String, String)>,
}

fn line((path, method): &(String, String)) -> String {
    format!("{method} {path}")
}

/// A file's lines but its test modules, by `production_lines`' rule in scripts/oby-check.sh: from
/// a `#[cfg(test)]` on a `mod name {` to the next `}` in column 0. Comment lines are dropped too.
fn production(source: &str) -> String {
    let module = Regex::new(r"^(pub(\([^)]*\))? )?mod [A-Za-z0-9_]+ *\{").unwrap();
    let (mut out, mut held, mut skip) = (Vec::new(), Vec::new(), false);
    for line in source.lines() {
        if skip {
            skip = !line.starts_with('}');
            continue;
        }
        if !held.is_empty() {
            if line.trim().is_empty() {
                held.push(line);
                continue;
            }
            if module.is_match(line) {
                held.clear();
                skip = !line.trim_end().ends_with('}');
                continue;
            }
            out.append(&mut held);
        }
        if line.starts_with("#[cfg(test)]") {
            held.push(line);
            continue;
        }
        out.push(line);
    }
    out.append(&mut held);
    out.retain(|l| !l.trim_start().starts_with("//"));
    out.join("\n")
}

/// The calls at the top nesting level of the method-router expression that starts `text`, up to
/// the `,` or `)` that ends it: `get(a).post(b)` is `get` and `post`, never the handlers.
fn top_level_calls(text: &str) -> Vec<String> {
    let (mut calls, mut name, mut depth) = (Vec::new(), String::new(), 0usize);
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(s) = chars.next() {
                    match s {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
                name.clear();
            }
            '(' | '[' | '{' => {
                if depth == 0 && c == '(' && !name.is_empty() {
                    calls.push(name.rsplit("::").next().unwrap().to_owned());
                }
                name.clear();
                depth += 1;
            }
            ')' | ']' | '}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                name.clear();
            }
            ',' if depth == 0 => break,
            c if depth == 0 && (c.is_alphanumeric() || c == '_' || c == ':') => name.push(c),
            _ => name.clear(),
        }
    }
    calls
}

/// The routes the files declare: each `.route(` whose path is a literal starting `/v1/`, one per
/// method, gateway.rs's in its own section. A method router the guard doesn't know is a problem.
fn declared(files: &[(String, String)]) -> Result<Routes, Vec<String>> {
    let (mut routes, mut problems) = (Routes::default(), Vec::new());
    for (file, source) in files {
        let text = production(source);
        for (at, _) in text.match_indices(".route(") {
            let rest = text[at + ".route(".len()..].trim_start();
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            let Some(end) = rest.find('"') else { continue };
            let path = &rest[..end];
            if !path.starts_with("/v1/") {
                continue;
            }
            let Some(expression) = rest[end + 1..].trim_start().strip_prefix(',') else {
                problems.push(format!("{file}: `.route(\"{path}\"` has no method router"));
                continue;
            };
            let calls = top_level_calls(expression);
            if !calls.iter().any(|c| METHODS.contains(&c.as_str())) {
                problems.push(format!(
                    "{file}: `.route(\"{path}\", …)` has no method router"
                ));
            }
            for call in calls {
                if METHODS.contains(&call.as_str()) {
                    let section = if file.ends_with("src/gateway.rs") {
                        &mut routes.gateway
                    } else {
                        &mut routes.api
                    };
                    section.insert((path.to_owned(), call.to_uppercase()));
                } else if !LAYERS.contains(&call.as_str()) {
                    problems.push(format!(
                        "{file}: `.route(\"{path}\", …)` uses `{call}(…)`, which the route guard \
                         doesn't read: teach core/crates/obid/tests/routes_guard.rs its method"
                    ));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(routes)
    } else {
        Err(problems)
    }
}

/// docs/api/routes.txt as the guard writes it.
fn render(routes: &Routes) -> String {
    let mut out = format!("{HEADER}\n");
    for route in &routes.api {
        out += &format!("{}\n", line(route));
    }
    out += &format!("\n{GATEWAY_HEADING}\n");
    for route in &routes.gateway {
        out += &format!("{}\n", line(route));
    }
    out
}

/// A route list's lines by section: the API's, then, from the first `#` line after a route, the
/// gateway's.
fn listed(list: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut api, mut gateway, mut in_gateway) = (BTreeSet::new(), BTreeSet::new(), false);
    for l in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if l.starts_with('#') {
            in_gateway |= !api.is_empty();
        } else if in_gateway {
            gateway.insert(l.to_owned());
        } else {
            api.insert(l.to_owned());
        }
    }
    (api, gateway)
}

/// CI-9: what differs between the routes declared and the committed list.
fn route_list_problems(routes: &Routes, list: &str) -> Vec<String> {
    let (api, gateway) = listed(list);
    let mut problems = Vec::new();
    for (section, declared, committed) in [
        ("the API's router", &routes.api, &api),
        ("the gateway's listener", &routes.gateway, &gateway),
    ] {
        let declared: BTreeSet<String> = declared.iter().map(line).collect();
        for went in committed.difference(&declared) {
            problems.push(format!(
                "`{went}` went: {ROUTE_LIST} lists it under {section} and nothing declares it \
                 there (obid's API stays add-only: put the route back)"
            ));
        }
        for appeared in declared.difference(committed) {
            problems.push(format!(
                "`{appeared}` appeared: {section} declares it and {ROUTE_LIST} doesn't list it \
                 (commit its line with the route)"
            ));
        }
    }
    let expected = render(routes);
    if problems.is_empty() && list != expected {
        problems.push(format!(
            "{ROUTE_LIST} isn't as the route guard writes it (its header, order or sections); \
             it should read:\n{expected}"
        ));
    }
    problems
}

/// obid's source files, as the guard reads them.
fn obid_sources() -> Vec<(String, String)> {
    with_source(files_under(SOURCE, &|f| f.ends_with(".rs")))
}

/// Every path docs/api/routes.txt lists, in both sections, and its API section's (method, path).
fn listed_routes() -> (Vec<String>, Vec<(String, String)>) {
    let (api, gateway) = listed(&read(ROUTE_LIST));
    let split = |l: &String| {
        let (method, path) = l.split_once(' ').unwrap();
        (method.to_owned(), path.to_owned())
    };
    let paths = api.iter().chain(&gateway).map(|l| split(l).1).collect();
    (paths, api.iter().map(split).collect())
}

#[test]
fn ci_9_a_removed_route_fails() {
    // Seen red: a list holding a route the source no longer declares.
    let source = r#"
pub fn router() -> Router {
    Router::new()
        // .route("/v1/commented", get(nothing))
        .route("/v1/status", get(status))
        .route(
            "/v1/jobs/{id}",
            axum::routing::get(crate::routes::job).patch(rename(x)).delete(remove),
        )
        .route("/v1/attachments", post(upload).layer(limit()))
        .route("/callback", get(callback))
}

#[cfg(test)]
mod tests {
    fn stand_in() -> Router {
        Router::new().route("/v1/stand-in", get(|| async { "{}" }))
    }
}
"#;
    let gateway =
        r#"pub fn router() -> Router { Router::new().route("/v1/pairing", post(redeem)) }"#;
    let files = [
        ("core/crates/obid/src/api.rs".to_owned(), source.to_owned()),
        (GATEWAY.to_owned(), gateway.to_owned()),
    ];
    let routes = declared(&files).unwrap();
    let mut list = render(&routes);
    assert_eq!(
        list,
        format!(
            "{HEADER}\nPOST /v1/attachments\nDELETE /v1/jobs/{{id}}\nGET /v1/jobs/{{id}}\n\
             PATCH /v1/jobs/{{id}}\nGET /v1/status\n\n{GATEWAY_HEADING}\nPOST /v1/pairing\n"
        )
    );
    assert_eq!(route_list_problems(&routes, &list), Vec::<String>::new());
    list = list.replacen("GET /v1/status\n", "POST /v1/clear\nGET /v1/status\n", 1);
    let problems = route_list_problems(&routes, &list);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].contains("`POST /v1/clear` went"),
        "{problems:#?}"
    );
    // A method router it doesn't know fails, naming the file and the path.
    let unknown = [(
        "core/crates/obid/src/x.rs".to_owned(),
        r#"Router::new().route("/v1/any", any(handler))"#.to_owned(),
    )];
    let problems = declared(&unknown).unwrap_err();
    assert!(
        problems[0].contains("core/crates/obid/src/x.rs") && problems[0].contains("/v1/any"),
        "{problems:#?}"
    );

    let routes = declared(&obid_sources()).unwrap_or_else(|p| panic!("{}", p.join("\n")));
    let problems = route_list_problems(&routes, &read(ROUTE_LIST));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// The clients' calls

const DEAD_CALLS: &str = "docs/api/dead-client-calls.txt";

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// A string literal's text, `\0` for each interpolation, or `None` where a `'` or `"` literal
/// doesn't end on its line. `chars` is past the opening `quote`.
fn literal(chars: &mut Chars<'_>, quote: char, swift: bool) -> Option<String> {
    let mut text = String::new();
    while let Some(c) = chars.next() {
        match c {
            c if c == quote => return Some(text),
            '\n' if quote != '`' => return None,
            // Swift's `\(…)`, with balanced parentheses.
            '\\' if swift && chars.peek() == Some(&'(') => {
                chars.next();
                skip_balanced(chars, '(', ')');
                text.push('\0');
            }
            '\\' => text.extend(chars.next()),
            // TypeScript's `${…}`, with balanced braces.
            '$' if quote == '`' && chars.peek() == Some(&'{') => {
                chars.next();
                skip_balanced(chars, '{', '}');
                text.push('\0');
            }
            c => text.push(c),
        }
    }
    None
}

/// Past the `close` that balances an `open` already read, skipping the strings in between.
fn skip_balanced(chars: &mut Chars<'_>, open: char, close: char) {
    let mut depth = 1;
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' | '`' => {
                while let Some(s) = chars.next() {
                    match s {
                        '\\' => {
                            chars.next();
                        }
                        s if s == c => break,
                        _ => {}
                    }
                }
            }
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

/// A literal's path as the guard compares it: a leading `/`, cut at the first `?`, a trailing
/// interpolation glued to text dropped (a query or suffix helper), and every segment with an
/// interpolation in it a wildcard, `*`.
fn normalised(text: &str) -> String {
    let mut path = if text.starts_with('/') {
        text.to_owned()
    } else {
        format!("/{text}")
    };
    if let Some(at) = path.find('?') {
        path.truncate(at);
    }
    while path.ends_with('\0') && !path[..path.len() - 1].ends_with(['/', '\0']) {
        path.pop();
    }
    path.split('/')
        .map(|s| if s.contains('\0') { "*" } else { s })
        .collect::<Vec<_>>()
        .join("/")
}

/// The `/v1/` paths a client file's string literals hold, normalised: Swift's literals are `"…"`,
/// TypeScript's `'…'`, `"…"` and `` `…` ``. Comment lines are skipped.
fn client_paths(file: &str, source: &str) -> BTreeSet<String> {
    let swift = file.ends_with(".swift");
    let quotes: &[char] = if swift { &['"'] } else { &['\'', '"', '`'] };
    let text: Vec<&str> = source
        .lines()
        .filter(|l| {
            let l = l.trim_start();
            !(l.starts_with("//") || l.starts_with("/*") || l.starts_with('*'))
        })
        .collect();
    let text = text.join("\n");
    let mut chars = text.chars().peekable();
    let mut paths = BTreeSet::new();
    while let Some(c) = chars.next() {
        if quotes.contains(&c)
            && let Some(literal) = literal(&mut chars, c, swift)
            && (literal.starts_with("/v1/") || literal.starts_with("v1/"))
        {
            paths.insert(normalised(&literal));
        }
    }
    paths
}

/// A path's segments after its leading `/`.
fn segments(path: &str) -> Vec<&str> {
    path.trim_start_matches('/').split('/').collect()
}

/// Whether a route serves a client's path: a client `*` matches any segment, a route `{x}` any
/// one, a route `{*x}` one or more; a path ending in `/` is a prefix some route must start with.
fn serves(route: &str, path: &str) -> bool {
    let route = segments(route);
    let prefix = path.ends_with('/');
    let path = segments(path.trim_end_matches('/'));
    for (i, r) in route.iter().enumerate() {
        if r.starts_with("{*") {
            return prefix || path.len() > i;
        }
        if i == path.len() {
            return prefix;
        }
        if !(path[i] == "*" || r.starts_with('{') || path[i] == *r) {
            return false;
        }
    }
    !prefix && route.len() == path.len()
}

/// CI-10: the client files' calls no route serves, against the committed list of the calls
/// already dead, which only shrinks.
fn client_call_problems(
    clients: &[(String, String)],
    routes: &[String],
    known_dead: &str,
) -> Vec<String> {
    let mut found = BTreeSet::new();
    for (file, source) in clients {
        for path in client_paths(file, source) {
            found.insert((file.clone(), path));
        }
    }
    let dead = |(_, path): &(String, String)| !routes.iter().any(|r| serves(r, path));
    let known: BTreeSet<(String, String)> = known_dead
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (file, path) = l.split_once(' ').unwrap_or((l, ""));
            (file.to_owned(), path.trim().to_owned())
        })
        .collect();
    let mut problems = Vec::new();
    for call @ (file, path) in found.iter().filter(|c| dead(c)) {
        if !known.contains(call) {
            problems.push(format!(
                "{file} calls {path}, which obid doesn't serve: add the route, or change the call \
                 ({ROUTE_LIST})"
            ));
        }
    }
    for call @ (file, path) in &known {
        if !found.contains(call) {
            problems.push(format!(
                "{DEAD_CALLS} lists `{file} {path}`, which is no longer in {file}: delete that line"
            ));
        } else if !dead(call) {
            problems.push(format!(
                "{DEAD_CALLS} lists `{file} {path}`, which isn't dead (obid serves it): delete \
                 that line"
            ));
        }
    }
    problems
}

/// The client sources the guard reads: the Electron app's TypeScript but its tests and mock, and
/// the Swift of the shared client and oby-mcp's server.
fn client_sources() -> Vec<(String, String)> {
    let electron = files_under("apps/electron/src", &|f| {
        (f.ends_with(".ts") || f.ends_with(".tsx"))
            && !f.contains("/test/")
            && !f.ends_with(".test.ts")
            && !f.ends_with(".test.tsx")
            && f != "apps/electron/src/renderer/src/mock.ts"
    });
    let swift = |f: &str| f.ends_with(".swift");
    let mut files = electron;
    files.extend(files_under("apps/shared", &swift));
    files.extend(files_under("mcp/server", &swift));
    with_source(files)
}

#[test]
fn ci_10_a_client_path_without_a_route_fails() {
    let routes = [
        "/v1/jobs".to_owned(),
        "/v1/jobs/{id}/stop".to_owned(),
        "/v1/artifacts/{id}/{version}/{*path}".to_owned(),
    ];
    // Seen red: a call to a path with no route.
    let ts = "apps/electron/src/renderer/src/fixture.ts".to_owned();
    let red = [(ts.clone(), "call('GET', '/v1/nowhere');\n".to_owned())];
    let problems = client_call_problems(&red, &routes, "");
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].contains(&ts) && problems[0].contains("/v1/nowhere"),
        "{problems:#?}"
    );
    // Served paths, with interpolated segments, a query, a suffix helper and a bare prefix pass.
    let served = [
        (
            ts,
            "// call('GET', '/v1/commented')\n\
             call('POST', `/v1/jobs/${id(j)}/stop?now=1`);\n\
             call('GET', `/v1/jobs${q({ limit })}`);\n\
             call('GET', \"/v1/artifacts/\" + rest); // don't\n"
                .to_owned(),
        ),
        (
            "apps/shared/Fixture.swift".to_owned(),
            "call(\"POST\", \"v1/jobs/\\(try id(\"x\"))/stop\")\n\
             call(\"GET\", \"v1/artifacts/\\(a)/\\(b)/\\(c)/d\")\n"
                .to_owned(),
        ),
    ];
    assert_eq!(
        client_call_problems(&served, &routes, ""),
        Vec::<String>::new()
    );
    // The known-dead list only shrinks: a listed call that's gone or served fails.
    let problems = client_call_problems(
        &served,
        &routes,
        "apps/shared/Fixture.swift /v1/gone\napps/shared/Fixture.swift /v1/jobs/*/stop\n",
    );
    assert_eq!(problems.len(), 2, "{problems:#?}");
    assert!(
        problems[0].contains("no longer in") && problems[1].contains("isn't dead"),
        "{problems:#?}"
    );

    let (paths, _) = listed_routes();
    let problems = client_call_problems(&client_sources(), &paths, &read(DEAD_CALLS));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// The gateway's allowlist

/// gateway.rs's `ROUTES` entries, as (method, pattern), from its source.
fn gateway_entries(source: &str) -> Vec<(String, String)> {
    let text = production(source);
    let Some(start) = text.find("const ROUTES") else {
        return Vec::new();
    };
    let entries = &text[start..];
    let entries = &entries[..entries.find("];").unwrap_or(entries.len())];
    Regex::new(r#"\(\s*"([A-Z]+)"\s*,\s*"([^"]*)"\s*\)"#)
        .unwrap()
        .captures_iter(entries)
        .map(|c| (c[1].to_owned(), c[2].to_owned()))
        .collect()
}

/// Whether a gateway pattern reaches a route: `*` is any one segment, a literal an equal one or a
/// `{x}`, and a final `**` one or more segments, or a `{*x}`.
fn reaches(pattern: &str, route: &str) -> bool {
    let mut pattern = segments(pattern);
    let route = segments(route);
    let rest = pattern.last() == Some(&"**");
    if rest {
        pattern.pop();
    }
    let length = if rest {
        route.len() > pattern.len()
    } else {
        route.len() == pattern.len()
    };
    length
        && pattern
            .iter()
            .zip(&route)
            .all(|(p, r)| *p == "*" || p == r || (r.starts_with('{') && !r.starts_with("{*")))
}

/// CI-11: the gateway's entries no API route serves.
fn gateway_problems(source: &str, api: &[(String, String)]) -> Vec<String> {
    let entries = gateway_entries(source);
    if entries.is_empty() {
        return vec![format!(
            "{GATEWAY} has no `const ROUTES` entries the guard can read"
        )];
    }
    entries
        .iter()
        .filter(|(method, pattern)| !api.iter().any(|(m, r)| m == method && reaches(pattern, r)))
        .map(|(method, pattern)| {
            format!(
                "gateway.rs's ROUTES lets a phone `{method} {pattern}`, which no route of the \
                 API's serves: delete the entry"
            )
        })
        .collect()
}

#[test]
fn ci_11_a_dead_gateway_entry_fails() {
    let api = [
        ("GET".to_owned(), "/v1/jobs/{id}".to_owned()),
        (
            "GET".to_owned(),
            "/v1/artifacts/{id}/{version}/{*path}".to_owned(),
        ),
    ];
    let source = |extra: &str| {
        format!(
            "const ROUTES: &[(&str, &str)] = &[\n    (\"GET\", \"/v1/jobs/*\"),\n    \
             // (\"GET\", \"/v1/commented\"),\n    (\"GET\", \"/v1/artifacts/*/*/**\"),\n{extra}];\n"
        )
    };
    // Seen red: an entry no route serves.
    let problems = gateway_problems(&source("    (\"POST\", \"/v1/watches\"),\n"), &api);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(problems[0].contains("`POST /v1/watches`"), "{problems:#?}");
    // A method no route of that path has is dead too.
    assert_eq!(
        gateway_problems(&source("    (\"POST\", \"/v1/jobs/*\"),\n"), &api).len(),
        1
    );
    assert_eq!(gateway_problems(&source(""), &api), Vec::<String>::new());

    let (_, api) = listed_routes();
    let problems = gateway_problems(&read(GATEWAY), &api);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
