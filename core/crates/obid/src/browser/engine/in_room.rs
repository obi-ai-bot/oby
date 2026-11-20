//! Whether a path is inside an agent's room (the `upload` action of BROWSER-1's `oby_browser_act`):
//! uploads come only from it. Compared as written, never through the file system; it owns no
//! state and writes nothing.

use std::path::{Component, Path};

/// `path` is inside `room` (both absolute, compared as written after `..` is refused).
pub fn in_room(room: &Path, path: &Path) -> bool {
    path.is_absolute()
        && !path.components().any(|c| matches!(c, Component::ParentDir))
        && path.starts_with(room)
}
