//! An own run's branches and stash, its own (LANES-8, the owner's LANE-10a): git keeps a
//! worktree's refs in the folder its `commondir` names, whatever `GIT_COMMON_DIR` says. For an own
//! run obid points it at `oby-common` in the worktree's git folder: links to the clone's `.git` but
//! for its refs, which are the job's beside it (`oby-refs`: its branches and stash, and links to
//! the clone's remotes and tags; `oby-logs`; `oby-packed-refs`, the clone's but its branches and
//! stash). After the run, and before any other git call of obid's on the job, `commondir` names the
//! clone's folder again, so obid never reads a ref an agent wrote and the clone's stay obid's alone.

use super::*;

/// The folder [`own_refs`] makes in the worktree's own git folder.
pub(crate) const OWN_COMMON: &str = "oby-common";
/// Where [`settle_own_refs`] keeps the branch an own run left HEAD on.
const OWN_HEAD: &str = "oby-head";
/// HEAD when its branch names no commit: no namespace obid or the clone writes, so it never resolves.
const UNBORN: &str = "refs/oby/unborn";
/// Why obid reads none of a job's git: setting its worktree back to the clone's refs failed.
const UNSETTLED: &str = "obid can't set the job's git back to its clone's";

/// The own run under way for a job ([`Jobs::own_turn`]): only while it lives does obid read the
/// job's git through its refs; it sets them back when its turn ends ([`OwnRun::settle`]), and a
/// turn cut short is set back by obid's next git call on the job.
pub(super) struct OwnRun<'a> {
    jobs: &'a Jobs,
    id: String,
    dirs: (PathBuf, PathBuf),
}

impl OwnRun<'_> {
    pub(super) async fn settle(&self) -> Result<(), String> {
        let worktree = self.jobs.worktree_path(&self.id);
        settle_own_refs(&self.dirs.0, &self.dirs.1, &worktree)
            .await
            .map_err(|e| format!("{UNSETTLED}: {e}"))
    }
}

impl Drop for OwnRun<'_> {
    fn drop(&mut self) {
        self.jobs
            .own_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

impl Jobs {
    /// Before a turn: the job's git as obid reads it, and for an own run (`own`) its own refs.
    pub(super) async fn own_turn(&self, id: &str, own: bool) -> Result<Option<OwnRun<'_>>, String> {
        let dirs = match self.own_git_dirs(id).await {
            Ok(dirs) if own => dirs,
            Err(why) if why.starts_with(UNSETTLED) => return Err(why),
            _ => return Ok(None),
        };
        self.own_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_owned());
        let run = OwnRun {
            jobs: self,
            id: id.to_owned(),
            dirs,
        };
        own_refs(&run.dirs.0, &run.dirs.1, &self.worktree_path(id))
            .await
            .map_err(|e| format!("obid can't set up the job's git: {e}"))?;
        Ok(Some(run))
    }

    /// The git directories of the job's worktree (its own and the common one), only when they're
    /// obid's: the clone's `.git`, and a folder directly in its `worktrees/` whose `gitdir` names
    /// this worktree (never another job's). The worktree's `.git` is the agent's to write, and
    /// could name any repository. Outside an own run, a worktree still on `oby-common` (obid went
    /// down mid-run, its task panicked, setting it back failed) is set back first; when that
    /// fails, obid reads none of the job's git.
    pub(super) async fn own_git_dirs(&self, id: &str) -> Result<(PathBuf, PathBuf), String> {
        let job = self.get(id).ok_or("the job is gone")?;
        let worktree = self.worktree_path(id);
        let (admin, common) = git::worktree_dirs(&worktree)
            .await
            .map_err(|e| e.to_string())?;
        let ours = self.clone_for(&job).join(".git").canonicalize();
        let this = worktree.join(".git").canonicalize();
        let (admin, common) = match (admin.canonicalize(), common.canonicalize(), ours, this) {
            (Ok(admin), Ok(common), Ok(ours), Ok(this))
                if (common == ours || common == admin.join(OWN_COMMON))
                    && admin.parent() == Some(ours.join("worktrees").as_path())
                    && std::fs::read_to_string(admin.join("gitdir"))
                        .ok()
                        .and_then(|named| Path::new(named.trim()).canonicalize().ok())
                        .as_ref()
                        == Some(&this) =>
            {
                (admin, ours)
            }
            _ => return Err(NOT_OUR_GIT.into()),
        };
        if !self
            .own_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(id)
        {
            settle_own_refs(&admin, &common, &worktree)
                .await
                .map_err(|e| format!("{UNSETTLED}: {e}"))?;
        }
        Ok((admin, common))
    }
}

/// Before an own run: `oby-common` made again (its commands can't write it), `oby-refs` holding
/// only the job's branches and stash besides fresh links to the clone's other refs, and HEAD back
/// on the branch the last run left it on while that branch is still at HEAD.
pub(crate) async fn own_refs(admin: &Path, common: &Path, worktree: &Path) -> std::io::Result<()> {
    let shared = admin.join(OWN_COMMON);
    gone(&shared)?;
    let [refs, logs, packed] = ["oby-refs", "oby-logs", "oby-packed-refs"].map(|f| admin.join(f));
    links(
        common,
        &shared,
        &["refs", "logs", "packed-refs", "worktrees"],
    )?;
    links(&common.join("refs"), &refs, &["heads", "stash"])?;
    own_folder(&logs)?;
    // ponytail: a fresh copy every run, so refs its commands packed themselves go with it.
    let clone_packed = std::fs::read_to_string(common.join("packed-refs")).unwrap_or_default();
    let kept: String = clone_packed
        .lines()
        .filter(|line| !line.ends_with(" refs/stash") && !line.contains(" refs/heads/"))
        .map(|line| format!("{line}\n"))
        .collect();
    write_fresh(&packed, kept.as_bytes())?;
    for (to, name) in [(&refs, "refs"), (&logs, "logs"), (&packed, "packed-refs")] {
        std::os::unix::fs::symlink(to, shared.join(name))?;
    }
    // Relative, as git writes it: the clone's folder can move (a marked clone made again).
    write_fresh(
        &admin.join("commondir"),
        format!("{OWN_COMMON}\n").as_bytes(),
    )?;
    let Ok(left) = std::fs::read_to_string(admin.join(OWN_HEAD)) else {
        return Ok(());
    };
    std::fs::remove_file(admin.join(OWN_HEAD))?;
    let branch = left.trim().strip_prefix("ref: ").unwrap_or_default();
    if git::is_branch_name(branch.strip_prefix("refs/heads/").unwrap_or_default())
        && head_of(admin) == resolved(admin, common, worktree, branch).await
    {
        write_fresh(&admin.join("HEAD"), format!("ref: {branch}\n").as_bytes())?;
    }
    Ok(())
}

/// After an own run, and before any other git call of obid's on the job: `commondir` names the
/// clone's folder again (`../..`, as git writes it). A HEAD on one of the run's branches is
/// detached at its commit, the branch kept in `oby-head`; one on a branch the job doesn't have
/// names no commit ([`UNBORN`]), never the clone's branch of that name.
pub(crate) async fn settle_own_refs(
    admin: &Path,
    common: &Path,
    worktree: &Path,
) -> std::io::Result<()> {
    let pointer = admin.join("commondir");
    let named = std::fs::read_to_string(&pointer).unwrap_or_default();
    if admin.join(named.trim()) != admin.join(OWN_COMMON) {
        return Ok(());
    }
    let head = std::fs::read_to_string(admin.join("HEAD")).unwrap_or_default();
    if head.starts_with("ref: ") {
        let commit = resolved(admin, common, worktree, "HEAD").await;
        if git::is_sha(&commit) {
            write_fresh(&admin.join(OWN_HEAD), head.as_bytes())?;
        }
        let detached = match git::is_sha(&commit) {
            true => format!("{commit}\n"),
            false => format!("ref: {UNBORN}\n"),
        };
        write_fresh(&admin.join("HEAD"), detached.as_bytes())?;
    }
    write_fresh(&pointer, b"../..\n")
}

/// `name`'s commit as the worktree sees it now, or nothing.
async fn resolved(admin: &Path, common: &Path, worktree: &Path, name: &str) -> String {
    let args = ["rev-parse", "--verify", "--quiet", name];
    let folders = git_folders(admin, common, worktree);
    let found = git::pinned(worktree, &folders, &args)
        .await
        .unwrap_or_default();
    if git::is_sha(&found) {
        found
    } else {
        String::new()
    }
}

/// The worktree's HEAD as it's written, when it's a commit; else nothing.
fn head_of(admin: &Path) -> String {
    let head = std::fs::read_to_string(admin.join("HEAD")).unwrap_or_default();
    Some(head.trim().to_owned())
        .filter(|h| git::is_sha(h))
        .unwrap_or_else(|| "-".into())
}

/// Whatever is at `path` gone, never followed.
fn gone(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => Ok(()),
    }
}

/// `to` a folder of its own: anything else there goes first, never followed.
fn own_folder(to: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(to).is_ok_and(|meta| !meta.is_dir()) {
        std::fs::remove_file(to)?;
    }
    if std::fs::symlink_metadata(to).is_err() {
        std::fs::create_dir(to)?;
    }
    Ok(())
}

/// `to` a folder of its own holding only a link to each of `from`'s entries (none when it's
/// missing), and what it held named in `but`: the rest goes first, never followed, so an own run's
/// remotes and tags are the clone's however its commands left them, wherever the clone moved.
fn links(from: &Path, to: &Path, but: &[&str]) -> std::io::Result<()> {
    own_folder(to)?;
    let kept = |name: &std::ffi::OsStr| but.iter().any(|but| name == *but);
    for entry in std::fs::read_dir(to)?.flatten() {
        if !kept(&entry.file_name()) {
            gone(&entry.path())?;
        }
    }
    for entry in std::fs::read_dir(from).into_iter().flatten().flatten() {
        if !kept(&entry.file_name()) {
            std::os::unix::fs::symlink(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}
