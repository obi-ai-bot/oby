//! The browser's contract (docs/contracts/browser.md, BROWSER-1 to 25): what the browser's screens
//! and the agents' tools talk to, shaped as obid's API, add-only like the rest of it. The wire
//! types are one per file below; this file holds the routes, the lists the clients decode, the
//! reasons, and the agents' tool list with its schemas. It owns no state and writes nothing. Its
//! TypeScript twin, and the test that keeps the two alike, stay in the lab
//! (`tools/obid-spec/lab-ui/browser/src/contract.ts`).
//!
//! Who calls what:
//!
//! - **The person's app** (the Electron app): every route but the MCP one, with obid's owner token
//!   (`Authorization: Bearer <token>`). The owner's own acts on a tab (take, give back, a secret, an
//!   answer, Save a copy, a setting) also carry the device's signed words (`X-Oby-Words`,
//!   [`WORDS_HEADER`]) over the exact body, as obid's other owner's words.
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

use serde_json::{Value, json};

use crate::browser::engine;

mod answer_body;
mod api_error;
mod ask;
mod ask_kind;
mod browser_state;
mod computer;
#[cfg(test)]
mod doc_guard;
mod download;
mod downloads_answer;
mod event;
mod event_body;
mod hold_answer;
mod input_answer;
mod job_browser;
mod mcp_annotations;
mod mcp_tool;
mod outcome;
mod outside;
mod owner;
mod place;
mod point;
mod save_copy_answer;
mod secret_answer;
mod secret_body;
mod settings;
mod settings_patch;
mod started_by;
mod stream_frame;
mod stream_input;
mod tab;
mod tab_state;
mod tabs_answer;

pub use answer_body::AnswerBody;
pub use api_error::ApiError;
pub use ask::Ask;
pub use ask_kind::AskKind;
pub use browser_state::BrowserState;
pub use computer::Computer;
pub use download::Download;
pub use downloads_answer::DownloadsAnswer;
pub use event::Event;
pub use event_body::EventBody;
pub use hold_answer::HoldAnswer;
pub use input_answer::InputAnswer;
pub use job_browser::JobBrowser;
pub use mcp_annotations::McpAnnotations;
pub use mcp_tool::McpTool;
pub use outcome::Outcome;
pub use outside::Outside;
pub use owner::Owner;
pub use place::Place;
pub use point::Point;
pub use save_copy_answer::SaveCopyAnswer;
pub use secret_answer::SecretAnswer;
pub use secret_body::SecretBody;
pub use settings::Settings;
pub use settings_patch::SettingsPatch;
pub use started_by::StartedBy;
pub use stream_frame::StreamFrame;
pub use stream_input::StreamInput;
pub use tab::Tab;
pub use tab_state::TabState;
pub use tabs_answer::TabsAnswer;

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
    "POST /v1/browser/asks/{id}/answer",
    "POST /v1/browser/mcp",
];

// ── Lists the clients decode ────────────────────────────────────────────────────────────────────

pub const TAB_STATES: &[&str] = &[
    "loading",
    "ready",
    "failed",
    "timedOut",
    "dialog",
    "certificate",
    "crashed",
];

/// How long a job's browser may take to start (Chrome launching for its profile), apart from
/// any command's deadline: a command's deadline counts only once the browser is up.
pub const BROWSER_START_ALLOWANCE_MS: u64 = 60_000;

pub const BROWSER_STATES: &[&str] = &["notStarted", "starting", "ready", "failed"];

pub const ASK_KINDS: &[&str] = &["signIn", "captcha", "choice", "code", "passkey"];

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

// ── Errors ──────────────────────────────────────────────────────────────────────────────────────

/// Every `reason` the browser's routes and tools answer. The engine's own
/// ([`engine::Refusal::code`]) come first, then the API's.
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
];

// ── The agents' tools (MCP) ─────────────────────────────────────────────────────────────────────

/// The tools obid serves every agent, in the order `tools/list` gives them. Each is the engine's
/// tool of the same name ([`engine::TOOLS`]), but `ask_person`, the engine's
/// `oby_browser_request_takeover` with the ask's kind.
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

/// What `ask_person` adds to the engine's takeover request.
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

/// `tools/list`: each tool with the engine's description, its schema, and whether it only reads.
pub fn mcp_tools() -> Vec<McpTool> {
    MCP_TOOLS
        .iter()
        .map(|&name| {
            let engine = engine::tool(name);
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
    fn browser_1_every_tool_is_the_engines_or_ask_person_and_every_reason_is_known() {
        for t in mcp_tools() {
            assert!(
                engine::tool(&t.name).is_some() || t.name == "oby_browser_ask_person",
                "{} isn't the engine's",
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
        use engine::Refusal;
        for r in [
            Refusal::UnknownTool("x".into()),
            Refusal::HumanControl,
            Refusal::ControlChanged,
            Refusal::Settling { tool: "x".into() },
            Refusal::Browser("x".into()),
        ] {
            assert!(REASONS.contains(&r.code()), "{}", r.code());
        }
        assert_eq!(EVENT_TYPES.len(), 9);
    }
}
