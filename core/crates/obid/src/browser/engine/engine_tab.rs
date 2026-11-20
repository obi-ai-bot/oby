//! A tab as the engine knows it (BROWSER-5, 10, 11): its conversation, the browser's target, who
//! holds it, its generations, refs, settling and dialog. A value [`super::Browsers`] owns; it
//! writes nothing. The lab calls it `Tab`; here it is `EngineTab`, apart from the wire's
//! [`crate::browser::contract::Tab`].

use serde_json::Value;

use super::Refs;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineTab {
    /// Oby's id for the tab (`tab-3`), unique across conversations.
    pub id: String,
    pub conversation: String,
    /// The browser's own id for it (CDP's target id).
    pub target: String,
    pub url: String,
    pub title: String,
    /// The person holding it, if any.
    pub person: Option<String>,
    /// Changes on every take and release (T3's `epoch`).
    pub control_generation: u64,
    /// Changes on every navigation Oby sees.
    pub page_generation: u64,
    pub refs: Option<Refs>,
    /// The tool whose command timed out, while the browser is still finishing it.
    pub settling: Option<String>,
    pub takeover_asked: Option<String>,
    pub dialog: Option<Value>,
}
