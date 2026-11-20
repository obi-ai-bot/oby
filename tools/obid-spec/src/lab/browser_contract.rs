//! The browser's contract (the plan https://claude.ai/artifact/TVVtw8kq77vnm81B5UfcXq, gate 1): what
//! the browser's screens and the agents' tools talk to, shaped as obid's future API so the port
//! renames nothing, add-only like the rest of obid's API. Its TypeScript twin is
//! `tools/obid-spec/lab-ui/browser/src/contract.ts`. Owned by the lane building the browser's API;
//! a change here changes the twin in the same commit (`the_typescript_twin_matches`, below).
//!
//! Who calls what:
//!
//! - **The person's app** (lane B's screens, later the Electron app): every route but the MCP one,
//!   with obid's owner token (`Authorization: Bearer <token>`). The owner's own acts on a tab
//!   (take, give back, a secret, an answer, Save a copy, a setting, a way out of a page state) also
//!   carry the device's signed words (`X-Oby-Words`, [`WORDS_HEADER`]) over the exact body, as
//!   obid's other owner's words.
//! - **An agent**: only [`PATH_MCP`], JSON-RPC 2.0 over HTTP, with its job's own token, which dies
//!   with the job. Never the owner token, never the person's routes.
//!
//! Errors are obid's: `{"error": "One line a person can read.", "reason": "<code>"}` with a 4xx
//! ([`ApiError`]); the codes are [`REASONS`]. An agent's tool refusal is an MCP tool result with
//! `isError: true` and the same code.
//!
//! Every name here is camelCase on the wire. Enums the clients decode ([`TabState`], [`AskKind`],
//! [`EventBody`]'s `type`) only grow: a client treats a value it doesn't know as the nearest it does
//! (a state as `ready`, an event as one to ignore), never as an error.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ── Routes ──────────────────────────────────────────────────────────────────────────────────────

/// `GET`: every tab, with the settings and the last event the answer reflects ([`TabsAnswer`]).
pub const PATH_BROWSER: &str = "/v1/browser";
/// `GET`: Server-Sent Events, one [`Event`] per message (`id:` its seq, `event:` its type). A
/// reader reconnecting with `Last-Event-ID` gets what it missed while the server still holds it.
pub const PATH_EVENTS: &str = "/v1/browser/events";
/// `GET` the [`Settings`]; `PUT` a [`SettingsPatch`], signed.
pub const PATH_SETTINGS: &str = "/v1/browser/settings";
/// `GET`: every download ([`DownloadsAnswer`]).
pub const PATH_DOWNLOADS: &str = "/v1/browser/downloads";
/// `POST`: the agents' tools, MCP's JSON-RPC 2.0 over HTTP, with the job's token.
pub const PATH_MCP: &str = "/v1/browser/mcp";
/// The prefix of a tab's own routes: `/v1/browser/tabs/{id}/{verb}`.
pub const PATH_TABS: &str = "/v1/browser/tabs";
/// `GET`: Server-Sent Events of [`StreamFrame`]s (`event: frame`), newest only per reader.
pub const VERB_STREAM: &str = "stream";
/// `POST` a [`StreamInput`] while the person holds the tab ([`InputAnswer`]).
pub const VERB_INPUT: &str = "input";
/// `POST` `{}`, signed: the person takes the tab ([`HoldAnswer`]).
pub const VERB_TAKE: &str = "take";
/// `POST` `{}`, signed: the person gives it back ([`HoldAnswer`]).
pub const VERB_GIVE_BACK: &str = "giveBack";
/// `POST` a [`SecretBody`], signed, while the person holds the tab ([`SecretAnswer`]).
pub const VERB_SECRET: &str = "secret";
/// The person's ways out of a page state (board 10), each signed and only on a tab the signing
/// device holds (`not_held`, `not_holder`), each answering the [`Tab`] as it is after; never an
/// agent's tool. `POST` a [`ReloadBody`]: Try again (failed) and Reload (timed out, crashed); a page
/// that loads clears the tab's failure, one that still doesn't is refused `browser`.
pub const VERB_RELOAD: &str = "reload";
/// `POST` a [`BackBody`]: Go back, the tab's history back.
pub const VERB_BACK: &str = "back";
/// `POST` a [`DialogBody`]: the page's own dialog answered, Leave (`accept`) or Stay; with no
/// dialog on the tab, refused `no_dialog`.
pub const VERB_DIALOG: &str = "dialog";
/// `POST` an [`AnswerBody`], signed: the person's answer to an agent's ask.
pub const PATH_ASKS: &str = "/v1/browser/asks";
pub const VERB_ANSWER: &str = "answer";
/// `POST` `{}`, signed: a copy of an agent's download into the person's Downloads.
pub const VERB_SAVE_COPY: &str = "saveCopy";

/// The owner's words, signed by a registered device key over the request's exact body:
/// `<deviceId> <signedAt> <signature>`, the signature (P-256, DER, base64) over
/// `oby-words\n<METHOD>\n<path>\n<sha256 hex of the body>\n<signedAt>`.
pub const WORDS_HEADER: &str = "X-Oby-Words";
/// How old a signature may be, either way, before it is refused.
pub const WORDS_MAX_SKEW_MS: i64 = 120_000;

pub fn tab_path(tab: &str, verb: &str) -> String {
    format!("{PATH_TABS}/{tab}/{verb}")
}

pub fn ask_path(ask: &str) -> String {
    format!("{PATH_ASKS}/{ask}/{VERB_ANSWER}")
}

pub fn save_copy_path(download: &str) -> String {
    format!("{PATH_DOWNLOADS}/{download}/{VERB_SAVE_COPY}")
}

/// Every route, as `METHOD path` with `{id}` for an id: what the port adds to obid's routes.
pub const ROUTES: &[&str] = &[
    "GET /v1/browser",
    "GET /v1/browser/events",
    "GET /v1/browser/settings",
    "PUT /v1/browser/settings",
    "GET /v1/browser/downloads",
    "POST /v1/browser/downloads/{id}/saveCopy",
    "GET /v1/browser/tabs/{id}/stream",
    "POST /v1/browser/tabs/{id}/input",
    "POST /v1/browser/tabs/{id}/take",
    "POST /v1/browser/tabs/{id}/giveBack",
    "POST /v1/browser/tabs/{id}/secret",
    "POST /v1/browser/tabs/{id}/reload",
    "POST /v1/browser/tabs/{id}/back",
    "POST /v1/browser/tabs/{id}/dialog",
    "POST /v1/browser/asks/{id}/answer",
    "POST /v1/browser/mcp",
];

// ── Tabs ────────────────────────────────────────────────────────────────────────────────────────

/// Whose tab it is: an agent's (its job, which agent) or the person's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Owner {
    #[serde(rename_all = "camelCase")]
    Agent {
        job: String,
        /// `claude`, `codex`, `cursor`, … (open: a string, never a closed enum).
        agent: String,
    },
    Person,
}

/// What the tab shows, each with its own way out on the screens (board 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabState {
    Loading,
    Ready,
    Failed,
    TimedOut,
    Dialog,
    Certificate,
    Crashed,
}

pub const TAB_STATES: &[&str] = &[
    "loading",
    "ready",
    "failed",
    "timedOut",
    "dialog",
    "certificate",
    "crashed",
];

/// Which computer the tab runs on (board 14: "on devbox").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    /// `local` for this Mac, else the other computer's id.
    pub id: String,
    /// What the screens say: `This Mac`, `devbox`.
    pub name: String,
}

impl Computer {
    pub fn this_mac() -> Computer {
        Computer {
            id: "local".into(),
            name: "This Mac".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: String,
    pub owner: Owner,
    /// The page's own words: shown to the person as they are, quoted to an agent.
    pub title: String,
    pub url: String,
    pub state: TabState,
    /// The device whose person holds the tab (take control), or null.
    pub held_by: Option<String>,
    /// Changes on every take and give back. An agent's act let through under one generation is
    /// refused under the next, never half-applied.
    pub hold_generation: u64,
    pub computer: Computer,
    /// The agent's ask on this tab still waiting for the person, if any.
    pub ask: Option<Ask>,
    /// The dialog the page shows (an alert's words, the page's), if any.
    pub dialog: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabsAnswer {
    pub tabs: Vec<Tab>,
    pub settings: Settings,
    /// Each job's browser: whether it is up yet (a job has one before it has a tab).
    pub browsers: Vec<JobBrowser>,
    /// The last event this answer reflects: a reader applies only the events after it, and doesn't
    /// take an answer older than an event it already applied (add-only; 0 before any event).
    #[serde(default)]
    pub seq: u64,
}

// ── A job's browser ─────────────────────────────────────────────────────────────────────────────

/// How long a job's browser may take to start (Chrome launching for its profile), apart from
/// any command's deadline: a command's deadline counts only once the browser is up.
pub const BROWSER_START_ALLOWANCE_MS: u64 = 60_000;

/// Whether a job's browser is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserState {
    NotStarted,
    Starting,
    Ready,
    Failed,
}

pub const BROWSER_STATES: &[&str] = &["notStarted", "starting", "ready", "failed"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobBrowser {
    pub job: String,
    pub agent: String,
    pub state: BrowserState,
    /// For `failed`: why, in a sentence.
    pub reason: Option<String>,
}

// ── Holding a tab ───────────────────────────────────────────────────────────────────────────────

/// The answer to take and give back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldAnswer {
    pub tab_id: String,
    pub held_by: Option<String>,
    pub hold_generation: u64,
}

// ── Asks ────────────────────────────────────────────────────────────────────────────────────────

/// What an agent asks the person for (boards 7 to 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AskKind {
    SignIn,
    Captcha,
    Choice,
    Code,
    Passkey,
}

pub const ASK_KINDS: &[&str] = &["signIn", "captcha", "choice", "code", "passkey"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    pub id: String,
    pub tab_id: String,
    pub job: String,
    pub kind: AskKind,
    /// The agent's words, why it asks.
    pub reason: String,
    /// For `choice`: what the person picks from.
    #[serde(default)]
    pub choices: Vec<String>,
    /// For `signIn` and `code`: the page's field the secret goes into (a CSS selector).
    pub field: Option<String>,
    pub answered: bool,
    /// For `choice`, once answered: what the person picked (the agent reads it in list_tabs).
    pub choice: Option<String>,
}

/// The person's answer: `choice` for a choice, nothing for "Done" (a sign-in, a CAPTCHA, a
/// passkey they did themselves). A password or code never goes here: it goes through the secret.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerBody {
    pub choice: Option<String>,
}

// ── Secrets ─────────────────────────────────────────────────────────────────────────────────────

/// A password or code the person typed into a card, put straight into the page's field. Its value
/// is never echoed, logged, kept, written or shown to the agent.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBody {
    /// The page's field, a CSS selector (the ask's `field`).
    pub field: String,
    pub value: String,
}

impl std::fmt::Debug for SecretBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretBody")
            .field("field", &self.field)
            .field("value", &"<never shown>")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretAnswer {
    pub tab_id: String,
    pub field: String,
    pub filled: bool,
}

// ── The person's ways out of a page state ───────────────────────────────────────────────────────

/// Try again and Reload: nothing more to say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReloadBody {}

/// Go back: nothing more to say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackBody {}

/// The page's own dialog answered: `accept` is Leave (OK), else Stay (Cancel); `promptText` for a
/// prompt.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogBody {
    pub accept: bool,
    #[serde(default)]
    pub prompt_text: Option<String>,
}

// ── Downloads ───────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartedBy {
    Agent,
    Person,
}

/// Where a download is: in the agent's profile (an agent's own), or delivered to the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    Profile,
    Delivered,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: String,
    pub job: String,
    pub tab_id: Option<String>,
    /// The file's name as the page gave it (the page's words).
    pub name: String,
    pub started_by: StartedBy,
    pub place: Place,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsAnswer {
    pub downloads: Vec<Download>,
    /// The last event this answer reflects, as [`TabsAnswer::seq`].
    #[serde(default)]
    pub seq: u64,
}

/// Save a copy: the agent's file stays in its profile, a copy lands in the person's Downloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCopyAnswer {
    pub id: String,
    /// The copy's name in the person's Downloads (a number added when the name is taken).
    pub saved_as: String,
}

// ── Settings ────────────────────────────────────────────────────────────────────────────────────

/// Settings › General's two switches, both on by default (the owner's decision of 2026-11-19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// "Agents can use the browser": off, every `oby_browser_*` tool refuses with `browser_off`.
    pub agents_use_browser: bool,
    /// "Agents can use this Mac": the desktop control's switch.
    pub agents_use_mac: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            agents_use_browser: true,
            agents_use_mac: true,
        }
    }
}

/// `PUT /v1/browser/settings`: only the switches named change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub agents_use_browser: Option<bool>,
    pub agents_use_mac: Option<bool>,
}

// ── The live stream ─────────────────────────────────────────────────────────────────────────────

/// One frame of a tab, `event: frame` on its stream. A reader that falls behind gets the newest
/// frame next and the ones between are dropped, never queued (`dropped` counts them for this
/// reader); one stuck reader never holds another back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamFrame {
    pub tab_id: String,
    /// The browser's own frame number: a gap is a dropped frame.
    pub seq: u64,
    /// The image, base64.
    pub data: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    /// Frames dropped for this reader so far.
    pub dropped: u64,
}

/// The person's input on a held tab, in the page's CSS pixels (agent-browser's stream input).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamInput {
    #[serde(rename_all = "camelCase")]
    Mouse {
        /// `mousePressed`, `mouseReleased`, `mouseMoved`, `mouseWheel`.
        event_type: String,
        x: f64,
        y: f64,
        #[serde(default)]
        button: Option<String>,
        #[serde(default)]
        click_count: Option<u32>,
        #[serde(default)]
        delta_x: Option<f64>,
        #[serde(default)]
        delta_y: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Key {
        /// `keyDown`, `keyUp`, `char`.
        event_type: String,
        key: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        code: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputAnswer {
    pub tab_id: String,
    /// Downloads this input started, delivered to the person.
    pub delivered: Vec<String>,
}

// ── Events ──────────────────────────────────────────────────────────────────────────────────────

/// Data from a page, quoted (browser.rs's `outside`): where it came from, what it is, the note,
/// and the data itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outside {
    pub from: String,
    pub what: String,
    pub note: String,
    pub data: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// How an agent's action ended, as its log on the screens shows it (board 6: "refused").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Started,
    Done,
    Refused,
    Failed,
}

/// One message of `GET /v1/browser/events`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// Grows by one per event; the SSE `id`.
    pub seq: u64,
    #[serde(flatten)]
    pub body: EventBody,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventBody {
    /// A tab opened or changed (its URL, title, state, hold, ask or dialog).
    Tab {
        tab: Tab,
    },
    #[serde(rename_all = "camelCase")]
    TabClosed {
        tab_id: String,
    },
    /// The agent's action and pointer on a tab: "Clicking Sign in".
    #[serde(rename_all = "camelCase")]
    Action {
        tab_id: Option<String>,
        job: String,
        tool: String,
        /// What the screens say it does: `Clicking Sign in`, `Opening example.com`.
        text: String,
        pointer: Option<Point>,
        outcome: Outcome,
        /// For `refused` and `failed`: the reason's code.
        reason: Option<String>,
    },
    Ask {
        ask: Ask,
    },
    #[serde(rename_all = "camelCase")]
    AskAnswered {
        ask_id: String,
    },
    Download {
        download: Download,
    },
    /// An agent ran a script in a page (board 11): the script, and its result as outside data.
    #[serde(rename_all = "camelCase")]
    ScriptRun {
        tab_id: String,
        job: String,
        expression: String,
        result: Outside,
    },
    Settings {
        settings: Settings,
    },
    /// A job's browser starting, up, or not started after all.
    Browser {
        browser: JobBrowser,
    },
}

pub const EVENT_TYPES: &[&str] = &[
    "tab",
    "tabClosed",
    "action",
    "ask",
    "askAnswered",
    "download",
    "scriptRun",
    "settings",
    "browser",
];

impl EventBody {
    pub fn kind(&self) -> &'static str {
        match self {
            EventBody::Tab { .. } => "tab",
            EventBody::TabClosed { .. } => "tabClosed",
            EventBody::Action { .. } => "action",
            EventBody::Ask { .. } => "ask",
            EventBody::AskAnswered { .. } => "askAnswered",
            EventBody::Download { .. } => "download",
            EventBody::ScriptRun { .. } => "scriptRun",
            EventBody::Settings { .. } => "settings",
            EventBody::Browser { .. } => "browser",
        }
    }
}

// ── Errors ──────────────────────────────────────────────────────────────────────────────────────

/// obid's error body: one line a person can read, and a stable code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub reason: String,
}

/// Every `reason` the browser's routes and tools answer. The engine's own (browser.rs's
/// `Refusal::code`) come first, then the API's.
pub const REASONS: &[&str] = &[
    // The engine's.
    "unknown_tool",
    "bad_args",
    "no_tab",
    "no_such_tab",
    "not_your_tab",
    "human_control",
    "not_holder",
    "held_by_other",
    "control_changed",
    "unknown_ref",
    "stale_ref",
    "settling",
    "expired",
    "timed_out",
    "upload_outside_room",
    "not_granted",
    "browser",
    "certificate_page",
    // The API's.
    "browser_off",
    "no_token",
    "bad_token",
    "unsigned",
    "bad_signature",
    "unknown_device",
    "not_held",
    "no_such_ask",
    "no_such_download",
    "not_found",
    "bad_request",
    "browser_starting",
    "no_dialog",
];

// ── The agents' tools (MCP) ─────────────────────────────────────────────────────────────────────

/// The tools obid serves every agent, in the order `tools/list` gives them. Each is browser.rs's
/// tool of the same name, but `ask_person`, browser.rs's `oby_browser_request_takeover` with the
/// ask's kind.
pub const MCP_TOOLS: &[&str] = &[
    "oby_browser_open",
    "oby_browser_list_tabs",
    "oby_browser_snapshot",
    "oby_browser_screenshot",
    "oby_browser_wait_for",
    "oby_browser_navigate",
    "oby_browser_act",
    "oby_browser_evaluate",
    "oby_browser_dialog",
    "oby_browser_ask_person",
    "oby_browser_close_tab",
];

/// The MCP protocol version the server answers `initialize` with.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// One tool as `tools/list` gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub annotations: McpAnnotations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAnnotations {
    pub read_only_hint: bool,
}

/// What `ask_person` adds to browser.rs's takeover request.
pub const ASK_PERSON_DESCRIPTION: &str = "Ask the person for what only they can do on this tab: `signIn`, `captcha`, `choice` (with `choices`), `code` or `passkey`, with `reason`. A password or code goes from the person straight into the page's `field`; you never see it. Changes nothing in the page; list_tabs shows the ask and, for a choice, the answer.";

fn object(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

fn tab_prop() -> Value {
    json!({"type": "string", "description": "The tab's id; this job's current tab when left out."})
}

fn deadline_prop() -> Value {
    json!({"type": "integer", "description": "How long the command may take, in ms (default 15000, at most 120000). A command that runs out is never run again."})
}

/// Each tool's input schema.
pub fn input_schema(tool: &str) -> Value {
    match tool {
        "oby_browser_open" => object(
            json!({"url": {"type": "string"}, "newTab": {"type": "boolean"}, "deadlineMs": deadline_prop()}),
            &["url"],
        ),
        "oby_browser_list_tabs" => object(json!({}), &[]),
        "oby_browser_snapshot" | "oby_browser_screenshot" | "oby_browser_close_tab" => object(
            json!({"tab": tab_prop(), "deadlineMs": deadline_prop()}),
            &[],
        ),
        "oby_browser_wait_for" => object(
            json!({"tab": tab_prop(), "text": {"type": "string"}, "selector": {"type": "string"}, "url": {"type": "string"}, "timeoutMs": {"type": "integer"}, "deadlineMs": deadline_prop()}),
            &[],
        ),
        "oby_browser_navigate" => object(
            json!({"tab": tab_prop(), "url": {"type": "string"}, "deadlineMs": deadline_prop()}),
            &["url"],
        ),
        "oby_browser_act" => object(
            json!({
                "tab": tab_prop(),
                "action": {"type": "string", "enum": ["click", "type", "hover", "select", "drag", "upload", "press", "scroll"]},
                "ref": {"type": "string", "description": "A ref from oby_browser_snapshot (`e5`)."},
                "text": {"type": "string"}, "clear": {"type": "boolean"},
                "values": {"type": "array", "items": {"type": "string"}},
                "target": {"type": "string"}, "paths": {"type": "array", "items": {"type": "string"}},
                "key": {"type": "string"}, "modifiers": {"type": "array", "items": {"type": "string"}},
                "clickCount": {"type": "integer"}, "deltaX": {"type": "integer"}, "deltaY": {"type": "integer"},
                "deadlineMs": deadline_prop()
            }),
            &["action"],
        ),
        "oby_browser_evaluate" => object(
            json!({"tab": tab_prop(), "expression": {"type": "string"}, "deadlineMs": deadline_prop()}),
            &["expression"],
        ),
        "oby_browser_dialog" => object(
            json!({"tab": tab_prop(), "accept": {"type": "boolean"}, "promptText": {"type": "string"}, "deadlineMs": deadline_prop()}),
            &[],
        ),
        "oby_browser_ask_person" => object(
            json!({
                "tab": tab_prop(),
                "kind": {"type": "string", "enum": ASK_KINDS},
                "reason": {"type": "string"},
                "choices": {"type": "array", "items": {"type": "string"}},
                "field": {"type": "string", "description": "For signIn and code: the page's field the secret goes into, a CSS selector."}
            }),
            &["kind", "reason"],
        ),
        _ => object(json!({}), &[]),
    }
}

/// `tools/list`: each tool with browser.rs's description, its schema, and whether it only reads.
pub fn mcp_tools() -> Vec<McpTool> {
    MCP_TOOLS
        .iter()
        .map(|&name| {
            let engine = super::browser::tool(name);
            McpTool {
                name: name.into(),
                description: match engine {
                    Some(t) => t.description.into(),
                    None => ASK_PERSON_DESCRIPTION.into(),
                },
                input_schema: input_schema(name),
                annotations: McpAnnotations {
                    read_only_hint: engine.map(|t| t.kind.read_only()).unwrap_or(true),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWIN: &str = include_str!("../../lab-ui/browser/src/contract.ts");

    /// The strings of `export const <name> = [ … ] as const;` in the twin, in order.
    fn twin_list(name: &str) -> Vec<String> {
        let start = TWIN
            .find(&format!("export const {name} = ["))
            .unwrap_or_else(|| panic!("contract.ts has no {name}"));
        let rest = &TWIN[start..];
        let end = rest.find("] as const").expect("the list's end");
        rest[..end]
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    }

    fn twin_const(name: &str) -> String {
        let start = TWIN
            .find(&format!("export const {name} = \""))
            .unwrap_or_else(|| panic!("contract.ts has no {name}"));
        TWIN[start..].split('"').nth(1).unwrap().to_string()
    }

    #[test]
    fn the_typescript_twin_matches() {
        assert_eq!(twin_list("ROUTES"), ROUTES);
        assert_eq!(twin_list("TAB_STATES"), TAB_STATES);
        assert_eq!(twin_list("ASK_KINDS"), ASK_KINDS);
        assert_eq!(twin_list("EVENT_TYPES"), EVENT_TYPES);
        assert_eq!(twin_list("REASONS"), REASONS);
        assert_eq!(twin_list("MCP_TOOLS"), MCP_TOOLS);
        assert_eq!(twin_list("BROWSER_STATES"), BROWSER_STATES);
        assert!(TWIN.contains("export const BROWSER_START_ALLOWANCE_MS = 60_000;"));
        assert_eq!(BROWSER_START_ALLOWANCE_MS, 60_000);
        assert_eq!(twin_const("WORDS_HEADER"), WORDS_HEADER);
        assert_eq!(twin_const("MCP_PROTOCOL_VERSION"), MCP_PROTOCOL_VERSION);
        for path in [
            PATH_BROWSER,
            PATH_EVENTS,
            PATH_SETTINGS,
            PATH_DOWNLOADS,
            PATH_MCP,
            PATH_TABS,
            PATH_ASKS,
        ] {
            assert!(
                TWIN.contains(&format!("\"{path}\"")),
                "contract.ts lacks {path}"
            );
        }
        // Every field the wire carries is named in the twin.
        let wire = [
            serde_json::to_value(sample_tab()).unwrap(),
            serde_json::to_value(Event {
                seq: 1,
                body: EventBody::ScriptRun {
                    tab_id: "t".into(),
                    job: "j".into(),
                    expression: "1".into(),
                    result: Outside {
                        from: "".into(),
                        what: "".into(),
                        note: "".into(),
                        data: json!(1),
                    },
                },
            })
            .unwrap(),
            serde_json::to_value(StreamFrame {
                tab_id: "t".into(),
                seq: 1,
                data: "".into(),
                mime: "image/jpeg".into(),
                width: 1,
                height: 1,
                dropped: 0,
            })
            .unwrap(),
            serde_json::to_value(TabsAnswer {
                tabs: vec![],
                settings: Settings::default(),
                browsers: vec![JobBrowser {
                    job: "j".into(),
                    agent: "claude".into(),
                    state: BrowserState::Starting,
                    reason: None,
                }],
                seq: 7,
            })
            .unwrap(),
            serde_json::to_value(DialogBody {
                accept: true,
                prompt_text: Some("p".into()),
            })
            .unwrap(),
            serde_json::to_value(Download {
                id: "d".into(),
                job: "j".into(),
                tab_id: None,
                name: "n".into(),
                started_by: StartedBy::Agent,
                place: Place::Profile,
                bytes: 0,
            })
            .unwrap(),
        ];
        fn keys(v: &Value, out: &mut Vec<String>) {
            if let Value::Object(m) = v {
                for (k, v) in m {
                    out.push(k.clone());
                    keys(v, out);
                }
            }
        }
        let mut all = vec![];
        for v in &wire {
            keys(v, &mut all);
        }
        for k in all {
            assert!(
                TWIN.contains(&format!("{k}:")) || TWIN.contains(&format!("{k}?:")),
                "contract.ts lacks the field {k}"
            );
        }
    }

    fn sample_tab() -> Tab {
        Tab {
            id: "tab-1".into(),
            owner: Owner::Agent {
                job: "j".into(),
                agent: "claude".into(),
            },
            title: "t".into(),
            url: "u".into(),
            state: TabState::TimedOut,
            held_by: Some("d".into()),
            hold_generation: 2,
            computer: Computer::this_mac(),
            ask: Some(Ask {
                id: "a".into(),
                tab_id: "tab-1".into(),
                job: "j".into(),
                kind: AskKind::SignIn,
                reason: "r".into(),
                choices: vec![],
                field: Some("#pw".into()),
                answered: false,
                choice: None,
            }),
            dialog: None,
        }
    }

    #[test]
    fn names_are_camel_case_on_the_wire() {
        let v = serde_json::to_value(sample_tab()).unwrap();
        assert_eq!(v["state"], "timedOut");
        assert_eq!(v["heldBy"], "d");
        assert_eq!(v["holdGeneration"], 2);
        assert_eq!(
            v["owner"],
            json!({"kind": "agent", "job": "j", "agent": "claude"})
        );
        assert_eq!(v["ask"]["kind"], "signIn");
        let e = serde_json::to_value(Event {
            seq: 3,
            body: EventBody::TabClosed {
                tab_id: "tab-1".into(),
            },
        })
        .unwrap();
        assert_eq!(e, json!({"seq": 3, "type": "tabClosed", "tabId": "tab-1"}));
        let s: Settings =
            serde_json::from_value(json!({"agentsUseBrowser": false, "agentsUseMac": true}))
                .unwrap();
        assert!(!s.agents_use_browser);
        let i: StreamInput = serde_json::from_value(
            json!({"type": "mouse", "eventType": "mousePressed", "x": 1, "y": 2, "button": "left", "clickCount": 1}),
        )
        .unwrap();
        assert!(matches!(i, StreamInput::Mouse { .. }));
    }

    #[test]
    fn both_switches_are_on_by_default() {
        assert_eq!(
            serde_json::to_value(Settings::default()).unwrap(),
            json!({"agentsUseBrowser": true, "agentsUseMac": true})
        );
    }

    #[test]
    fn a_secret_never_prints_its_value() {
        let s: SecretBody =
            serde_json::from_value(json!({"field": "#pw", "value": "hunter2"})).unwrap();
        assert!(!format!("{s:?}").contains("hunter2"));
    }

    #[test]
    fn every_tool_is_the_engines_or_ask_person_and_every_reason_is_known() {
        for t in mcp_tools() {
            assert!(
                super::super::browser::tool(&t.name).is_some()
                    || t.name == "oby_browser_ask_person",
                "{} isn't browser.rs's",
                t.name
            );
            assert_eq!(t.input_schema["type"], "object");
        }
        let ro: Vec<_> = mcp_tools()
            .into_iter()
            .filter(|t| t.annotations.read_only_hint)
            .map(|t| t.name)
            .collect();
        assert_eq!(
            ro,
            [
                "oby_browser_list_tabs",
                "oby_browser_snapshot",
                "oby_browser_screenshot",
                "oby_browser_wait_for",
                "oby_browser_ask_person"
            ]
        );
        use super::super::browser::Refusal;
        for r in [
            Refusal::UnknownTool("x".into()),
            Refusal::HumanControl,
            Refusal::ControlChanged,
            Refusal::Settling { tool: "x".into() },
            Refusal::Browser("x".into()),
            Refusal::CertificatePage,
        ] {
            assert!(REASONS.contains(&r.code()), "{}", r.code());
        }
        assert_eq!(EVENT_TYPES.len(), 9);
    }
}
