//! A refusal of the browser's API (every reason of the contract's `REASONS`): obid's status, the
//! contract's reason and a sentence a person or agent can read. It owns no state and writes
//! nothing.

use serde_json::Value;

use crate::browser::contract as c;

/// A refusal: obid's status, the contract's reason and a sentence a person (or agent) can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub status: u16,
    pub reason: &'static str,
    pub message: String,
}

impl Refused {
    pub fn new(status: u16, reason: &'static str, message: impl Into<String>) -> Refused {
        debug_assert!(
            c::REASONS.contains(&reason),
            "{reason} isn't in the contract"
        );
        Refused {
            status,
            reason,
            message: message.into(),
        }
    }

    /// obid's error body.
    pub fn body(&self) -> Value {
        serde_json::to_value(c::ApiError {
            error: self.message.clone(),
            reason: self.reason.into(),
        })
        .expect("an error serializes")
    }

    /// An engine refusal, as the person's routes answer it.
    pub fn engine(code: &str, message: &str) -> Refused {
        let reason = c::REASONS
            .iter()
            .copied()
            .find(|r| *r == code)
            .unwrap_or("browser");
        let status = match reason {
            "no_such_tab" => 404,
            "not_holder" => 403,
            "bad_args" => 400,
            _ => 409,
        };
        Refused::new(status, reason, message)
    }
}

/// "Agents can use the browser" is off, for another Oby's task (BROWSER-15, 18): it finishes without
/// a browser, since its own owner can't turn the switch on.
pub fn browser_off_for_another_oby() -> Refused {
    Refused::new(
        403,
        "browser_off",
        "The owner of the Oby running this task turned off \"Agents can use the browser\" in Settings › General, so no browser tool runs for it. Finish without a browser, and say so in your summary.",
    )
}

/// "Agents can use the browser" is off (BROWSER-15): every tool refuses with this.
pub fn browser_off() -> Refused {
    Refused::new(
        403,
        "browser_off",
        "The person turned off \"Agents can use the browser\" in Settings › General, so no browser tool runs. Ask them if you need it.",
    )
}
