//! A password or code the person typed into a card (BROWSER-8), put straight into the page's field.
//! A wire type that only deserializes: it owns no state, writes nothing, and its `Debug` never
//! shows the value.

use serde::Deserialize;

/// A password or code the person typed into a card, put straight into the page's field. Its value
/// is never echoed, logged, kept, written or shown to the agent.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBody {
    /// The page's field, a CSS selector (the ask's `field`).
    pub field: String,
    pub value: String,
}

impl std::fmt::Debug for SecretBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretBody")
            .field("field", &self.field)
            .field("value", &"<never shown>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn browser_8_a_secret_never_prints_its_value() {
        let s: SecretBody =
            serde_json::from_value(json!({"field": "#pw", "value": "hunter2"})).unwrap();
        assert!(!format!("{s:?}").contains("hunter2"));
    }
}
