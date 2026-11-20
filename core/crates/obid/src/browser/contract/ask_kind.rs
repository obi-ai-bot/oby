//! What an agent asks the person for (BROWSER-19; boards 7 to 9). Decoded by the clients as open.
//! A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

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
