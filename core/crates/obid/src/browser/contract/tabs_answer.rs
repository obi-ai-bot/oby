//! `GET /v1/browser`'s answer (BROWSER-2 to 4): every tab, the settings, each job's browser and
//! the last event the answer reflects. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

use super::{JobBrowser, Settings, Tab};

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
