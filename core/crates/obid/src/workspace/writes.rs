//! The paths a conversation's commands may write (docs/contracts/folders.md, WORKSPACE-1, proposed):
//! its workspace and nothing else. In "This folder" that is the project's folder, edited in place;
//! in "A new worktree", the worktree obid made for it under `home/workspaces/`. Never obid's home
//! (but that one worktree), the person's home itself or a folder holding it, or any account's
//! folder.
//!
//! A pure function of canonical paths: it reads no disk and decides only from what it's given.
//! The lanes give the sandbox its list from it (WORKSPACE-1, wired in `jobs/lanes.rs` by its own
//! change); a Kani harness below proves it over any paths.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use super::Workspace;

/// What the write list is checked against, every path canonical.
#[derive(Debug, Clone)]
pub struct Guards {
    /// obid's home.
    pub obid: PathBuf,
    /// Where obid keeps conversations' worktrees (`home/workspaces`), inside obid's home.
    pub worktrees: PathBuf,
    /// The person's home folders.
    pub homes: Vec<PathBuf>,
    /// Every account's own folder (`configDir`), the default logins' too (`~/.claude`, …).
    pub accounts: Vec<PathBuf>,
}

/// The one path a conversation in `workspace` working at `chosen` may write: `[chosen]`, or
/// `None` when `chosen` is a place it never writes. `chosen` and every guard are canonical
/// (absolute, links resolved, no `..`); a path that isn't is refused.
pub fn writes(workspace: Workspace, chosen: &Path, guards: &Guards) -> Option<Vec<PathBuf>> {
    let parts = |p: &Path| components(p);
    let chosen_parts = parts(chosen)?;
    let obid = parts(&guards.obid)?;
    let worktrees = parts(&guards.worktrees)?;
    let homes = guards
        .homes
        .iter()
        .map(|h| parts(h))
        .collect::<Option<Vec<_>>>()?;
    let accounts = (guards.accounts.iter())
        .map(|a| parts(a))
        .collect::<Option<Vec<_>>>()?;
    let homes: Vec<&[OsString]> = homes.iter().map(Vec::as_slice).collect();
    let accounts: Vec<&[OsString]> = accounts.iter().map(Vec::as_slice).collect();
    let places = Places {
        obid: &obid,
        worktrees: &worktrees,
        homes: &homes,
        accounts: &accounts,
    };
    allowed(workspace, &chosen_parts, &places).then(|| vec![chosen.to_path_buf()])
}

/// The guards as component lists, for [`allowed`].
pub(crate) struct Places<'a, C> {
    pub obid: &'a [C],
    pub worktrees: &'a [C],
    pub homes: &'a [&'a [C]],
    pub accounts: &'a [&'a [C]],
}

/// Whether a conversation in `workspace` may write `chosen` (paths as their components, root
/// first): never the whole disk, a person's home or a folder holding one, nor anything touching an
/// account's folder; in a folder, nothing touching obid's home; in a worktree, only a folder strictly
/// inside obid's worktrees, which are themselves inside obid's home.
pub(crate) fn allowed<C: PartialEq>(workspace: Workspace, chosen: &[C], at: &Places<C>) -> bool {
    if chosen.is_empty() || at.homes.iter().any(|home| within(home, chosen)) {
        return false;
    }
    if at.accounts.iter().any(|account| touches(chosen, account)) {
        return false;
    }
    match workspace {
        Workspace::Folder => !touches(chosen, at.obid),
        Workspace::Worktree => {
            within(at.worktrees, at.obid)
                && at.worktrees.len() > at.obid.len()
                && within(chosen, at.worktrees)
                && chosen.len() > at.worktrees.len()
        }
    }
}

/// `path` is `root` or inside it, by whole components.
pub(crate) fn within<C: PartialEq>(path: &[C], root: &[C]) -> bool {
    path.len() >= root.len() && path[..root.len()] == *root
}

/// `a` is `b`, inside it, or holds it.
fn touches<C: PartialEq>(a: &[C], b: &[C]) -> bool {
    within(a, b) || within(b, a)
}

/// `path`'s components after the root, compared as the Mac compares them (ignoring case on
/// macOS, as crate::folders does), or `None` for a path that isn't absolute and canonical.
fn components(path: &Path) -> Option<Vec<OsString>> {
    let mut parts = path.components();
    if parts.next() != Some(Component::RootDir) {
        return None;
    }
    parts
        .map(|part| match part {
            Component::Normal(name) if cfg!(target_os = "macos") => {
                Some(name.to_string_lossy().to_lowercase().into())
            }
            Component::Normal(name) => Some(name.to_owned()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guards() -> Guards {
        Guards {
            obid: "/Users/me/Library/Application Support/Oby/home".into(),
            worktrees: "/Users/me/Library/Application Support/Oby/home/workspaces".into(),
            homes: vec!["/Users/me".into()],
            accounts: vec!["/Users/me/.claude".into(), "/Users/me/work-claude".into()],
        }
    }

    #[test]
    fn a_folder_writes_only_itself() {
        let api = Path::new("/Users/me/Projects/api");
        assert_eq!(
            writes(Workspace::Folder, api, &guards()),
            Some(vec![api.to_path_buf()])
        );
    }

    #[test]
    fn a_worktree_writes_only_itself_inside_obids_worktrees() {
        let tree = Path::new("/Users/me/Library/Application Support/Oby/home/workspaces/c1/api");
        assert_eq!(
            writes(Workspace::Worktree, tree, &guards()),
            Some(vec![tree.to_path_buf()])
        );
        // A worktree is never the project's folder, nor obid's worktrees themselves.
        for path in [
            "/Users/me/Projects/api",
            "/Users/me/Library/Application Support/Oby/home/workspaces",
            "/Users/me/Library/Application Support/Oby/home/vault",
        ] {
            assert_eq!(
                writes(Workspace::Worktree, Path::new(path), &guards()),
                None,
                "{path}"
            );
        }
    }

    #[test]
    fn never_obids_home_the_persons_home_or_an_account() {
        for path in [
            "/",
            "/Users",
            "/Users/me",
            "/Users/me/Library/Application Support/Oby",
            "/Users/me/Library/Application Support/Oby/home",
            "/Users/me/Library/Application Support/Oby/home/workspaces/c1/api",
            "/Users/me/.claude",
            "/Users/me/.claude/projects",
            "/Users/me/work-claude",
            "relative/path",
            "/Users/me/Projects/../.claude",
        ] {
            assert_eq!(
                writes(Workspace::Folder, Path::new(path), &guards()),
                None,
                "{path}"
            );
        }
    }

    #[test]
    fn a_folder_in_another_volume_is_its_own() {
        let elsewhere = Path::new("/Volumes/Work/api");
        assert_eq!(
            writes(Workspace::Folder, elsewhere, &guards()),
            Some(vec![elsewhere.to_path_buf()])
        );
    }
}

/// Kani: for every workspace and any paths of up to three components, each component any byte,
/// the list is the chosen path alone, and it never is nor holds obid's home, a person's home or an
/// account's folder, nor sits inside an account's; a folder never sits inside obid's home, and a
/// worktree only strictly inside obid's worktrees.
#[cfg(kani)]
mod proofs {
    use super::*;

    const N: usize = 3;

    fn any_path(buf: &mut [u8; N]) -> &[u8] {
        *buf = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= N);
        &buf[..len]
    }

    fn any_workspace() -> Workspace {
        if kani::any() {
            Workspace::Folder
        } else {
            Workspace::Worktree
        }
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn the_write_list_holds_the_workspace_and_nothing_guarded() {
        let (mut c, mut o, mut w, mut h, mut a) = ([0; N], [0; N], [0; N], [0; N], [0; N]);
        let chosen = any_path(&mut c);
        let obid = any_path(&mut o);
        let worktrees = any_path(&mut w);
        let homes = [any_path(&mut h)];
        let accounts = [any_path(&mut a)];
        let workspace = any_workspace();
        let at = Places {
            obid,
            worktrees,
            homes: &homes,
            accounts: &accounts,
        };
        if allowed(workspace, chosen, &at) {
            // Never the whole disk, a person's home or what holds it.
            assert!(!chosen.is_empty());
            assert!(!within(homes[0], chosen));
            // Nothing of an account's folder, nor what holds one.
            assert!(!within(chosen, accounts[0]) && !within(accounts[0], chosen));
            // Never obid's home itself, nor anything holding it.
            assert!(!within(obid, chosen));
            match workspace {
                Workspace::Folder => assert!(!within(chosen, obid)),
                Workspace::Worktree => {
                    assert!(within(chosen, worktrees) && chosen.len() > worktrees.len());
                    assert!(within(worktrees, obid) && worktrees.len() > obid.len());
                }
            }
        }
    }
}
