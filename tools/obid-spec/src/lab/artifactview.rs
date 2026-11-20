//! The lab's `artifactview` feature (src/lab/mod.rs): how Oby shows an artifact, the second lab
//! PR of Oby Artifacts (plan §3.2 and §3.3, order §7.2). Its nodes are `lab.artifactview.<verb>`.
//! Owned by the session building Oby Artifacts; nothing else edits this file.
//!
//! Like the other lab features, two halves, and only the first is Oby's to port:
//!
//! 1. **The core** (pure, no IO, no clock): what a version serves to a view ([`serve`]): its files
//!    under obid's artifact CSP, and for a `design` Oby's own runtime of the `.dc.html` format
//!    (`lab-ui/artifacts/runtime/dc-runtime.js`, written from the format alone, on React 18.3.1
//!    vendored, no Babel, no CDN) as `support.js`, every board's template and logic beside it
//!    (`__dc/`), and the canvas host (`__canvas.html`), built for the version and never stored;
//!    which requests and navigations a view may make ([`navigation`]: the version's own files only,
//!    an address outside asked of the person first, anything else denied); and the artifact tab
//!    ([`ArtifactTabs`]), the browser contract's `kind: artifact`, the person's only, never streamed,
//!    taken or driven, kept over a restart.
//! 2. **The lab** (from [`View`] down): a view server on 127.0.0.1 where each version is an origin
//!    of its own (`<id>-v<version>.localhost:<port>`, as the app's `oby-artifact://<id>-v<version>/`
//!    will be: a board's root-relative `/_blob/<id>` and `/Board.dc.html` then stay inside the
//!    version), the artifact tabs' routes beside it, chrome-headless-shell driven over the
//!    DevTools protocol (the lab's pinned one), with the app's navigation rule applied to every
//!    request, and `oby_artifact_check` on an agent's folder.
//!
//! Each rule can be switched off, one at a time, only to watch its workflow go red:
//! `OBID_SPEC_LAB_ARTIFACTVIEW_BREAK=runtime,csp,…` ([`Rules::broken`]). obid keeps [`Rules::ALL`].

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::artifacts::{self as store, Files};
use super::artifacts_contract::{self as ac, Artifact, Kind, Version};
use super::browser_contract as bc;
use super::browserapi::Refused;
use crate::obid::Run;

// ════════════════════════════════════════════════════════════════════════════════════════════
// 1. The core: what obid takes as it is.
// ════════════════════════════════════════════════════════════════════════════════════════════

/// The rules a view keeps. Each is on in obid; the lab switches one off only to prove its
/// workflow goes red without it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// A design's `support.js` is Oby's runtime. Off: nothing draws the boards.
    pub runtime: bool,
    /// Every file carries obid's artifact CSP. Off: none.
    pub csp: bool,
    /// The app lets a view load only its version's own files and asks before an address outside.
    /// Off: anything goes.
    pub navigation: bool,
    /// An artifact tab is never streamed, taken or driven. Off: its routes answer as a page's.
    pub tabkind: bool,
    /// Artifact tabs are kept over a restart. Off: forgotten.
    pub persist: bool,
    /// `oby_artifact_check` says each board's mistake by its name. Off: it says none.
    pub errors: bool,
}

impl Rules {
    pub const ALL: Rules = Rules {
        runtime: true,
        csp: true,
        navigation: true,
        tabkind: true,
        persist: true,
        errors: true,
    };

    /// All rules but the ones named (comma-separated): the lab's red proofs.
    pub fn broken(names: &str) -> Rules {
        let mut r = Rules::ALL;
        for n in names.split(',').map(str::trim) {
            match n {
                "runtime" => r.runtime = false,
                "csp" => r.csp = false,
                "navigation" => r.navigation = false,
                "tabkind" => r.tabkind = false,
                "persist" => r.persist = false,
                "errors" => r.errors = false,
                _ => {}
            }
        }
        r
    }
}

/// obid's artifact CSP for a `design`'s files: [`ac::POLICY`], but its frames may sit in the
/// canvas host of the same version (`frame-ancestors 'self'` rather than `'none'`). The board stays
/// sandboxed (an origin of its own), so it reaches neither the host nor another board.
pub const BOARD_POLICY: &str = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' data: blob:; style-src 'self' 'unsafe-inline' data:; img-src 'self' data: blob:; connect-src 'none'; frame-ancestors 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; sandbox allow-scripts";

/// Only for the lab's comparison with Claude's runtime (`<label>-claude`): [`BOARD_POLICY`] with
/// `'unsafe-eval'`, which Claude's runtime needs and obid never grants.
const CLAUDE_POLICY: &str = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' 'unsafe-eval' data: blob:; style-src 'self' 'unsafe-inline' data:; img-src 'self' data: blob:; connect-src 'none'; frame-ancestors 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; sandbox allow-scripts";

/// The canvas host's: obid's own page (no sandbox, so its boards may name it as their ancestor),
/// which still loads nothing but the version's files and fetches nothing.
pub const HOST_POLICY: &str = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'none'; frame-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

const REACT: &str = include_str!("../../lab-ui/artifacts/runtime/vendor/react.production.min.js");
const REACT_DOM: &str =
    include_str!("../../lab-ui/artifacts/runtime/vendor/react-dom.production.min.js");
const RUNTIME: &str = include_str!("../../lab-ui/artifacts/runtime/dc-runtime.js");
const CANVAS: &str = include_str!("../../lab-ui/artifacts/runtime/canvas.html");

/// A board's parts, as its file has them: the template inside `<x-dc>` (its `<helmet>`s taken
/// out), the helmets' insides, its logic (`<script type="text/x-dc" data-dc-script>`), and the
/// defaults its `data-props` give (its tweaks, which Oby v1 shows as they are and doesn't edit).
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

/// `__dc/boards.js`: every board of the version, its template and helmet, and whether it has logic.
pub fn boards_js(files: &Files) -> String {
    let boards: BTreeMap<&str, Value> = files
        .iter()
        .filter(|(p, _)| is_board(p))
        .map(|(p, b)| {
            let parts = board_parts(&String::from_utf8_lossy(b));
            (
                p.as_str(),
                json!({"template": parts.template, "helmet": parts.helmet, "logic": parts.logic.is_some(), "defaults": parts.defaults}),
            )
        })
        .collect();
    format!(
        "window.__OBY_DC=window.__OBY_DC||{{}};window.__OBY_DC.boards={};\n",
        serde_json::to_string(&boards).unwrap_or_else(|_| "{}".into())
    )
}

/// `__dc/logic/<board>.js`: the board's logic as a script of its own, so a mistake in it stops
/// only its board (the runtime tells it by the file's name).
pub fn logic_js(name: &str, logic: &str) -> String {
    format!(
        "window.__OBY_DC=window.__OBY_DC||{{}};window.__OBY_DC.logic=window.__OBY_DC.logic||{{}};\nwindow.__OBY_DC.logic[{}]=function(DCLogic,React){{\n{logic}\n;return typeof Component==='undefined'?null:Component;}};\n",
        serde_json::to_string(name).unwrap_or_default()
    )
}

/// `support.js`: React and ReactDOM, then Oby's runtime, all served by obid from the same version.
pub fn support_js(rules: &Rules) -> String {
    if !rules.runtime {
        return "/* the runtime is off (OBID_SPEC_LAB_ARTIFACTVIEW_BREAK=runtime) */\n".into();
    }
    format!("{REACT}\n;\n{REACT_DOM}\n;\n{RUNTIME}")
}

/// `__canvas.html`: the canvas host with this version's `canvas.json` in it, as data a script
/// reads (`</` can't end the block early).
pub fn canvas_html(canvas: &Value) -> String {
    let data = canvas
        .to_string()
        .replace("</", "<\\/")
        .replace("<!--", "<\\!--");
    CANVAS.replace("{{CANVAS_JSON}}", &data)
}

/// A file a view serves: its media type, its CSP (none only with [`Rules::csp`] off), its bytes.
pub type Served = (String, Option<&'static str>, Vec<u8>);

/// What a view serves for `path` of a version: its media type, its CSP (none only with
/// [`Rules::csp`] off) and its bytes; `None` is `404`.
pub fn serve(
    kind: Kind,
    version: &Version,
    files: &Files,
    path: &str,
    rules: &Rules,
) -> Option<Served> {
    let policy = |p: &'static str| rules.csp.then_some(p);
    let design = kind == Kind::Design;
    let js = "text/javascript; charset=utf-8".to_string();
    if design {
        if path == "support.js" || path.ends_with("/support.js") {
            return Some((js, policy(BOARD_POLICY), support_js(rules).into_bytes()));
        }
        if path == "__dc/boards.js" {
            return Some((js, policy(BOARD_POLICY), boards_js(files).into_bytes()));
        }
        if let Some(board) = path
            .strip_prefix("__dc/logic/")
            .and_then(|b| b.strip_suffix(".js"))
        {
            let logic = board_parts(&String::from_utf8_lossy(files.get(board)?)).logic?;
            return Some((
                js,
                policy(BOARD_POLICY),
                logic_js(board, &logic).into_bytes(),
            ));
        }
        if path == "__canvas.html" || path.is_empty() {
            let canvas: Value = serde_json::from_slice(files.get("canvas.json")?).ok()?;
            return Some((
                "text/html; charset=utf-8".into(),
                policy(HOST_POLICY),
                canvas_html(&canvas).into_bytes(),
            ));
        }
    }
    let path = if path.is_empty() { "index.html" } else { path };
    let kept = store::resolve(version, path, &store::Rules::ALL)?;
    let bytes = files.get(&kept)?.clone();
    Some((
        store::type_of(&kept, &store::Rules::ALL).to_string(),
        policy(if design { BOARD_POLICY } else { ac::POLICY }),
        bytes,
    ))
}

/// What the app does with a request a view makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    /// The version's own files (and `data:`/`blob:` inside a page).
    Allow,
    /// A page going to an address outside: stopped, and the person asked whether to open it
    /// outside Oby. The view stays where it is.
    Ask,
    /// Anything else: stopped.
    Deny,
}

/// The app's rule for every request a view makes (`will-navigate`, `setWindowOpenHandler` and its
/// session's requests in Electron): `origin` is the view's (`oby-artifact://<id>-v<n>`), `document`
/// whether it would replace a page.
pub fn navigation(origin: &str, url: &str, document: bool, rules: &Rules) -> Nav {
    if !rules.navigation {
        return Nav::Allow;
    }
    let own = url == origin || url.starts_with(&format!("{origin}/"));
    if own {
        return Nav::Allow;
    }
    if !document && (url.starts_with("data:") || url.starts_with("blob:")) {
        return Nav::Allow;
    }
    let lower = url.to_ascii_lowercase();
    if document
        && ["https://", "http://", "mailto:", "oby://"]
            .iter()
            .any(|s| lower.starts_with(s))
    {
        return Nav::Ask;
    }
    Nav::Deny
}

/// The address an artifact's tab shows (the app's protocol): a design's canvas, a page's index,
/// any other kind its one file.
pub fn view_url(a: &Artifact, version: u32) -> String {
    let first = a
        .versions
        .iter()
        .find(|v| v.version == version)
        .and_then(|v| v.files.first().cloned())
        .unwrap_or_default();
    let path = match a.kind {
        Kind::Design => "__canvas.html".to_string(),
        Kind::Html => "index.html".to_string(),
        _ => first,
    };
    format!("oby-artifact://{}-v{version}/{path}", a.id)
}

/// The person's artifact tabs (the browser contract's `kind: artifact`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactTabs {
    pub tabs: Vec<bc::Tab>,
    pub next: u64,
}

fn no_tab() -> Refused {
    Refused::new(404, "no_such_tab", "There's no such tab.")
}

impl ArtifactTabs {
    /// The artifact's tab, opened on its latest version, or the one already open.
    pub fn open(&mut self, a: &Artifact) -> bc::Tab {
        if let Some(t) = self
            .tabs
            .iter()
            .find(|t| t.artifact.as_ref().is_some_and(|r| r.id == a.id))
        {
            return t.clone();
        }
        self.next += 1;
        let latest = a.versions.last().map_or(1, |v| v.version);
        let tab = bc::Tab {
            id: format!("artifact-{}", self.next),
            owner: bc::Owner::Person,
            title: a.title.clone(),
            url: view_url(a, latest),
            state: bc::TabState::Ready,
            held_by: None,
            hold_generation: 0,
            computer: bc::Computer::this_mac(),
            ask: None,
            dialog: None,
            certificate: None,
            kind: bc::TabKind::Artifact,
            artifact: Some(bc::ArtifactRef {
                id: a.id.clone(),
                version: latest,
                latest: true,
            }),
        };
        self.tabs.push(tab.clone());
        tab
    }

    /// Pins a tab to `version`, or (`None`) follows the latest again.
    pub fn pin(
        &mut self,
        tab: &str,
        version: Option<u32>,
        a: &Artifact,
    ) -> Result<bc::Tab, Refused> {
        let t = self
            .tabs
            .iter_mut()
            .find(|t| t.id == tab)
            .ok_or_else(no_tab)?;
        let latest = a.versions.last().map_or(1, |v| v.version);
        let v = version.unwrap_or(latest);
        if !a.versions.iter().any(|x| x.version == v) {
            return Err(Refused::new(404, "not_found", "There's no such version."));
        }
        t.artifact = Some(bc::ArtifactRef {
            id: a.id.clone(),
            version: v,
            latest: version.is_none(),
        });
        t.url = view_url(a, v);
        Ok(t.clone())
    }

    pub fn close(&mut self, tab: &str) -> Result<(), Refused> {
        let before = self.tabs.len();
        self.tabs.retain(|t| t.id != tab);
        if self.tabs.len() == before {
            return Err(no_tab());
        }
        Ok(())
    }

    /// Tabs following the latest move to a new version; every tab takes the artifact's title (a
    /// rename); a deleted artifact's tab closes.
    pub fn refresh(&mut self, read: impl Fn(&str) -> Option<Artifact>) {
        self.tabs.retain_mut(|t| {
            let Some(r) = t.artifact.clone() else {
                return true;
            };
            let Some(a) = read(&r.id) else {
                return false;
            };
            t.title = a.title.clone();
            if r.latest {
                let latest = a.versions.last().map_or(r.version, |v| v.version);
                t.artifact = Some(bc::ArtifactRef {
                    version: latest,
                    ..r
                });
                t.url = view_url(&a, latest);
            }
            true
        });
    }

    /// A page tab's verb (take, give back, stream, input, a secret, a way out) on an artifact tab:
    /// never. The app draws it; nobody drives it.
    pub fn page_verb(&self, tab: &str, rules: &Rules) -> Result<(), Refused> {
        let t = self.tabs.iter().find(|t| t.id == tab).ok_or_else(no_tab)?;
        if rules.tabkind && t.kind == bc::TabKind::Artifact {
            return Err(Refused::new(
                409,
                "artifact_tab",
                "An artifact tab is the app's own drawing: it isn't streamed, taken or driven.",
            ));
        }
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════════════════════════════════
// 2. The lab: the view server, Chrome over the DevTools protocol, the nodes.
// ════════════════════════════════════════════════════════════════════════════════════════════

pub const NODES: &[(&str, &str)] = &[
    ("lab.artifactview.up", "Starts the view server on 127.0.0.1 (a free port) over the artifacts' store (`artifacts`: a lab.artifacts.up step's output): each version is the origin <id>-v<n>.localhost:<port>, and the artifact tabs' routes answer on 127.0.0.1:<port>. {port, token, api}. One per workflow."),
    ("lab.artifactview.device", "Registers a new device key of the owner's with the view server: {id, key, publicKey}."),
    ("lab.artifactview.url", "The address a version's file has in the view (`id`, `version`, `path`; `check` for a check's folder): {url, origin}."),
    ("lab.artifactview.render", "Opens a view's file in chrome-headless-shell at `width`×`height` (the page `name`, default main), the app's navigation rule on every request, waits until it is drawn (`ready`: a script; by default the runtime's or the canvas host's), and saves a picture: {ready, errors, console, size, fits (against `within`), picture, url}. Skipped without chrome-headless-shell."),
    ("lab.artifactview.eval", "Runs `expression` in a page (`name`), awaiting a promise: {value, error}; `waitMs` first; with expect."),
    ("lab.artifactview.navigation", "What the app did with a page's requests outside its version (`name`): {asked, denied, allowed}; with expect."),
    ("lab.artifactview.compare", "Compares two pictures (`a`, `b`, PNG): {width, height, differing (pixels past `tolerance`, default 16 a channel), ratio}; with expect."),
    ("lab.artifactview.vsclaude", "Draws one board (`id`, `version`, `path`, `width`, `height`) with Oby's runtime and with Claude's (`OBID_SPEC_CLAUDE_DC_RUNTIME`: a copy of Claude Design's dc-runtime.js, which this repository never holds), and compares the pictures: {ratio, differing, within (`most`, default 0.02), oby, claude}. Skipped without the variable."),
    ("lab.artifactview.check", "oby_artifact_check on an agent's folder (`path`): draws it (`board`, else every board, or the page), at `width`, and answers what the tool will: {ok, picture, height, errors: [{board, message}], console}; with expect."),
    ("lab.artifactview.outside", "A server outside every view (127.0.0.1, a free port) that counts what reaches it: {url, hits}; again with `outside` to read the count."),
    ("lab.artifactview.call", "One request to the artifact tabs' routes: `method`, `path`, `body`, `as` (owner, device, unsigned, none, wrong). {status, body}; with expect."),
    ("lab.artifactview.restart", "Stops the view server and starts it again on the same store (the tabs as it kept them): the new {port, token, api}."),
    ("lab.artifactview.down", "Stops the view server and Chrome now rather than at the cleanup: {stopped}."),
];

fn rules_here() -> Rules {
    match std::env::var("OBID_SPEC_LAB_ARTIFACTVIEW_BREAK") {
        Ok(names) if !names.is_empty() => Rules::broken(&names),
        _ => Rules::ALL,
    }
}

/// A version a view serves (a stored one, or a folder an agent asked `oby_artifact_check` about),
/// read once: a version never changes. What it serves is kept too.
struct Mount {
    kind: Kind,
    version: Version,
    files: Files,
    served: Mutex<HashMap<String, Option<Served>>>,
}

impl Mount {
    fn new(kind: Kind, version: Version, files: Files) -> Arc<Mount> {
        Arc::new(Mount {
            kind,
            version,
            files,
            served: Mutex::new(HashMap::new()),
        })
    }

    fn serve(&self, path: &str, rules: &Rules) -> Option<Served> {
        let mut served = self.served.lock().unwrap();
        served
            .entry(path.to_string())
            .or_insert_with(|| serve(self.kind, &self.version, &self.files, path, rules))
            .clone()
    }
}

/// The view server's state: the store's folder, the artifact tabs, the owner's devices, and the
/// folders being checked.
struct View {
    store: PathBuf,
    tabs_file: PathBuf,
    rules: Rules,
    token: String,
    words: store::Words,
    tabs: ArtifactTabs,
    /// Each version served so far, and each check's folder, by its host's label.
    mounts: HashMap<String, Arc<Mount>>,
}

impl View {
    fn save_tabs(&self) {
        if self.rules.persist {
            if let Ok(text) = serde_json::to_vec_pretty(&self.tabs) {
                let _ = std::fs::write(&self.tabs_file, text);
            }
        }
    }

    fn refresh(&mut self) {
        let dir = self.store.clone();
        self.tabs.refresh(|id| store::read_artifact(&dir, id));
    }

    /// The version a view's host names: `<id>-v<n>` or `check-<n>`.
    fn version_of(&mut self, label: &str) -> Option<Arc<Mount>> {
        if let Some(m) = self.mounts.get(label) {
            return Some(m.clone());
        }
        let (id, n) = label.rsplit_once("-v")?;
        let (a, v, files) = store::read_version(&self.store, id, n.parse().ok()?)?;
        let m = Mount::new(a.kind, v, files);
        self.mounts.insert(label.to_string(), m.clone());
        Some(m)
    }
}

struct Server {
    view: Arc<Mutex<View>>,
    port: u16,
    accept: tokio::task::JoinHandle<()>,
}

static SERVER: Mutex<Option<Server>> = Mutex::new(None);
static CHROME: std::sync::OnceLock<tokio::sync::Mutex<Option<Chrome>>> = std::sync::OnceLock::new();

fn chrome_slot() -> &'static tokio::sync::Mutex<Option<Chrome>> {
    CHROME.get_or_init(|| tokio::sync::Mutex::new(None))
}
/// The server outside every view: its port, how many requests reached it, its task.
type Outside = (u16, Arc<AtomicU64>, tokio::task::JoinHandle<()>);
static OUTSIDE: Mutex<Option<Outside>> = Mutex::new(None);

fn view() -> Result<(Arc<Mutex<View>>, u16)> {
    SERVER
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| (s.view.clone(), s.port))
        .context("the view server isn't up: a lab.artifactview.up step first")
}

struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

async fn read_request(conn: &mut tokio::net::TcpStream) -> Result<Request> {
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 16 * 1024];
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
    anyhow::ensure!(len <= 1024 * 1024, "the body is too long");
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

fn json_answer(status: u16, body: &Value) -> Answer {
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
    json_answer(r.status, &r.body())
}

async fn write_answer(conn: &mut tokio::net::TcpStream, a: Answer) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        a.status,
        if a.status < 300 { "OK" } else { "Refused" },
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

/// A file of a version's view, for the origin its host names.
fn view_file(view: &Mutex<View>, label: &str, path: &str) -> Answer {
    let not_found = || refused(Refused::new(404, "not_found", "There's no such file."));
    // `<label>-claude`: the same version with Claude's own runtime as its support.js, only when
    // OBID_SPEC_CLAUDE_DC_RUNTIME names a copy of it (never in this repository), to compare pictures.
    let (label, claude) = match label.strip_suffix("-claude") {
        Some(l) => (l, std::env::var_os("OBID_SPEC_CLAUDE_DC_RUNTIME")),
        None => (label, None),
    };
    if let Some(runtime) = &claude {
        if path.split('?').next().unwrap_or("").trim_start_matches('/') == "support.js" {
            return match std::fs::read(runtime) {
                Ok(body) => Answer {
                    status: 200,
                    headers: vec![
                        (
                            "Content-Type".into(),
                            "text/javascript; charset=utf-8".into(),
                        ),
                        ("Content-Security-Policy".into(), CLAUDE_POLICY.into()),
                        ("Access-Control-Allow-Origin".into(), "*".into()),
                    ],
                    body,
                },
                Err(_) => not_found(),
            };
        }
    }
    let (mount, rules) = {
        let mut v = view.lock().unwrap();
        let rules = v.rules;
        (v.version_of(label), rules)
    };
    let Some(mount) = mount else {
        return not_found();
    };
    let Some(path) =
        store::percent_decode(path.split('?').next().unwrap_or("").trim_start_matches('/'))
    else {
        return not_found();
    };
    match mount.serve(&path, &rules) {
        Some((kind, policy, body)) => {
            let mut headers = vec![
                ("Content-Type".into(), kind),
                ("X-Content-Type-Options".into(), "nosniff".into()),
                ("Referrer-Policy".into(), "no-referrer".into()),
            ];
            if let Some(p) = policy {
                // Claude's runtime evaluates strings as code, which obid's CSP refuses: only its
                // comparison origin allows it, so the two pictures can be compared at all.
                let p = if claude.is_some() { CLAUDE_POLICY } else { p };
                headers.push(("Content-Security-Policy".into(), p.into()));
            }
            // A sandboxed board's origin is its own (`null`), so its runtime and logic load with
            // CORS: a mistake in them is then said with its file's name, not as "Script error.".
            if path == "support.js" || path.ends_with("/support.js") || path.starts_with("__dc/") {
                headers.push(("Access-Control-Allow-Origin".into(), "*".into()));
            }
            Answer {
                status: 200,
                headers,
                body,
            }
        }
        None => not_found(),
    }
}

/// The artifact tabs' routes, on 127.0.0.1:<port> with the owner's token.
fn tabs_route(v: &mut View, req: &Request) -> Answer {
    let path = req.path.split('?').next().unwrap_or("");
    let bearer = req
        .headers
        .get("authorization")
        .and_then(|a| a.strip_prefix("Bearer "));
    match bearer {
        None => return refused(Refused::new(401, "no_token", "obid's token is needed.")),
        Some(t) if t != v.token => {
            return refused(Refused::new(401, "bad_token", "That isn't obid's token."))
        }
        _ => {}
    }
    v.refresh();
    if req.method == "GET" && path == bc::PATH_BROWSER {
        let answer = bc::TabsAnswer {
            tabs: v.tabs.tabs.clone(),
            settings: bc::Settings::default(),
            browsers: vec![],
            seq: 0,
        };
        return json_answer(200, &serde_json::to_value(answer).unwrap_or_default());
    }
    let Some(rest) = path.strip_prefix(bc::PATH_TABS) else {
        return refused(Refused::new(404, "not_found", "There's no such route."));
    };
    let signed = req.method == "POST";
    if signed {
        if let Err(e) = v.words.verify(
            req.headers
                .get(&bc::WORDS_HEADER.to_ascii_lowercase())
                .map(String::as_str),
            &req.method,
            path,
            &req.body,
            chrono::Utc::now().timestamp_millis(),
            &store::Rules::ALL,
        ) {
            return json_answer(e.status, &e.body());
        }
    }
    let parts: Vec<&str> = rest.trim_start_matches('/').split('/').collect();
    let answer = match (req.method.as_str(), parts.as_slice()) {
        ("POST", [""]) => {
            let body: bc::OpenArtifactBody = match serde_json::from_slice(&req.body) {
                Ok(b) => b,
                Err(e) => {
                    return refused(Refused::new(
                        400,
                        "bad_request",
                        format!("The body is {{\"artifact\": \"<id>\"}} ({e})."),
                    ))
                }
            };
            match store::read_artifact(&v.store, &body.artifact) {
                Some(a) => json_answer(
                    200,
                    &serde_json::to_value(v.tabs.open(&a)).unwrap_or_default(),
                ),
                None => refused(Refused::new(404, "not_found", "There's no such artifact.")),
            }
        }
        ("POST", [tab, verb]) if *verb == bc::VERB_VERSION => {
            let body: bc::VersionBody = match serde_json::from_slice(&req.body) {
                Ok(b) => b,
                Err(e) => {
                    return refused(Refused::new(
                        400,
                        "bad_request",
                        format!("The body is {{\"version\": n}} ({e})."),
                    ))
                }
            };
            let Some(id) = v
                .tabs
                .tabs
                .iter()
                .find(|t| t.id == *tab)
                .and_then(|t| t.artifact.as_ref().map(|r| r.id.clone()))
            else {
                return refused(no_tab());
            };
            let Some(a) = store::read_artifact(&v.store, &id) else {
                return refused(Refused::new(404, "not_found", "There's no such artifact."));
            };
            match v.tabs.pin(tab, body.version, &a) {
                Ok(t) => json_answer(200, &serde_json::to_value(t).unwrap_or_default()),
                Err(e) => refused(e),
            }
        }
        ("POST", [tab, verb]) if *verb == bc::VERB_CLOSE => match v.tabs.close(tab) {
            Ok(()) => json_answer(204, &Value::Null),
            Err(e) => refused(e),
        },
        (_, [tab, _verb]) => match v.tabs.page_verb(tab, &v.rules) {
            // Off, the artifact tab answers as a page tab's route would (the red proof).
            Ok(()) => json_answer(200, &json!({"tab": tab, "answeredAsAPage": true})),
            Err(e) => refused(e),
        },
        _ => refused(Refused::new(404, "not_found", "There's no such route.")),
    };
    v.save_tabs();
    answer
}

fn handle(view: &Mutex<View>, req: &Request, port: u16) -> Answer {
    let host = req.headers.get("host").cloned().unwrap_or_default();
    let label = host
        .strip_suffix(&format!(".localhost:{port}"))
        .map(str::to_string);
    match label {
        Some(label) if req.method == "GET" => view_file(view, &label, &req.path),
        Some(_) => refused(Refused::new(404, "not_found", "A view only reads.")),
        None => tabs_route(&mut view.lock().unwrap(), req),
    }
}

async fn serve_loop(listener: tokio::net::TcpListener, view: Arc<Mutex<View>>, port: u16) {
    loop {
        let Ok((mut conn, _)) = listener.accept().await else {
            continue;
        };
        let view = view.clone();
        tokio::spawn(async move {
            let Ok(req) = read_request(&mut conn).await else {
                return;
            };
            let Ok(answer) = tokio::task::spawn_blocking(move || handle(&view, &req, port)).await
            else {
                return;
            };
            write_answer(&mut conn, answer).await;
        });
    }
}

async fn start(store_dir: PathBuf, tabs_file: PathBuf) -> Result<Value> {
    let rules = rules_here();
    let tabs = if rules.persist {
        std::fs::read(&tabs_file)
            .ok()
            .and_then(|t| serde_json::from_slice(&t).ok())
            .unwrap_or_default()
    } else {
        ArtifactTabs::default()
    };
    let token = hex::encode(rand::random::<[u8; 16]>());
    let view = Arc::new(Mutex::new(View {
        store: store_dir,
        tabs_file,
        rules,
        token: token.clone(),
        words: store::Words::default(),
        tabs,
        mounts: HashMap::new(),
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let accept = tokio::spawn(serve_loop(listener, view.clone(), port));
    *SERVER.lock().unwrap() = Some(Server { view, port, accept });
    Ok(json!({"port": port, "token": token, "api": {"port": port, "token": token}}))
}

async fn up(input: &Value) -> Result<Value> {
    if let Ok((v, port)) = view() {
        let token = v.lock().unwrap().token.clone();
        return Ok(json!({"port": port, "token": token, "api": {"port": port, "token": token}}));
    }
    let home = PathBuf::from(
        input["artifacts"]["home"]
            .as_str()
            .context("artifacts: a lab.artifacts.up step's output")?,
    );
    let browser = home.join("browser");
    crate::obid::private_dir(&browser)?;
    start(home.join("artifacts"), browser.join("artifact-tabs.json")).await
}

async fn stop_server() -> bool {
    let server = SERVER.lock().unwrap().take();
    match server {
        Some(s) => {
            s.accept.abort();
            let _ = s.accept.await;
            true
        }
        None => false,
    }
}

async fn restart() -> Result<Value> {
    let (store_dir, tabs_file) = {
        let (v, _) = view()?;
        let v = v.lock().unwrap();
        (v.store.clone(), v.tabs_file.clone())
    };
    stop_server().await;
    start(store_dir, tabs_file).await
}

// ── Chrome over the DevTools protocol ───────────────────────────────────────────────────────

type Pending =
    Mutex<HashMap<u64, tokio::sync::oneshot::Sender<std::result::Result<Value, String>>>>;

struct Cdp {
    next: AtomicU64,
    pending: Pending,
    /// Every event, with the session it came on.
    events: Mutex<Vec<Value>>,
    /// Each page's session → its view's origin, for the navigation rule.
    origins: Mutex<HashMap<String, String>>,
    /// What the rule did: {session, url, document, decision}.
    decisions: Mutex<Vec<Value>>,
    rules: Rules,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
}

impl Cdp {
    fn send_now(&self, session: Option<&str>, method: &str, params: Value) -> u64 {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let _ = self.tx.send(msg.to_string());
        id
    }

    async fn call(&self, session: Option<&str>, method: &str, params: Value) -> Result<Value> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        self.pending.lock().unwrap().insert(id, tx);
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        self.tx
            .send(msg.to_string())
            .ok()
            .context("Chrome's connection is closed")?;
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => bail!("{method}: {e}"),
            Ok(Err(_)) => bail!("{method}: Chrome's connection closed"),
            Err(_) => bail!("{method}: Chrome answered nothing within 60 s"),
        }
    }

    /// The app's rule, applied to a request Chrome paused.
    fn paused(&self, event: &Value) {
        let session = event["sessionId"].as_str().unwrap_or_default().to_string();
        let p = &event["params"];
        let id = p["requestId"].clone();
        let url = p["request"]["url"].as_str().unwrap_or_default().to_string();
        let document = p["resourceType"] == "Document";
        let origin = self
            .origins
            .lock()
            .unwrap()
            .get(&session)
            .cloned()
            .unwrap_or_default();
        let decision = navigation(&origin, &url, document, &self.rules);
        let (method, params) = match decision {
            Nav::Allow => ("Fetch.continueRequest", json!({"requestId": id})),
            // A page that would leave is answered 204 No Content: the browser stays where it was,
            // as Electron's will-navigate stopped it.
            _ if document => (
                "Fetch.fulfillRequest",
                json!({"requestId": id, "responseCode": 204, "responseHeaders": []}),
            ),
            _ => (
                "Fetch.failRequest",
                json!({"requestId": id, "errorReason": "BlockedByClient"}),
            ),
        };
        self.decisions.lock().unwrap().push(json!({
            "session": session, "url": url, "document": document,
            "decision": format!("{decision:?}").to_lowercase(),
        }));
        self.send_now(Some(&session), method, params);
    }
}

struct Chrome {
    child: tokio::process::Child,
    cdp: Arc<Cdp>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    /// Each page by name: its session.
    pages: HashMap<String, String>,
    pictures: PathBuf,
    /// Its profile folder: every process of this Chrome has it in its command line.
    dir: PathBuf,
}

/// chrome-headless-shell: `OBID_SPEC_CHROME`, else the lab's pinned one (installed once).
async fn chrome_path() -> Result<std::result::Result<PathBuf, String>> {
    if let Some(p) = std::env::var_os("OBID_SPEC_CHROME") {
        return Ok(Ok(PathBuf::from(p)));
    }
    super::browser::install_shell().await
}

async fn launch(run: &Run) -> Result<std::result::Result<Chrome, String>> {
    let exe = match chrome_path().await? {
        Ok(p) => p,
        Err(why) => return Ok(Err(why)),
    };
    let dir = run.scratch.join("artifactview-chrome");
    crate::obid::private_dir(&dir)?;
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", dir.display()))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--hide-scrollbars",
            "--mute-audio",
            "--disable-gpu",
            "--force-color-profile=srgb",
            "--font-render-hinting=none",
            "about:blank",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if cfg!(target_os = "linux") {
        // The lab's pages are its own fixtures; Ubuntu's AppArmor refuses Chrome's sandbox here.
        cmd.arg("--no-sandbox");
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("{} didn't start", exe.display()))?;
    let stderr = child.stderr.take().context("Chrome's stderr")?;
    let mut lines = tokio::io::BufReader::new(stderr).lines();
    let ws = tokio::time::timeout(Duration::from_secs(30), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(at) = line.find("ws://") {
                return Some(line[at..].trim().to_string());
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .context("Chrome didn't say where its DevTools listen")?;
    // Keep reading Chrome's stderr so it never blocks on a full pipe.
    let drain = tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    let (socket, _) = tokio_tungstenite::connect_async(ws.as_str())
        .await
        .context("Chrome's DevTools didn't take the connection")?;
    let (mut write, mut read) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let cdp = Arc::new(Cdp {
        next: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        events: Mutex::new(vec![]),
        origins: Mutex::new(HashMap::new()),
        decisions: Mutex::new(vec![]),
        rules: rules_here(),
        tx,
    });
    let writer = tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if write
                .send(tokio_tungstenite::tungstenite::Message::Text(text))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let reading = cdp.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            let tokio_tungstenite::tungstenite::Message::Text(text) = msg else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if let Some(id) = v["id"].as_u64() {
                if let Some(tx) = reading.pending.lock().unwrap().remove(&id) {
                    let answer = match v.get("error") {
                        Some(e) => Err(e["message"].as_str().unwrap_or("an error").to_string()),
                        None => Ok(v["result"].clone()),
                    };
                    let _ = tx.send(answer);
                }
                continue;
            }
            if v["method"] == "Fetch.requestPaused" {
                reading.paused(&v);
                continue;
            }
            reading.events.lock().unwrap().push(v);
        }
    });
    let pictures = run.scratch.join("artifactview-pictures");
    std::fs::create_dir_all(&pictures)?;
    Ok(Ok(Chrome {
        child,
        cdp,
        tasks: vec![drain, writer, reader],
        pages: HashMap::new(),
        pictures,
        dir,
    }))
}

impl Chrome {
    async fn page(&mut self, name: &str, origin: &str, width: u64, height: u64) -> Result<String> {
        let session = match self.pages.get(name) {
            Some(s) => s.clone(),
            None => {
                let target = self
                    .cdp
                    .call(None, "Target.createTarget", json!({"url": "about:blank"}))
                    .await?;
                let attached = self
                    .cdp
                    .call(
                        None,
                        "Target.attachToTarget",
                        json!({"targetId": target["targetId"], "flatten": true}),
                    )
                    .await?;
                let s = attached["sessionId"]
                    .as_str()
                    .context("a session")?
                    .to_string();
                for m in ["Page.enable", "Runtime.enable", "Log.enable"] {
                    self.cdp.call(Some(&s), m, json!({})).await?;
                }
                self.cdp
                    .call(
                        Some(&s),
                        "Fetch.enable",
                        json!({"patterns": [{"urlPattern": "*", "requestStage": "Request"}]}),
                    )
                    .await?;
                self.pages.insert(name.into(), s.clone());
                s
            }
        };
        self.cdp
            .origins
            .lock()
            .unwrap()
            .insert(session.clone(), origin.to_string());
        self.cdp
            .call(
                Some(&session),
                "Emulation.setDeviceMetricsOverride",
                json!({"width": width, "height": height, "deviceScaleFactor": 1, "mobile": false}),
            )
            .await?;
        Ok(session)
    }

    async fn eval(
        &self,
        session: &str,
        expression: &str,
    ) -> Result<std::result::Result<Value, String>> {
        let r = self
            .cdp
            .call(
                Some(session),
                "Runtime.evaluate",
                json!({"expression": expression, "awaitPromise": true, "returnByValue": true}),
            )
            .await?;
        if let Some(e) = r.get("exceptionDetails") {
            let text = e["exception"]["description"]
                .as_str()
                .or(e["text"].as_str())
                .unwrap_or("it threw")
                .to_string();
            return Ok(Err(text));
        }
        Ok(Ok(r["result"]["value"].clone()))
    }

    async fn picture(&self, session: &str, file: &str) -> Result<PathBuf> {
        let r = self
            .cdp
            .call(
                Some(session),
                "Page.captureScreenshot",
                json!({"format": "png"}),
            )
            .await?;
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(r["data"].as_str().context("the picture")?)?;
        let path = self.pictures.join(file);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    /// The page's console errors and thrown errors, and the CSP's refusals, since `from`.
    fn said(&self, session: &str, from: usize) -> Vec<String> {
        let events = self.cdp.events.lock().unwrap();
        events
            .iter()
            .skip(from)
            .filter(|e| e["sessionId"] == session)
            .filter_map(|e| {
                let p = &e["params"];
                match e["method"].as_str()? {
                    "Runtime.exceptionThrown" => Some(
                        p["exceptionDetails"]["exception"]["description"]
                            .as_str()
                            .or(p["exceptionDetails"]["text"].as_str())?
                            .to_string(),
                    ),
                    "Runtime.consoleAPICalled" if p["type"] == "error" => Some(
                        p["args"]
                            .as_array()?
                            .iter()
                            .map(|a| {
                                a["value"].as_str().map(str::to_string).unwrap_or_else(|| {
                                    a["description"].as_str().unwrap_or("").to_string()
                                })
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                    "Log.entryAdded" if p["entry"]["level"] == "error" => {
                        Some(p["entry"]["text"].as_str()?.to_string())
                    }
                    _ => None,
                }
            })
            .collect()
    }
}

async fn chrome(
    run: &Run,
) -> Result<std::result::Result<tokio::sync::MutexGuard<'static, Option<Chrome>>, String>> {
    let mut guard = chrome_slot().lock().await;
    if guard.is_none() {
        match launch(run).await? {
            Ok(c) => *guard = Some(c),
            Err(why) => return Ok(Err(why)),
        }
    }
    Ok(Ok(guard))
}

fn origin_of(port: u16, label: &str) -> String {
    format!("http://{label}.localhost:{port}")
}

fn label_of(input: &Value) -> Result<String> {
    if let Some(c) = input["check"].as_str() {
        return Ok(c.to_string());
    }
    let id = input["id"].as_str().context("id (or check)")?;
    let version = input["version"].as_u64().context("version")?;
    Ok(format!("{id}-v{version}"))
}

/// What the runtime or the canvas host says when its page is drawn.
const READY: &str = "(window.__OBY_CANVAS ? window.__OBY_CANVAS.ready === true : window.__OBY_DC ? window.__OBY_DC.ready === true : document.readyState === 'complete')";

async fn open(run: &Run, input: &Value, label: &str, path: &str) -> Result<Value> {
    let (_, port) = view()?;
    open_at(run, input, &origin_of(port, label), path).await
}

/// A page of `origin` (a version's view, or the Sphere's public link page) in Chrome, the app's
/// navigation rule holding it to that origin: drawn, then pictured once still.
pub(crate) async fn open_at(run: &Run, input: &Value, origin: &str, path: &str) -> Result<Value> {
    let mut guard = match chrome(run).await? {
        Ok(g) => g,
        Err(why) => return Ok(json!({"skipped": why})),
    };
    let c = guard.as_mut().context("Chrome")?;
    let name = input["name"].as_str().unwrap_or("main").to_string();
    let width = input["width"].as_u64().unwrap_or(1280);
    let height = input["height"].as_u64().unwrap_or(800);
    let session = c.page(&name, origin, width, height).await?;
    let from = c.cdp.events.lock().unwrap().len();
    let url = format!("{origin}/{}", path.trim_start_matches('/'));
    c.cdp
        .call(Some(&session), "Page.navigate", json!({"url": url}))
        .await?;
    let ready = input["ready"].as_str().unwrap_or(READY);
    let until =
        Instant::now() + Duration::from_millis(input["timeoutMs"].as_u64().unwrap_or(20_000));
    let mut drawn = false;
    while Instant::now() < until {
        if let Ok(Ok(Value::Bool(true))) = c.eval(&session, ready).await {
            drawn = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let state = c
        .eval(
            &session,
            "({errors: (window.__OBY_DC && window.__OBY_DC.errors) || (window.__OBY_CANVAS && window.__OBY_CANVAS.errors) || [], \
              size: {width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight}, \
              title: document.title, playing: window.__OBY_DC ? window.__OBY_DC.playing : null, \
              frames: window.__OBY_CANVAS ? window.__OBY_CANVAS.frames : null})",
        )
        .await?
        .unwrap_or(Value::Null);
    // A picture of the page once it is still: its animations that end have ended (at most 3 s; a
    // pulse that never ends is taken as it is).
    let _ = c
        .eval(
            &session,
            "Promise.race([Promise.all(document.getAnimations().filter(a => a.effect && a.effect.getComputedTiming().endTime !== Infinity).map(a => a.finished.catch(() => null))), new Promise(r => setTimeout(r, 3000))]).then(() => true)",
        )
        .await?;
    // Never the part after `#`: a public link's key is there.
    let shot = format!(
        "{}-{}.png",
        name,
        path.split('#')
            .next()
            .unwrap_or("")
            .replace(['/', '?', '=', '&'], "_")
    );
    let picture = c.picture(&session, &shot).await?;
    let fits = match (
        input["within"]["width"].as_u64(),
        input["within"]["height"].as_u64(),
    ) {
        (Some(w), Some(h)) => json!(
            state["size"]["width"].as_u64().unwrap_or(u64::MAX) <= w
                && state["size"]["height"].as_u64().unwrap_or(u64::MAX) <= h
        ),
        _ => Value::Null,
    };
    Ok(json!({
        "ready": drawn,
        "url": url.split('#').next().unwrap_or(""),
        "errors": state["errors"],
        "size": state["size"],
        "title": state["title"],
        "playing": state["playing"],
        "frames": state["frames"],
        "fits": fits,
        "console": c.said(&session, from),
        "picture": picture,
    }))
}

async fn eval_node(run: &Run, input: &Value) -> Result<Value> {
    if let Some(ms) = input["waitMs"].as_u64() {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
    let mut guard = match chrome(run).await? {
        Ok(g) => g,
        Err(why) => return Ok(json!({"skipped": why})),
    };
    let c = guard.as_mut().context("Chrome")?;
    let name = input["name"].as_str().unwrap_or("main");
    let session = c
        .pages
        .get(name)
        .cloned()
        .context("no such page: a lab.artifactview.render step first")?;
    let expression = input["expression"].as_str().context("expression")?;
    Ok(match c.eval(&session, expression).await? {
        Ok(v) => json!({"value": v, "error": Value::Null}),
        Err(e) => json!({"value": Value::Null, "error": e}),
    })
}

async fn navigation_node(run: &Run, input: &Value) -> Result<Value> {
    let guard = match chrome(run).await? {
        Ok(g) => g,
        Err(why) => return Ok(json!({"skipped": why})),
    };
    let c = guard.as_ref().context("Chrome")?;
    let name = input["name"].as_str().unwrap_or("main");
    let session = c.pages.get(name).cloned().unwrap_or_default();
    let (_, port) = view()?;
    let ours = format!(".localhost:{port}/");
    let mut out = json!({"asked": [], "denied": [], "allowed": []});
    for d in c.cdp.decisions.lock().unwrap().iter() {
        if d["session"] != session.as_str() {
            continue;
        }
        let url = d["url"].as_str().unwrap_or_default();
        let key = match d["decision"].as_str() {
            Some("ask") => "asked",
            Some("deny") => "denied",
            // The view's own files are many and expected: only what left it is listed.
            _ if url.contains(&ours) => continue,
            _ => "allowed",
        };
        out[key].as_array_mut().unwrap().push(json!(url));
    }
    Ok(out)
}

fn rgba(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(
        std::fs::File::open(path).with_context(|| format!("can't open {}", path.display()))?,
    );
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let pixels = &buf[..info.buffer_size()];
    let out = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => bail!("{} is {other:?}, not RGB(A)", path.display()),
    };
    Ok((info.width, info.height, out))
}

/// How many pixels of two pictures differ by more than `tolerance` in a channel.
pub fn differing(a: &[u8], b: &[u8], tolerance: u8) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .any(|(x, y)| x.abs_diff(*y) > tolerance)
        })
        .count()
}

fn compare(input: &Value) -> Result<Value> {
    let a = rgba(Path::new(input["a"].as_str().context("a")?))?;
    let b = rgba(Path::new(input["b"].as_str().context("b")?))?;
    if (a.0, a.1) != (b.0, b.1) {
        return Ok(
            json!({"width": a.0, "height": a.1, "otherSize": [b.0, b.1], "ratio": 1.0, "differing": Value::Null}),
        );
    }
    let tolerance = input["tolerance"].as_u64().unwrap_or(16).min(255) as u8;
    let n = differing(&a.2, &b.2, tolerance);
    let ratio = n as f64 / (a.0 as f64 * a.1 as f64).max(1.0);
    Ok(
        json!({"width": a.0, "height": a.1, "differing": n, "ratio": ratio, "within": ratio <= input["most"].as_f64().unwrap_or(0.01)}),
    )
}

/// One board drawn by Oby's runtime and by Claude's, the pictures compared.
async fn vs_claude(run: &Run, input: &Value) -> Result<Value> {
    if std::env::var_os("OBID_SPEC_CLAUDE_DC_RUNTIME").is_none() {
        return Ok(
            json!({"skipped": "OBID_SPEC_CLAUDE_DC_RUNTIME names no copy of Claude Design's runtime: nothing to compare with"}),
        );
    }
    let label = label_of(input)?;
    let path = input["path"].as_str().context("path")?;
    let mut oby = input.clone();
    oby["name"] = json!(format!("oby-{path}"));
    let ours = open(run, &oby, &label, path).await?;
    if ours.get("skipped").is_some() {
        return Ok(ours);
    }
    // Claude's runtime says nothing when it is done: its page is given the time it takes.
    let mut theirs_in = input.clone();
    theirs_in["name"] = json!(format!("claude-{path}"));
    theirs_in["ready"] =
        json!("new Promise(r => setTimeout(() => r(document.readyState === 'complete'), 3000))");
    let theirs = open(run, &theirs_in, &format!("{label}-claude"), path).await?;
    let mut cmp = compare(
        &json!({"a": ours["picture"], "b": theirs["picture"], "tolerance": input["tolerance"], "most": input["most"]}),
    )?;
    cmp["oby"] = ours["picture"].clone();
    cmp["claude"] = theirs["picture"].clone();
    cmp["obyErrors"] = ours["errors"].clone();
    cmp["claudeConsole"] = theirs["console"].clone();
    Ok(cmp)
}

static CHECKS: AtomicU64 = AtomicU64::new(0);

/// `oby_artifact_check`: the agent's folder drawn as it would be published, and what went wrong
/// on each board, by its name.
async fn check(run: &Run, input: &Value) -> Result<Value> {
    let path = PathBuf::from(input["path"].as_str().context("path")?);
    let (v, _) = view()?;
    let rules = v.lock().unwrap().rules;
    let mut files = match store::read_source(&path, &store::Rules::ALL) {
        Ok((f, _)) => f,
        Err(e) => return Ok(json!({"ok": false, "error": e.message})),
    };
    let folder = path.is_dir();
    let kind = match store::kind_of(&files, folder, None, &store::Rules::ALL) {
        Ok(k) => k,
        Err(e) => return Ok(json!({"ok": false, "error": e.message})),
    };
    let blobs = if kind == Kind::Design {
        store::ingest_blobs(&mut files, &store::Rules::ALL)
    } else {
        BTreeMap::new()
    };
    let label = format!("check-{}", CHECKS.fetch_add(1, Ordering::SeqCst) + 1);
    let version = Version {
        version: 1,
        created: String::new(),
        files: files.keys().cloned().collect(),
        bytes: files.values().map(|b| b.len() as u64).sum(),
        blobs,
        job: None,
        by: None,
    };
    let boards: Vec<String> = match input["board"].as_str() {
        Some(b) => vec![b.to_string()],
        None if kind == Kind::Design => files.keys().filter(|p| is_board(p)).cloned().collect(),
        None => vec![version.files.first().cloned().unwrap_or_default()],
    };
    v.lock()
        .unwrap()
        .mounts
        .insert(label.clone(), Mount::new(kind, version, files));
    let width = input["width"].as_u64().unwrap_or(1280);
    let mut errors = vec![];
    let mut console = vec![];
    let mut pictures = vec![];
    let mut height = 0;
    for board in &boards {
        let out = open(
            run,
            &json!({"name": format!("{label}-{board}"), "width": width, "height": input["height"].as_u64().unwrap_or(800)}),
            &label,
            board,
        )
        .await?;
        if out.get("skipped").is_some() {
            return Ok(out);
        }
        if rules.errors {
            for e in out["errors"].as_array().into_iter().flatten() {
                errors.push(json!({"board": e["board"], "message": e["message"]}));
            }
            if out["ready"] != true {
                errors.push(json!({"board": board, "message": "it wasn't drawn within 20 s"}));
            }
        }
        for line in out["console"].as_array().into_iter().flatten() {
            console.push(line.clone());
        }
        height = height.max(out["size"]["height"].as_u64().unwrap_or(0));
        pictures.push(json!({"board": board, "picture": out["picture"], "bytes": std::fs::metadata(out["picture"].as_str().unwrap_or_default()).map(|m| m.len()).unwrap_or(0)}));
    }
    Ok(json!({
        "ok": errors.is_empty(),
        "kind": kind,
        "picture": pictures.first().map(|p| p["picture"].clone()),
        "pictures": pictures,
        "height": height,
        "errors": errors,
        "console": console,
    }))
}

async fn outside(input: &Value) -> Result<Value> {
    if input.get("outside").is_some() {
        let hits = OUTSIDE
            .lock()
            .unwrap()
            .as_ref()
            .map(|(_, h, _)| h.load(Ordering::SeqCst))
            .unwrap_or(0);
        return Ok(json!({"url": input["outside"]["url"], "hits": hits}));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let hits = Arc::new(AtomicU64::new(0));
    let counting = hits.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut conn, _)) = listener.accept().await else {
                continue;
            };
            counting.fetch_add(1, Ordering::SeqCst);
            let _ = read_request(&mut conn).await;
            write_answer(
                &mut conn,
                Answer {
                    status: 200,
                    headers: vec![
                        ("Content-Type".into(), "text/plain".into()),
                        ("Access-Control-Allow-Origin".into(), "*".into()),
                    ],
                    body: b"reached".to_vec(),
                },
            )
            .await;
        }
    });
    *OUTSIDE.lock().unwrap() = Some((port, hits, task));
    Ok(json!({"url": format!("http://127.0.0.1:{port}"), "hits": 0}))
}

pub async fn cleanup() -> Value {
    let server = stop_server().await;
    let mut left = 0;
    let chrome = match chrome_slot().lock().await.take() {
        Some(mut c) => {
            for t in &c.tasks {
                t.abort();
            }
            let _ = c.child.kill().await;
            // Its renderers go with it; a moment for them, then whatever is left, by its folder.
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                left = processes_of(&c.dir).await;
                if left == 0 || Instant::now() >= until {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            if left > 0 {
                let _ = tokio::process::Command::new("pkill")
                    .arg("-f")
                    .arg(c.dir.as_os_str())
                    .status()
                    .await;
            }
            true
        }
        None => false,
    };
    let outside = match OUTSIDE.lock().unwrap().take() {
        Some((_, _, t)) => {
            t.abort();
            true
        }
        None => false,
    };
    json!({"stopped": server, "chrome": chrome, "chromeLeft": left, "outside": outside})
}

/// How many processes have `dir` in their command line.
async fn processes_of(dir: &Path) -> usize {
    tokio::process::Command::new("pgrep")
        .arg("-f")
        .arg(dir.as_os_str())
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0)
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    // `within`: asked again until its expectation holds, for what a page does a moment later.
    let until = Instant::now() + Duration::from_secs(input["withinS"].as_u64().unwrap_or(0));
    loop {
        let out = run_once(run, name, input).await?;
        let holds = match input.get("expect") {
            Some(want) => crate::expect::check(want, &out, "").is_empty(),
            None => true,
        };
        if holds || Instant::now() >= until || out.get("skipped").is_some() {
            return Ok(out);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn run_once(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "lab.artifactview.up" => up(input).await,
        "lab.artifactview.device" => {
            let (v, _) = view()?;
            let key = crate::keys::DeviceKey::generate();
            let id = format!("device-{}", hex::encode(rand::random::<[u8; 4]>()));
            v.lock()
                .unwrap()
                .words
                .add(&id, &key.public())
                .map_err(|e| anyhow::anyhow!(e.message))?;
            Ok(json!({"id": id, "key": key.secret_hex(), "publicKey": key.public()}))
        }
        "lab.artifactview.url" => {
            let (_, port) = view()?;
            let label = label_of(input)?;
            let origin = origin_of(port, &label);
            Ok(
                json!({"origin": origin, "url": format!("{origin}/{}", input["path"].as_str().unwrap_or(""))}),
            )
        }
        "lab.artifactview.render" => {
            let label = label_of(input)?;
            open(run, input, &label, input["path"].as_str().unwrap_or("")).await
        }
        "lab.artifactview.eval" => eval_node(run, input).await,
        "lab.artifactview.navigation" => navigation_node(run, input).await,
        "lab.artifactview.compare" => compare(input),
        "lab.artifactview.vsclaude" => vs_claude(run, input).await,
        "lab.artifactview.check" => check(run, input).await,
        "lab.artifactview.outside" => outside(input).await,
        "lab.artifactview.call" => store::call(input).await,
        "lab.artifactview.restart" => restart().await,
        "lab.artifactview.down" => Ok(cleanup().await),
        _ => bail!("{name} isn't a lab.artifactview node"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "<!doctype html><html><head><script src=\"./support.js\"></script></head><body>\n<x-dc>\n<helmet><style>body{margin:0}</style></helmet>\n<div>{{title}}</div>\n</x-dc>\n<script type=\"text/x-dc\" data-dc-script data-props='{}'>\nclass Component extends DCLogic { renderVals() { return { title: 'Hi' }; } }\n</script>\n</body></html>";

    #[test]
    fn a_boards_tweaks_default_its_props() {
        let board = BOARD.replace(
            "data-props='{}'",
            "data-props='{\"start\": {\"editor\": \"enum\", \"default\": \"viewer\"}, \"title\": {\"default\": \"Ben&#39;s\"}, \"$preview\": {\"width\": 1}}'",
        );
        let d = board_parts(&board).defaults;
        assert_eq!(d.get("start"), Some(&json!("viewer")));
        assert_eq!(d.get("title"), Some(&json!("Ben's")));
        assert!(d.get("$preview").is_none());
    }

    #[test]
    fn a_boards_parts() {
        let p = board_parts(BOARD);
        assert_eq!(p.helmet, "<style>body{margin:0}</style>");
        assert!(p.template.contains("<div>{{title}}</div>") && !p.template.contains("helmet"));
        assert!(p.logic.unwrap().contains("class Component extends DCLogic"));
    }

    fn design() -> (Version, Files) {
        let files = Files::from([
            ("Main.dc.html".to_string(), BOARD.as_bytes().to_vec()),
            ("canvas.json".to_string(), br#"{"v":3,"boards":{"Main.dc.html":{"x":0,"y":0,"w":10,"h":10}},"notes":{"n":{"text":"</script><b>x</b>"}}}"#.to_vec()),
            ("assets/a.png".to_string(), b"png".to_vec()),
        ]);
        let v = Version {
            version: 1,
            created: String::new(),
            files: files.keys().cloned().collect(),
            bytes: 0,
            blobs: BTreeMap::from([("abc".to_string(), "assets/a.png".to_string())]),
            job: None,
            by: None,
        };
        (v, files)
    }

    #[test]
    fn a_design_serves_its_runtime_boards_logic_and_canvas() {
        let (v, files) = design();
        let r = Rules::ALL;
        let (t, p, b) = serve(Kind::Design, &v, &files, "support.js", &r).unwrap();
        assert!(t.starts_with("text/javascript") && p == Some(BOARD_POLICY));
        assert!(String::from_utf8_lossy(&b).contains("Oby's runtime of the .dc.html format"));
        let boards = String::from_utf8(
            serve(Kind::Design, &v, &files, "__dc/boards.js", &r)
                .unwrap()
                .2,
        )
        .unwrap();
        assert!(boards.contains("\"Main.dc.html\"") && boards.contains("\"logic\":true"));
        let logic = String::from_utf8(
            serve(Kind::Design, &v, &files, "__dc/logic/Main.dc.html.js", &r)
                .unwrap()
                .2,
        )
        .unwrap();
        assert!(logic.contains("window.__OBY_DC.logic[\"Main.dc.html\"]=function(DCLogic,React)"));
        let (_, hp, host) = serve(Kind::Design, &v, &files, "__canvas.html", &r).unwrap();
        assert_eq!(hp, Some(HOST_POLICY));
        let host = String::from_utf8(host).unwrap();
        assert!(
            !host.contains("</script><b>"),
            "a note's words can't end the data block"
        );
        assert_eq!(
            serve(Kind::Design, &v, &files, "_blob/abc", &r).unwrap().2,
            b"png"
        );
        assert!(serve(Kind::Design, &v, &files, "_blob/nope", &r).is_none());
        assert_eq!(
            serve(
                Kind::Design,
                &v,
                &files,
                "support.js",
                &Rules::broken("csp")
            )
            .unwrap()
            .1,
            None
        );
        // A page is no design: no runtime is served beside it.
        assert!(serve(Kind::Html, &v, &files, "support.js", &r).is_none());
    }

    #[test]
    fn a_view_loads_its_own_files_and_asks_before_going_outside() {
        let o = "http://a-v1.localhost:9";
        let r = Rules::ALL;
        assert_eq!(
            navigation(o, "http://a-v1.localhost:9/Main.dc.html", true, &r),
            Nav::Allow
        );
        assert_eq!(
            navigation(o, "http://a-v2.localhost:9/Main.dc.html", true, &r),
            Nav::Ask
        );
        assert_eq!(navigation(o, "https://example.com/", true, &r), Nav::Ask);
        assert_eq!(
            navigation(o, "https://example.com/x.js", false, &r),
            Nav::Deny
        );
        assert_eq!(
            navigation(o, "http://a-v1.localhost:90/x", false, &r),
            Nav::Deny
        );
        assert_eq!(navigation(o, "file:///etc/hosts", true, &r), Nav::Deny);
        assert_eq!(navigation(o, "data:text/html,x", true, &r), Nav::Deny);
        assert_eq!(
            navigation(o, "data:image/png;base64,x", false, &r),
            Nav::Allow
        );
        assert_eq!(
            navigation(
                o,
                "https://example.com/",
                true,
                &Rules::broken("navigation")
            ),
            Nav::Allow
        );
    }

    fn artifact(versions: u32) -> Artifact {
        Artifact {
            id: "a".into(),
            title: "T".into(),
            kind: Kind::Design,
            conversation: None,
            job: None,
            created: String::new(),
            versions: (1..=versions)
                .map(|n| Version {
                    version: n,
                    created: String::new(),
                    files: vec![],
                    bytes: 0,
                    blobs: BTreeMap::new(),
                    job: None,
                    by: None,
                })
                .collect(),
            profile: None,
            origin: Default::default(),
            shares: vec![],
            public: None,
        }
    }

    #[test]
    fn an_artifact_tab_follows_the_latest_unless_pinned_and_is_never_driven() {
        let mut tabs = ArtifactTabs::default();
        let t = tabs.open(&artifact(1));
        assert_eq!(t.kind, bc::TabKind::Artifact);
        assert_eq!(t.owner, bc::Owner::Person);
        assert_eq!(t.url, "oby-artifact://a-v1/__canvas.html");
        assert_eq!(
            tabs.open(&artifact(1)).id,
            t.id,
            "opening again brings the same tab"
        );
        tabs.refresh(|_| Some(artifact(2)));
        assert_eq!(tabs.tabs[0].artifact.as_ref().unwrap().version, 2);
        tabs.pin(&t.id, Some(1), &artifact(2)).unwrap();
        tabs.refresh(|_| Some(artifact(3)));
        assert_eq!(tabs.tabs[0].artifact.as_ref().unwrap().version, 1);
        assert_eq!(
            tabs.page_verb(&t.id, &Rules::ALL).unwrap_err().reason,
            "artifact_tab"
        );
        assert!(tabs.page_verb(&t.id, &Rules::broken("tabkind")).is_ok());
        tabs.refresh(|_| None);
        assert!(tabs.tabs.is_empty(), "a deleted artifact's tab closes");
    }
}
