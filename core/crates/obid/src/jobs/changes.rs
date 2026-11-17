//! What a job's branch changed against its base (docs/contracts/tasks.md, TASKS-23, 24, 29 and
//! 30), or in one of its task's rounds (docs/contracts/task-changes.md, CHANGES-1 to 4): its files
//! and its diff, read in its worktree inside its sandbox, at one HEAD; once a done job's worktree
//! was freed (CHANGES-15), in obid's clone at the commit its branch is kept at. It owns no state,
//! and never writes a job, a worktree or a branch.

use super::rounds::Round;
use super::*;

/// How much of a job's diff its changes answer, after redaction (TASKS-24).
const CHANGES_DIFF_MAX: usize = 1024 * 1024;

/// A changed file as git listed it: its path, `added`, `deleted` or `modified`, and its counts.
type Changed = (String, &'static str, u64, u64);

/// What `GET /v1/jobs/{id}/changes` narrows the whole task's changes to.
#[derive(Debug, Default)]
pub struct ChangesAsk<'a> {
    /// One file's, taken literally.
    pub path: Option<&'a str>,
    /// One round's, by its number in `rounds` (CHANGES-2).
    pub round: Option<usize>,
    /// Changes in whitespace alone left out, as `git diff -w` does (CHANGES-3).
    pub ignore_whitespace: bool,
}

impl Jobs {
    /// `GET /v1/jobs/{id}/changes`: the files and the diff the job `id`'s HEAD makes against its
    /// base (`origin/<base>...<head>`), or one of its rounds makes (`<from>..<to>`), only `path`'s
    /// when given, and its task's rounds; none when there's no such job, and nothing to read
    /// answers no files.
    pub async fn changes(&self, id: &str, ask: &ChangesAsk<'_>) -> Option<Value> {
        let job = self.get(id)?;
        let empty = |rounds: Vec<Value>| {
            json!({"base": null, "head": null, "files": [], "diff": "", "truncated": false,
                   "round": ask.round, "rounds": rounds})
        };
        let Some((base, head, clone)) = self.base_and_head(&job).await else {
            return Some(empty(Vec::new()));
        };
        let clone = clone.as_deref();
        let rounds = self.rounds_at(&job, &head, clone).await;
        let listed: Vec<Value> = rounds.iter().map(Round::to_json).collect();
        let range = match ask.round {
            None => format!("refs/remotes/origin/{base}...{head}"),
            Some(n) => match rounds.iter().find(|r| r.number == n) {
                Some(round) => format!("{}..{}", round.from, round.to),
                None => {
                    let mut none = empty(listed);
                    none["base"] = json!(base);
                    none["head"] = json!(head);
                    return Some(none);
                }
            },
        };
        let Some(mut files) = self
            .changed_over(id, clone, &range, ask.ignore_whitespace)
            .await
        else {
            return Some(empty(listed));
        };
        if let Some(path) = ask.path {
            files.retain(|f| f.0 == path);
        }
        let (diff, truncated) = if ask.path.is_some() && files.is_empty() {
            (String::new(), false)
        } else {
            self.change_diff(id, clone, &range, ask).await
        };
        let known = crate::secrets::known();
        let files: Vec<Value> = files
            .iter()
            .map(|(path, status, additions, deletions)| {
                json!({"path": known.redact(path), "status": status,
                       "additions": additions, "deletions": deletions})
            })
            .collect();
        Some(
            json!({"base": base, "head": head, "files": files, "diff": diff,
                    "truncated": truncated, "round": ask.round, "rounds": listed}),
        )
    }

    /// The job's base and the head its changes are read at, read once: its worktree's HEAD, or,
    /// once a job that ended has no worktree (freed, CHANGES-15), the commit its branch is at in
    /// obid's clone, with that clone. None when either can't be read.
    async fn base_and_head(&self, job: &Job) -> Option<(String, String, Option<PathBuf>)> {
        if job.branch.is_empty() {
            return None;
        }
        let base = job
            .base
            .clone()
            .or_else(|| self.repo(&job.repo).map(|r| r.default_branch.clone()))
            .filter(|b| git::is_branch_name(b))?;
        let worktree = self.worktree_path(&job.id);
        if !worktree.join(".git").exists() {
            if job.finished.is_none() || job.status.is_active() {
                return None;
            }
            let clone = self.clone_for(job);
            let head = kept_head(&clone, &job.branch).await?;
            return Some((base, head, Some(clone)));
        }
        let folders = self.pinned_git(&job.id).await.ok()?;
        let head = git::pinned(
            &worktree,
            &folders,
            &["rev-parse", "--verify", "--quiet", "HEAD"],
        )
        .await
        .ok()
        .filter(|h| git::is_sha(h))?;
        Some((base, head, None))
    }

    /// git over `args` where the job `id`'s changes are read: in its worktree inside its sandbox,
    /// or in obid's `clone` once its worktree was freed ([`in_clone`]); refused past `max` bytes.
    async fn changes_git(
        &self,
        id: &str,
        clone: Option<&Path>,
        args: &[&str],
        max: usize,
    ) -> Result<Vec<u8>, String> {
        match clone {
            None => {
                self.sandboxed_git_within(id, args, None, DIFF_LIMIT, max)
                    .await
            }
            Some(clone) => {
                let out = in_clone(clone, args).await?;
                if out.len() > max {
                    return Err(format!("git printed more than {max} bytes"));
                }
                Ok(out)
            }
        }
    }

    /// The files `range` changes, in git's order; none when they can't be read.
    async fn changed_over(
        &self,
        id: &str,
        clone: Option<&Path>,
        range: &str,
        ignore_whitespace: bool,
    ) -> Option<Vec<Changed>> {
        let mut args = vec![
            "diff",
            "--raw",
            "--numstat",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
        ];
        if ignore_whitespace {
            args.push("--ignore-all-space");
        }
        args.extend([range, "--"]);
        let out = self
            .changes_git(id, clone, &args, DIFF_OUTPUT_MAX)
            .await
            .ok()?;
        // `:<modes> <shas> <letter>` then its path, for each file; then `<added>\t<deleted>\t<path>`.
        let mut files: Vec<Changed> = Vec::new();
        let mut fields = out.split(|b| *b == 0).filter(|f| !f.is_empty());
        while let Some(field) = fields.next() {
            let field = String::from_utf8_lossy(field);
            if let Some(raw) = field.strip_prefix(':') {
                let status = match raw.chars().last() {
                    Some('A') => "added",
                    Some('D') => "deleted",
                    _ => "modified",
                };
                let path = String::from_utf8_lossy(fields.next()?).into_owned();
                files.push((path, status, 0, 0));
            } else if let Some((added, rest)) = field.split_once('\t')
                && let Some((deleted, path)) = rest.split_once('\t')
                && let Some(file) = files.iter_mut().find(|f| f.0 == path)
            {
                // A binary file's `-` counts 0.
                file.2 = added.parse().unwrap_or(0);
                file.3 = deleted.parse().unwrap_or(0);
            }
        }
        Some(files)
    }

    /// The diff over `range` (only `ask`'s path's, taken literally), redacted and then cut to
    /// [`CHANGES_DIFF_MAX`], and whether any was cut: none, and cut, when it can't be read whole.
    async fn change_diff(
        &self,
        id: &str,
        clone: Option<&Path>,
        range: &str,
        ask: &ChangesAsk<'_>,
    ) -> (String, bool) {
        let mut args = vec![
            "--literal-pathspecs",
            "diff",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
        ];
        if ask.ignore_whitespace {
            args.push("--ignore-all-space");
        }
        args.extend([range, "--"]);
        args.extend(ask.path);
        let Ok(out) = self.changes_git(id, clone, &args, CHANGED_MAX).await else {
            return (String::new(), true);
        };
        let mut text = crate::secrets::known().redact(&String::from_utf8_lossy(&out));
        let (keep, truncated) = diff_cut(&text, CHANGES_DIFF_MAX);
        text.truncate(keep);
        (text, truncated)
    }
}

/// git over `args` in obid's `clone`, as the secret scan reads it there: its own configuration
/// only (no hooks, no fsmonitor, nothing from the person's or the system's), objects as they are.
/// Its output, or why it failed (CHANGES-15).
pub(crate) async fn in_clone(clone: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut all = vec![
        "--no-replace-objects",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
    ];
    all.extend_from_slice(args);
    let all: Vec<&std::ffi::OsStr> = all.iter().map(std::ffi::OsStr::new).collect();
    git::run_bytes(clone, &all, crate::publish::SCAN_ENV, None)
        .await
        .map_err(|e| e.0)
}

/// The commit `branch` is at in obid's `clone`: what obid kept of a job whose worktree was freed.
async fn kept_head(clone: &Path, branch: &str) -> Option<String> {
    if !git::is_branch_name(branch) || !clone.join(".git").exists() {
        return None;
    }
    let commit = format!("refs/heads/{branch}^{{commit}}");
    let out = in_clone(clone, &["rev-parse", "--verify", "--quiet", &commit])
        .await
        .ok()?;
    Some(String::from_utf8_lossy(&out).trim().to_owned()).filter(|h| git::is_sha(h))
}

/// TASKS-24: how much of `text` stays within `cap` bytes, cut on a character, and whether any was cut.
pub(crate) fn diff_cut(text: &str, cap: usize) -> (usize, bool) {
    if text.len() <= cap {
        return (text.len(), false);
    }
    let mut end = cap;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (end, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TASKS-24: a text at or under the cap stays whole.
    #[test]
    fn tasks_24_diff_cut_keeps_a_text_within_the_cap_whole() {
        assert_eq!(diff_cut("", 0), (0, false));
        assert_eq!(diff_cut("abc", 3), (3, false));
        assert_eq!(diff_cut("abc", 10), (3, false));
    }

    /// TASKS-24: a cap inside a 2-, 3- or 4-byte character cuts before that character.
    #[test]
    fn tasks_24_diff_cut_cuts_before_a_character_the_cap_splits() {
        for wide in ["é", "€", "𝄞"] {
            let text = format!("ab{wide}cd");
            for cap in 3..2 + wide.len() {
                assert_eq!(diff_cut(&text, cap), (2, true), "{wide} at {cap}");
            }
            assert_eq!(diff_cut(&text, 2 + wide.len()), (2 + wide.len(), true));
        }
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proof of [`diff_cut`] (TASKS-24): `kani::any()` stands for every text of up to 8
    //! bytes and every cap.

    use super::*;

    /// TASKS-24: the cut is never over the cap, says it cut exactly when it did, and never splits a character.
    #[kani::proof]
    #[kani::unwind(10)]
    fn tasks_24_a_cut_diff_stays_within_the_cap_on_a_character() {
        let bytes: [u8; 8] = kani::any();
        let len: usize = kani::any();
        let cap: usize = kani::any();
        kani::assume(len <= 8);
        let Ok(text) = std::str::from_utf8(&bytes[..len]) else {
            return;
        };
        let (keep, truncated) = diff_cut(text, cap);
        assert!(keep <= cap);
        assert_eq!(truncated, keep < text.len());
        assert!(text.is_char_boundary(keep));
        if truncated {
            assert!(cap - keep < 4);
        } else {
            assert_eq!(keep, text.len());
        }
    }
}
