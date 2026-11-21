//! Where an own conversation's agent may write besides its room (docs/contracts/folders.md,
//! WORKSPACE-1; docs/contracts/lanes.md, LANES-12, LANES-13): the folder it works in, or the
//! worktree obid made for it (`crate::workspace::places`), as `crate::workspace::writes::writes`
//! allows it. Only an own session of a conversation no outside word marked (no `foreign` reason:
//! its words were signed by the owner's own app, and nothing it heard or read was someone else's);
//! every other gets its room only. Fails closed: a folder whose place has no path, or that
//! `crate::folders::check` refuses now (it may have become a link or moved since it was picked),
//! adds nothing. Inside, the repository's `.git/hooks` and `.git/config` and a worktree's `.git`
//! file and pointers stay read-only: a hook or a setting written there would run in the owner's
//! own git. Where a refusal comes from: Claude Code's file tools, obid's rule ([`allows`]); Codex
//! outside Bypass, its sandbox ([`codex_entries`], profile `oby`, LANES-27); an own Claude Code
//! conversation's Bash, the owner's own sandbox or none (LANES-23); Bypass, no sandbox. It owns no
//! state and writes nothing.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::api::Shared;
use crate::workspace::Workspace;
use crate::workspace::writes::{Guards, writes};

/// One folder an own conversation may write: where it works there, what a commit there needs of
/// the repository's git, and what stays read-only inside. Every path canonical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Writable {
    /// The folder itself ("This folder"), or its worktree ("A new worktree").
    pub path: PathBuf,
    /// What a commit there writes outside `path`: the repository's git folder for a folder inside
    /// it, or a worktree's own git folder and the repository's objects, refs and logs.
    pub git: Vec<PathBuf>,
    /// Never written: the repository's hooks and config, a worktree's `.git` file and pointers.
    pub kept: Vec<PathBuf>,
}

/// The folders conversation `id`'s `own` session may write this turn, each checked again now.
pub(crate) async fn of(shared: &Shared, id: &str, own: bool) -> Vec<Writable> {
    if !own || shared.conversations.foreign(id).is_some() {
        return Vec::new();
    }
    let home = &shared.config.home;
    let Ok(workspace) = crate::workspace::chosen(home, id) else {
        return Vec::new();
    };
    let checks = crate::folders::Places::of(shared);
    let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let obid = real(home);
    let guards = Guards {
        worktrees: real(&crate::workspace::worktrees(home)),
        homes: checks.homes.iter().map(|h| real(h)).collect(),
        accounts: (checks.accounts.iter())
            .chain(&shared.jobs.claude_folders())
            .map(|a| real(a))
            .collect(),
        obid,
    };
    let mut found = Vec::new();
    for place in crate::workspace::places(shared, id).await {
        let Some(path) = &place.path else { continue };
        let folder = place.folder.to_string_lossy();
        if let Err(why) = crate::folders::check(&checks, &folder) {
            tracing::warn!("conversation {id}: {folder} is no longer one it may write: {why}");
            continue;
        }
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        let kind = match crate::folders::same(&path.to_string_lossy(), &folder) {
            true => Workspace::Folder,
            false => workspace,
        };
        if writes(kind, &path, &guards).is_some_and(|list| list == [path.clone()]) {
            found.push(layout(&path));
        }
    }
    found
}

/// `path` with its repository's git as a commit there uses it: the nearest `.git` at or above it,
/// a folder (a repository's own) or a file (a worktree's, naming its git folder).
fn layout(path: &Path) -> Writable {
    let mut writable = Writable {
        path: path.to_path_buf(),
        git: Vec::new(),
        kept: Vec::new(),
    };
    let Some(dot) = path
        .ancestors()
        .map(|a| a.join(".git"))
        .find(|g| g.exists())
    else {
        return writable;
    };
    let real = |p: PathBuf| p.canonicalize().unwrap_or(p);
    if dot.is_dir() {
        if !dot.starts_with(path) {
            writable.git.push(real(dot.clone()));
        }
        writable.kept = ["hooks", "config"].map(|n| real(dot.join(n))).into();
        return writable;
    }
    // A worktree: `.git` is a file naming its own git folder, whose `commondir` names the
    // repository's.
    let named = std::fs::read_to_string(&dot).unwrap_or_default();
    let Some(admin) = named.trim().strip_prefix("gitdir: ").map(PathBuf::from) else {
        writable.kept.push(dot);
        return writable;
    };
    let admin = real(dot.parent().unwrap_or(path).join(admin));
    let common = std::fs::read_to_string(admin.join("commondir")).unwrap_or_default();
    let common = real(admin.join(common.trim()));
    writable.git.push(admin.clone());
    writable
        .git
        .extend(["objects", "refs", "logs", "packed-refs"].map(|n| real(common.join(n))));
    writable.kept = vec![dot, admin.join("commondir"), admin.join("gitdir")];
    writable
        .kept
        .extend(["hooks", "config"].map(|n| real(common.join(n))));
    writable
}

/// Whether Claude Code's file tool `tool` may run with `input` because every path it names is in
/// one of `writable`'s folders (never in a `.git`, never one of `kept`): obid's rule is the limit
/// there, as for the room. Relative paths are the room's, `cwd`.
pub(crate) fn allows(writable: &[Writable], tool: &str, input: &Value, cwd: &Path) -> bool {
    const FILE_TOOLS: [&str; 9] = [
        "Read",
        "Edit",
        "MultiEdit",
        "Write",
        "NotebookEdit",
        "NotebookRead",
        "Grep",
        "Glob",
        "LS",
    ];
    if writable.is_empty() || !FILE_TOOLS.contains(&tool) {
        return false;
    }
    let call = crate::permissions::Call::new(tool, input);
    !call.paths.is_empty()
        && call.paths.iter().all(|named| {
            let full = cwd.join(named);
            let Some(real) = crate::policy::real_path(&full).or_else(|| real_parent(&full)) else {
                return false;
            };
            let git = real
                .components()
                .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"));
            !git && writable
                .iter()
                .any(|w| real.starts_with(&w.path) && !w.kept.iter().any(|k| real.starts_with(k)))
        })
}

/// A file that isn't there yet, by its folder's real path.
fn real_parent(path: &Path) -> Option<PathBuf> {
    let (parent, name) = (path.parent()?, path.file_name()?);
    crate::policy::real_path(parent).map(|p| p.join(name))
}

/// What profile `oby` of an own Codex conversation says of `writable` (LANES-27): each folder and
/// what a commit there needs, `write`; what stays read-only inside, `read`. Applied only where
/// Codex's sandbox writes without asking (Accept edits, Sandbox: `codex::own::Policy::writing`).
pub(crate) fn codex_entries(writable: &[Writable]) -> (Vec<String>, Vec<String>) {
    let text = |p: &PathBuf| p.to_string_lossy().into_owned();
    let write = (writable.iter())
        .flat_map(|w| std::iter::once(&w.path).chain(&w.git))
        .map(text)
        .collect();
    let read = writable.iter().flat_map(|w| &w.kept).map(text).collect();
    (write, read)
}

/// What the turn's prompt says of `writable`, when there are any.
pub(crate) fn note(writable: &[Writable]) -> Option<String> {
    (!writable.is_empty()).then(|| {
        let paths: Vec<String> = (writable.iter())
            .map(|w| w.path.to_string_lossy().into_owned())
            .collect();
        format!(
            "This conversation works in these folders, and you may edit files in them besides your room: {}. Their .git folders' hooks and config are never yours to write.",
            paths.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn git(dir: &Path, args: &[&str]) {
        let ran = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            ran.status.success(),
            "{}",
            String::from_utf8_lossy(&ran.stderr)
        );
    }

    /// WORKSPACE-1: in a folder that is its repository, the folder is written, its `.git/hooks`
    /// and `.git/config` kept; in a worktree, its own git folder and the repository's objects,
    /// refs and logs are what a commit writes, and its `.git` file, pointers, the repository's
    /// hooks and config stay read-only.
    #[test]
    fn workspace_1_a_folder_and_a_worktree_keep_their_git_read_only() {
        let root = std::env::temp_dir().join(format!("obid-ws1-{}", uuid::Uuid::new_v4()));
        let repo = root.join("api");
        std::fs::create_dir_all(&repo).unwrap();
        let repo = repo.canonicalize().unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first",
            ],
        );
        let folder = layout(&repo);
        assert!(folder.git.is_empty(), "{folder:?}");
        assert_eq!(
            folder.kept,
            [repo.join(".git/hooks"), repo.join(".git/config")]
        );
        let tree = root.join("tree");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "oby/t",
                tree.to_str().unwrap(),
            ],
        );
        let tree = tree.canonicalize().unwrap();
        let worktree = layout(&tree);
        let admin = repo.join(".git/worktrees/tree");
        assert!(worktree.git.contains(&admin), "{worktree:?}");
        assert!(
            worktree.git.contains(&repo.join(".git/objects")),
            "{worktree:?}"
        );
        for kept in [
            tree.join(".git"),
            admin.join("commondir"),
            repo.join(".git/hooks"),
            repo.join(".git/config"),
        ] {
            assert!(worktree.kept.contains(&kept), "{kept:?}: {worktree:?}");
        }
        let (write, read) = codex_entries(&[worktree]);
        assert!(write.contains(&tree.to_string_lossy().into_owned()));
        assert!(read.contains(&repo.join(".git/hooks").to_string_lossy().into_owned()));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// WORKSPACE-1: Claude Code's file tools write the folder, by its path or the room's relative
    /// one through it, never outside it, never a `.git` in it, and nothing at all with no folder.
    #[test]
    fn workspace_1_claude_file_tools_write_only_the_folder() {
        let root = std::env::temp_dir().join(format!("obid-ws1c-{}", uuid::Uuid::new_v4()));
        let (folder, room, outside) = (root.join("api"), root.join("room"), root.join("other"));
        for dir in [folder.join(".git/hooks"), room.clone(), outside.clone()] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let folder = folder.canonicalize().unwrap();
        let writable = [layout(&folder)];
        let edit = |path: &Path| json!({"file_path": path, "old_string": "a", "new_string": "b"});
        assert!(allows(
            &writable,
            "Write",
            &edit(&folder.join("new.rs")),
            &room
        ));
        assert!(!allows(
            &writable,
            "Write",
            &edit(&outside.join("x.rs")),
            &room
        ));
        assert!(!allows(
            &writable,
            "Write",
            &edit(&folder.join(".git/hooks/pre-commit")),
            &room
        ));
        assert!(!allows(
            &writable,
            "Write",
            &edit(&folder.join(".git/config")),
            &room
        ));
        assert!(!allows(
            &writable,
            "Bash",
            &json!({"command": "true"}),
            &room
        ));
        assert!(!allows(&[], "Write", &edit(&folder.join("new.rs")), &room));
        let _ = std::fs::remove_dir_all(&root);
    }
}
