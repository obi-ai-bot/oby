//! Why the engine refused or failed a call (every engine reason of the contract's `REASONS`): a
//! code the agent can branch on and a sentence it can read. It owns no state and writes nothing.

use serde_json::{Value, json};

/// Why a call was refused or failed: a code the agent can branch on and a sentence it can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    UnknownTool(String),
    BadArgs(String),
    NoTab,
    NoSuchTab(String),
    NotYourTab,
    HumanControl,
    NotHolder,
    HeldByOther,
    ControlChanged,
    UnknownRef(String),
    StaleRef { reference: String, why: String },
    Settling { tool: String },
    Expired,
    TimedOut { ms: u64 },
    UploadOutsideRoom(String),
    NotGranted,
    Browser(String),
    CertificatePage,
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::UnknownTool(_) => "unknown_tool",
            Refusal::BadArgs(_) => "bad_args",
            Refusal::NoTab => "no_tab",
            Refusal::NoSuchTab(_) => "no_such_tab",
            Refusal::NotYourTab => "not_your_tab",
            Refusal::HumanControl => "human_control",
            Refusal::NotHolder => "not_holder",
            Refusal::HeldByOther => "held_by_other",
            Refusal::ControlChanged => "control_changed",
            Refusal::UnknownRef(_) => "unknown_ref",
            Refusal::StaleRef { .. } => "stale_ref",
            Refusal::Settling { .. } => "settling",
            Refusal::Expired => "expired",
            Refusal::TimedOut { .. } => "timed_out",
            Refusal::UploadOutsideRoom(_) => "upload_outside_room",
            Refusal::NotGranted => "not_granted",
            Refusal::Browser(_) => "browser",
            Refusal::CertificatePage => "certificate_page",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Refusal::UnknownTool(t) => format!("There is no browser tool named {t}."),
            Refusal::BadArgs(why) => why.clone(),
            Refusal::NoTab => {
                "This conversation has no tab yet: open one with oby_browser_open.".into()
            }
            Refusal::NoSuchTab(t) => format!("There is no tab {t}."),
            Refusal::NotYourTab => "This tab belongs to another conversation.".into(),
            Refusal::HumanControl => {
                "The person controls this tab. Reads still work; act again once they give it back."
                    .into()
            }
            Refusal::NotHolder => {
                "Only the person who took this tab can act on it or give it back.".into()
            }
            Refusal::HeldByOther => "Someone else controls this tab.".into(),
            Refusal::ControlChanged => {
                "Control of this tab changed. Take a new snapshot before trying again.".into()
            }
            Refusal::UnknownRef(r) => {
                format!("No ref {r} in this tab's last snapshot. Take a new snapshot.")
            }
            Refusal::StaleRef { reference, why } => {
                format!("Ref {reference} is stale: {why}. Take a new snapshot.")
            }
            Refusal::Settling { tool } => format!(
                "The tab's last command ({tool}) timed out and is still settling; it won't be run again. Check oby_browser_status, then take a new snapshot."
            ),
            Refusal::Expired => {
                "This request's deadline passed before it could start; it didn't run.".into()
            }
            Refusal::TimedOut { ms } => format!(
                "The command didn't finish within {ms} ms. It may have partly happened; it won't be run again. The tab settles on its own: check oby_browser_status, then take a new snapshot."
            ),
            Refusal::UploadOutsideRoom(p) => {
                format!("{p} is outside your room; only files in your room can be uploaded.")
            }
            Refusal::NotGranted => {
                "This app isn't one the person let this conversation drive.".into()
            }
            Refusal::Browser(why) => why.clone(),
            Refusal::CertificatePage => "This page stopped at a certificate problem (or another of the browser's own error pages); only the person can decide to go on. Reads still work, and so does navigating elsewhere.".into(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({"ok": false, "error": {"code": self.code(), "message": self.message()}})
    }
}
