//! What a tool does to a tab (BROWSER-5, 9): the kind that decides who may call it when. It owns
//! no state and writes nothing.

/// What a tool does to a tab, which decides who may call it when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Reads the tab; allowed while the person holds it (T3's `observe`).
    Read,
    /// Changes the tab or runs code in it; needs the agent's own tab with no person holding it.
    Act,
    /// Opens a tab of the conversation's own.
    Open,
    /// Changes nothing in a page: answered from Oby's own state.
    Oby,
}

impl Kind {
    pub fn read_only(self) -> bool {
        matches!(self, Kind::Read | Kind::Oby)
    }
}
