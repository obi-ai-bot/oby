//! obid's error body (every reason of `REASONS`): one line a person can read, and a stable code. A
//! wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// obid's error body: one line a person can read, and a stable code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub reason: String,
}
