//! A tab as the browser's API keeps it (BROWSER-6, 12): the wire tab, the agent's acts still
//! running on it, and the failure it last showed. A value [`super::Api`] owns; it writes nothing.

use crate::browser::contract as c;

#[derive(Clone, Debug)]
pub(super) struct TabRec {
    pub(super) tab: c::Tab,
    pub(super) inflight: u32,
    pub(super) last_error: Option<c::TabState>,
}
