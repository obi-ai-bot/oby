//! The person's answer to an ask (BROWSER-19): a choice, or nothing for "Done". A password or code
//! never goes here. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// The person's answer: `choice` for a choice, nothing for "Done" (a sign-in, a CAPTCHA, a
/// passkey they did themselves). A password or code never goes here: it goes through the secret.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerBody {
    pub choice: Option<String>,
}
