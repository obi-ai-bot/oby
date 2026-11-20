//! Who calls the engine (BROWSER-5, 13): an agent by its conversation, or the person by the device
//! they act from. It owns no state and writes nothing.

/// Who calls: an agent (by its conversation) or the person (by the device they act from).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Actor {
    Agent(String),
    Person(String),
}
