//! `POST /v1/browser/tabs/{id}/proceed`'s body (BROWSER-28): the person's Go on anyway, naming the
//! certificate they were shown. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Go on anyway: the SHA-256 fingerprint of the certificate the person was shown
/// ([`super::Certificate`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProceedBody {
    pub fingerprint: String,
}
