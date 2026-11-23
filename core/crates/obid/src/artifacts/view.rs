//! Oby's runtime, served beside a design (docs/contracts/artifacts.md, ARTIFACTS-13 to 15): what a
//! `design` version serves that it doesn't keep. Its boards are Claude Design's `.dc.html` files,
//! kept byte for byte; they need a runtime to draw, and Claude's own can't run under obid's policy
//! (it evaluates strings as code and fetches its board). So obid serves Oby's own, written from the
//! format alone (`core/assets/oby-dc/dc-runtime.js`, Apache-2.0) on React 18.3.1 and ReactDOM
//! (vendored unchanged, MIT, `core/assets/oby-dc/LICENSE-react`), with no Babel and nothing from a CDN:
//!
//! - `support.js` (and `<folder>/support.js`, which a board in a folder names): React, ReactDOM,
//!   then the runtime;
//! - `__dc/boards.js`: every board's template, its helmet, whether it has logic, and the defaults
//!   its `data-props` give (its props: Oby v1 shows a board's tweaks as they are);
//! - `__dc/logic/<board>.js`: each board's logic as a script of its own, so one board's mistake
//!   stops only that board;
//! - `__canvas.html`: the canvas host, every board of a page in a sandboxed frame of its own.
//!
//! Each is built for the version when asked, never stored, under [`BOARD_POLICY`] (obid's artifact
//! policy, but framed by its own version's canvas host) or, for the host, [`HOST_POLICY`]. The
//! runtime's paths are root-relative (`/__dc/…`, a board's `/_blob/<id>`), so it draws only on an
//! origin of the version's own (the app's `oby-artifact://<id>-v<n>/`, the port plan's P7);
//! through `/v1/artifacts/{id}/{version}/` a design shows as its files. It reads a version's files
//! and writes nothing.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

/// obid's artifact policy ([`super::POLICY`]) for a design's files, but its frames may sit in the
/// canvas host of the same version (`frame-ancestors 'self'` rather than `'none'`). A board stays
/// sandboxed (an origin of its own), so it reaches neither the host nor another board.
pub const BOARD_POLICY: &str = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' data: blob:; style-src 'self' 'unsafe-inline' data:; img-src 'self' data: blob:; connect-src 'none'; frame-ancestors 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; sandbox allow-scripts";

/// The canvas host's: obid's own page (not sandboxed, so its boards may name it as their
/// ancestor), which still loads nothing but the version's files and fetches nothing.
pub const HOST_POLICY: &str = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'none'; frame-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

const REACT: &str = include_str!("../../../../assets/oby-dc/react.production.min.js");
const REACT_DOM: &str = include_str!("../../../../assets/oby-dc/react-dom.production.min.js");
const RUNTIME: &str = include_str!("../../../../assets/oby-dc/dc-runtime.js");
const CANVAS: &str = include_str!("../../../../assets/oby-dc/canvas.html");

const JS: &str = "text/javascript; charset=utf-8";
const HTML: &str = "text/html; charset=utf-8";

/// What a design's version serves at a path it doesn't keep: its type, its policy, its bytes, and
/// whether it is read cross-origin (the runtime and the logic: a sandboxed board's origin is its
/// own, so it loads them with CORS, and a mistake in them is then said with its file's name).
pub struct Served {
    pub content_type: &'static str,
    pub policy: &'static str,
    pub bytes: Vec<u8>,
    pub cors: bool,
}

/// A board's parts, as its file has them: the template inside `<x-dc>` (its `<helmet>`s taken
/// out), the helmets' insides, its logic (`<script type="text/x-dc" data-dc-script>`), and the
/// defaults its `data-props` give.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardParts {
    pub template: String,
    pub helmet: String,
    pub logic: Option<String>,
    pub defaults: serde_json::Map<String, Value>,
}

fn unescape(attr: &str) -> String {
    attr.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The tweaks' defaults in a logic tag's attributes: `data-props='{"x": {"default": …}}'`, every
/// prop but the `$`-named (`$preview`).
fn defaults_of(attrs: &str) -> serde_json::Map<String, Value> {
    static PROPS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = PROPS.get_or_init(|| {
        regex::Regex::new(r#"(?is)\bdata-props\s*=\s*(?:'([^']*)'|"([^"]*)")"#)
            .expect("it compiles")
    });
    let Some(c) = re.captures(attrs) else {
        return Default::default();
    };
    let raw = c.get(1).or(c.get(2)).map_or("", |m| m.as_str());
    let Ok(Value::Object(props)) = serde_json::from_str::<Value>(&unescape(raw)) else {
        return Default::default();
    };
    props
        .into_iter()
        .filter(|(k, _)| !k.starts_with('$'))
        .filter_map(|(k, v)| Some((k, v.get("default")?.clone())))
        .collect()
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    hay.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

/// A board's file read into its parts.
pub fn board_parts(text: &str) -> BoardParts {
    static HELMET: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static LOGIC: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let helmet_re = HELMET
        .get_or_init(|| regex::Regex::new(r"(?is)<helmet>(.*?)</helmet>").expect("it compiles"));
    let logic_re = LOGIC.get_or_init(|| {
        regex::Regex::new(r#"(?is)<script\b([^>]*)>(.*?)</script>"#).expect("it compiles")
    });
    let inner = match (
        find_ci(text, "<x-dc>"),
        text.to_ascii_lowercase().rfind("</x-dc>"),
    ) {
        (Some(a), Some(b)) if b > a => &text[a + "<x-dc>".len()..b],
        _ => "",
    };
    let helmet: String = helmet_re
        .captures_iter(inner)
        .map(|c| c[1].to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let template = helmet_re.replace_all(inner, "").into_owned();
    let tag = logic_re.captures_iter(text).find(|c| {
        let attrs = c[1].to_ascii_lowercase();
        attrs.contains("text/x-dc") && attrs.contains("data-dc-script")
    });
    BoardParts {
        template,
        helmet,
        logic: tag.as_ref().map(|c| c[2].to_string()),
        defaults: tag.as_ref().map(|c| defaults_of(&c[1])).unwrap_or_default(),
    }
}

fn is_board(path: &str) -> bool {
    path.ends_with(".dc.html")
}

/// `__dc/boards.js`: every board of the version (`boards`: its path and text), its template and
/// helmet, whether it has logic, and its defaults.
pub fn boards_js(boards: &BTreeMap<String, String>) -> String {
    let all: BTreeMap<&str, Value> = boards
        .iter()
        .map(|(p, text)| {
            let parts = board_parts(text);
            (
                p.as_str(),
                json!({"template": parts.template, "helmet": parts.helmet,
                       "logic": parts.logic.is_some(), "defaults": parts.defaults}),
            )
        })
        .collect();
    format!(
        "window.__OBY_DC=window.__OBY_DC||{{}};window.__OBY_DC.boards={};\n",
        serde_json::to_string(&all).unwrap_or_else(|_| "{}".into())
    )
}

/// `__dc/logic/<board>.js`: the board's logic as a script of its own (the runtime tells its
/// mistakes by the file's name).
pub fn logic_js(name: &str, logic: &str) -> String {
    format!(
        "window.__OBY_DC=window.__OBY_DC||{{}};window.__OBY_DC.logic=window.__OBY_DC.logic||{{}};\nwindow.__OBY_DC.logic[{}]=function(DCLogic,React){{\n{logic}\n;return typeof Component==='undefined'?null:Component;}};\n",
        serde_json::to_string(name).unwrap_or_default()
    )
}

/// `support.js`: React and ReactDOM, then Oby's runtime.
pub fn support_js() -> String {
    format!("{REACT}\n;\n{REACT_DOM}\n;\n{RUNTIME}")
}

/// `__canvas.html`: the canvas host with this version's `canvas.json` in it, as data a script
/// reads: every `<` written `\u003c` (it is only ever inside a JSON string), so nothing in it can
/// end or open anything in the block, and `JSON.parse` reads it back as it was.
pub fn canvas_html(canvas: &Value) -> String {
    let data = canvas.to_string().replace('<', "\\u003c");
    CANVAS.replace("{{CANVAS_JSON}}", &data)
}

/// What a design version at `folder`, listing `files`, serves at `path` that it doesn't keep, or
/// none (then it is one of its own files, or `404`). `support.js` comes first, a file of that name
/// in the version included: Claude's own runtime, which a project zip may hold, can't run here.
pub fn serve(folder: &Path, files: &[String], path: &str) -> Option<Served> {
    let read = |name: &str| -> Option<String> {
        files.iter().any(|f| f == name).then_some(())?;
        let file = super::inside(folder, name)?;
        std::fs::read(file)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    let js = |bytes: String| Served {
        content_type: JS,
        policy: BOARD_POLICY,
        bytes: bytes.into_bytes(),
        cors: true,
    };
    if path == "support.js" || path.ends_with("/support.js") {
        return Some(js(support_js()));
    }
    if path == "__dc/boards.js" {
        let boards: BTreeMap<String, String> = files
            .iter()
            .filter(|f| is_board(f))
            .filter_map(|f| Some((f.clone(), read(f)?)))
            .collect();
        return Some(js(boards_js(&boards)));
    }
    if let Some(board) = path
        .strip_prefix("__dc/logic/")
        .and_then(|b| b.strip_suffix(".js"))
    {
        let logic = board_parts(&read(board)?).logic?;
        return Some(js(logic_js(board, &logic)));
    }
    if path == "__canvas.html" {
        let canvas: Value = serde_json::from_str(&read("canvas.json")?).ok()?;
        return Some(Served {
            content_type: HTML,
            policy: HOST_POLICY,
            bytes: canvas_html(&canvas).into_bytes(),
            cors: false,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "<!doctype html><html><head><script src=\"./support.js\"></script></head><body>\n<x-dc><helmet><style>h1{color:red}</style></helmet><h1>{{title}}</h1></x-dc>\n<script type=\"text/x-dc\" data-dc-script data-props='{\"start\": {\"default\": \"home\"}, \"$preview\": {\"default\": 1}}'>\nclass Component extends DCLogic { renderVals() { return {title: 'Hi'}; } }\n</script></body></html>\n";

    #[test]
    fn a_board_is_read_into_its_parts() {
        let parts = board_parts(BOARD);
        assert_eq!(parts.template, "<h1>{{title}}</h1>");
        assert_eq!(parts.helmet, "<style>h1{color:red}</style>");
        assert!(
            parts
                .logic
                .unwrap()
                .contains("class Component extends DCLogic")
        );
        assert_eq!(Value::Object(parts.defaults), json!({"start": "home"}));
    }

    /// The canvas host's data can't close its block, whatever canvas.json's words hold.
    #[test]
    fn canvas_json_can_not_end_the_hosts_block() {
        let host = canvas_html(&json!({"title": "</script><script>alert(1)</script><!--"}));
        assert!(!host.contains("</script><script>alert(1)"));
        assert!(!host.contains("<!--\""));
    }

    #[test]
    fn canvas_json_in_the_host_reads_back_as_it_was() {
        let canvas = json!({"title": "a </script> <!-- b --> <\\/c>", "notes": {"n": {"text": "x < y"}}});
        let host = canvas_html(&canvas);
        let start = host.find("id=\"oby-canvas\">").expect("the block") + "id=\"oby-canvas\">".len();
        let end = start + host[start..].find("</script>").expect("its end");
        let read: Value = serde_json::from_str(&host[start..end]).expect("JSON.parse reads it");
        assert_eq!(read, canvas);
    }

    use super::super::tests::{Temp, what};
    use super::super::{Artifacts, POLICY};

    fn board_file(title: &str, body: &str, logic: bool) -> String {
        let script = if logic {
            "<script type=\"text/x-dc\" data-dc-script data-props='{\"start\": {\"default\": \"home\"}}'>\nclass Component extends DCLogic {}\n</script>\n"
        } else {
            ""
        };
        format!(
            "<!doctype html>\n<html><head><title>{title}</title><script src=\"./support.js\"></script></head><body>\n<x-dc>\n{body}\n</x-dc>\n{script}</body></html>\n"
        )
    }

    /// A design of two boards (Main with logic and Card with one too), published.
    async fn published(t: &Temp) -> (std::sync::Arc<Artifacts>, String) {
        let canvas = json!({
            "v": 3,
            "title": "View design </script><!--",
            "pages": [{"id": "design", "name": "Design"}],
            "boards": {"Main.dc.html": {"x": 0, "y": 0, "w": 800, "h": 600, "page": "design"},
                       "Card.dc.html": {"x": 900, "y": 0, "w": 400, "h": 300, "page": "design"}},
            "order": ["Main.dc.html", "Card.dc.html"],
            "designSystems": [{"title": "View", "namespace": "view", "version": "1"}]
        });
        t.write(
            "design/canvas.json",
            serde_json::to_string_pretty(&canvas).unwrap(),
        );
        t.write(
            "design/Main.dc.html",
            board_file(
                "Main",
                "<h1>{{title}}</h1>\n<dc-import name=\"Card\"></dc-import>",
                true,
            ),
        );
        t.write(
            "design/Card.dc.html",
            board_file("Card", "<p>Card</p>", true),
        );
        t.write("design/ds/view/tokens.json", "{}");
        let (store, _events) = t.store();
        let room = t.room();
        let (id, _) = store.publish(what(&room, "design")).await.unwrap();
        (store, id)
    }

    /// ARTIFACTS-13: a design's version serves Oby's runtime beside it (React, ReactDOM, the
    /// runtime), every board's parts and each board's logic, under the board policy, read
    /// cross-origin; a board in a folder finds `support.js` beside it too.
    #[tokio::test]
    async fn artifacts_13_a_design_serves_obys_runtime_beside_it() {
        let t = Temp::new("view-13");
        let (store, id) = published(&t).await;
        for path in ["support.js", "ds/view/support.js"] {
            let support = store.served(&id, "1", path).unwrap();
            assert_eq!(support.content_type, JS);
            assert_eq!((support.policy, support.cors), (BOARD_POLICY, true));
            let text = String::from_utf8(support.bytes).unwrap();
            assert!(text.contains("react.production.min.js") && text.contains("oby-dc 1"));
        }
        let boards =
            String::from_utf8(store.served(&id, "1", "__dc/boards.js").unwrap().bytes).unwrap();
        let data: Value = serde_json::from_str(
            boards
                .strip_prefix("window.__OBY_DC=window.__OBY_DC||{};window.__OBY_DC.boards=")
                .unwrap()
                .trim_end()
                .trim_end_matches(';'),
        )
        .unwrap();
        assert_eq!(data["Main.dc.html"]["logic"], true);
        assert_eq!(data["Main.dc.html"]["defaults"], json!({"start": "home"}));
        assert!(
            data["Main.dc.html"]["template"]
                .as_str()
                .unwrap()
                .contains("<dc-import name=\"Card\">")
        );
        assert_eq!(data.as_object().unwrap().len(), 2);
        let logic = store
            .served(&id, "1", "__dc/logic/Main.dc.html.js")
            .unwrap();
        assert!(
            String::from_utf8(logic.bytes)
                .unwrap()
                .contains("class Component extends DCLogic")
        );
        assert!(logic.cors);
        // A file with no logic script has none to serve; a file the version doesn't hold, neither.
        assert!(
            store
                .served(&id, "1", "__dc/logic/canvas.json.js")
                .is_none()
        );
        assert!(
            store
                .served(&id, "1", "__dc/logic/Gone.dc.html.js")
                .is_none()
        );
        // The boards themselves, byte for byte, under the board policy.
        let main = store.served(&id, "1", "Main.dc.html").unwrap();
        assert_eq!((main.policy, main.cors), (BOARD_POLICY, false));
        assert_eq!(
            main.bytes,
            std::fs::read(t.room().join("design/Main.dc.html")).unwrap()
        );
    }

    /// ARTIFACTS-13: a page that isn't a design serves no runtime and keeps obid's policy.
    #[tokio::test]
    async fn artifacts_13_a_page_serves_no_runtime() {
        let t = Temp::new("view-13-page");
        t.write("page/index.html", "<h1>Page</h1>");
        let (store, _events) = t.store();
        let room = t.room();
        let (id, _) = store.publish(what(&room, "page")).await.unwrap();
        for path in ["support.js", "__dc/boards.js", "__canvas.html"] {
            assert!(store.served(&id, "1", path).is_none(), "{path}");
        }
        assert_eq!(store.served(&id, "1", "index.html").unwrap().policy, POLICY);
    }

    /// ARTIFACTS-14: `__canvas.html` is the canvas host built for the version, its canvas.json in
    /// it as data its words can't break out of, each board a sandboxed frame, under the host policy.
    #[tokio::test]
    async fn artifacts_14_the_canvas_host_is_built_for_the_version() {
        let t = Temp::new("view-14");
        let (store, id) = published(&t).await;
        let host = store.served(&id, "1", "__canvas.html").unwrap();
        assert_eq!(
            (host.content_type, host.policy, host.cors),
            (HTML, HOST_POLICY, false)
        );
        let text = String::from_utf8(host.bytes).unwrap();
        assert!(text.contains("View design \\u003c/script>\\u003c!--"), "{text}");
        assert!(!text.contains("{{CANVAS_JSON}}"));
        assert!(text.contains("sandbox: 'allow-scripts'"));
        assert!(store.served(&id, "2", "__canvas.html").is_none());
    }

    /// ARTIFACTS-15: the board policy is obid's artifact policy but for its framing: no fetch,
    /// nothing from outside, no eval, sandboxed with scripts only (no window, no popup).
    #[test]
    fn artifacts_15_a_design_is_served_under_the_board_policy() {
        assert_eq!(
            BOARD_POLICY,
            POLICY.replace("frame-ancestors 'none'", "frame-ancestors 'self'")
        );
        for policy in [BOARD_POLICY, HOST_POLICY] {
            assert!(policy.contains("connect-src 'none'"));
            assert!(policy.contains("default-src 'self' data: blob:"));
            assert!(!policy.contains("unsafe-eval"));
            assert!(!policy.contains("allow-popups") && !policy.contains("allow-same-origin"));
        }
        assert!(BOARD_POLICY.ends_with("sandbox allow-scripts"));
        assert!(HOST_POLICY.contains("frame-ancestors 'none'"));
    }

    /// React and ReactDOM are npm's 18.3.1 production builds, unchanged (core/assets/oby-dc/README.md).
    #[test]
    fn the_vendored_react_is_react_18_3_1() {
        use sha2::Digest;
        let sha = |text: &str| hex::encode(sha2::Sha256::digest(text.as_bytes()));
        assert_eq!(
            sha(REACT),
            "d949f1c3687aedadcedac85261865f29b17cd273997e7f6b2bfc53b2f9d4c4dd"
        );
        assert_eq!(
            sha(REACT_DOM),
            "35f4f974f4b2bcd44da73963347f8952e341f83909e4498227d4e26b98f66f0d"
        );
        assert!(REACT.contains("18.3.1") && REACT_DOM.contains("18.3.1"));
    }

    #[test]
    fn the_runtime_is_react_then_oby() {
        let support = support_js();
        let (react, oby) = (
            support.find("React").unwrap(),
            support.find("oby-dc 1").unwrap(),
        );
        assert!(react < oby);
    }
}
