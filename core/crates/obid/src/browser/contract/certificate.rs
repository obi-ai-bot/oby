//! A server's certificate on the wire (BROWSER-28): on a tab stopped at a certificate problem, the
//! one obid read there itself, what Go on anyway would trust. A wire type: it owns no state and
//! writes nothing.

use serde::{Deserialize, Serialize};

/// The host it was seen on and its SHA-256 fingerprint (lowercase hex of the certificate's DER),
/// which the person's Go on anyway names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Certificate {
    pub host: String,
    pub fingerprint: String,
}
