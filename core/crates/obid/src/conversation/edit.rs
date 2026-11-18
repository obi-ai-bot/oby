//! What one change of a conversation asks (`PATCH /v1/conversations/{id}`, [`super::Conversations::change`]):
//! its title, pin, folders, account, project and marks, each `None` to leave as it is. It owns no
//! state: it carries the request to the conversations' lock, where it's merged and written.

use super::{FolderChange, Marks};

#[derive(Default)]
pub struct Edit {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub folders: Option<FolderChange>,
    /// An account's id, `auto`, or `Some(None)` to follow the setting again.
    pub talk_account: Option<Option<String>>,
    /// The project it joins and that project's folders, or `Some(None)` to take it out.
    pub project: Option<Option<(String, Vec<String>)>>,
    pub marks: Marks,
}
