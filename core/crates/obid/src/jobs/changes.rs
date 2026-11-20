//! What a job's branch changed against its base (docs/contracts/tasks.md, TASKS-23, 24, 29 and
//! 30), or in one of its task's rounds (docs/contracts/task-changes.md, CHANGES-1 to 4): its files
//! and its diff, read in its worktree inside its sandbox, at one HEAD. It owns no state, and never
//! writes a job, a worktree or a branch.

pub(crate) mod rounds;

use super::*;
use rounds::Round;

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
        let Some((base, head)) = self.base_and_head(&job).await else {
            return Some(empty(Vec::new()));
        };
        let rounds = self.rounds_at(&job, &head).await;
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
        let mut files = match self.changed_over(id, &range, ask.ignore_whitespace).await {
            Ok(files) => files,
            Err(why) => {
                tracing::warn!("job {id}: its changed files couldn't be read: {why}");
                let mut none = empty(listed);
                none["error"] = json!(
                    crate::secrets::known()
                        .redact(&format!("Oby couldn't read its changed files: {why}"))
                );
                return Some(none);
            }
        };
        if let Some(path) = ask.path {
            files.retain(|f| f.0 == path);
        }
        let (diff, truncated, error) = if ask.path.is_some() && files.is_empty() {
            (String::new(), false, None)
        } else {
            match self.change_diff(id, &range, ask).await {
                Ok((diff, truncated)) => (diff, truncated, None),
                Err(why) => {
                    tracing::warn!("job {id}: its diff couldn't be read: {why}");
                    (
                        String::new(),
                        true,
                        Some(format!("Oby couldn't read its diff: {why}")),
                    )
                }
            }
        };
        let known = crate::secrets::known();
        let files: Vec<Value> = files
            .iter()
            .map(|(path, status, additions, deletions)| {
                json!({"path": known.redact(path), "status": status,
                       "additions": additions, "deletions": deletions})
            })
            .collect();
        let mut answer = json!({"base": base, "head": head, "files": files, "diff": diff,
                                "truncated": truncated, "round": ask.round, "rounds": listed});
        if let Some(error) = error {
            answer["error"] = json!(crate::secrets::known().redact(&error));
        }
        Some(answer)
    }

    /// The job's base and its worktree's HEAD, read once; none when either can't be read.
    async fn base_and_head(&self, job: &Job) -> Option<(String, String)> {
        let worktree = self.worktree_path(&job.id);
        if job.branch.is_empty() || !worktree.join(".git").exists() {
            return None;
        }
        let base = job
            .base
            .clone()
            .or_else(|| self.repo(&job.repo).map(|r| r.default_branch.clone()))
            .filter(|b| git::is_branch_name(b))?;
        let folders = self.pinned_git(&job.id).await.ok()?;
        let head = git::pinned(
            &worktree,
            &folders,
            &["rev-parse", "--verify", "--quiet", "HEAD"],
        )
        .await
        .ok()
        .filter(|h| git::is_sha(h))?;
        Some((base, head))
    }

    /// The files `range` changes, in git's order; why not, when they can't be read.
    async fn changed_over(
        &self,
        id: &str,
        range: &str,
        ignore_whitespace: bool,
    ) -> Result<Vec<Changed>, String> {
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
            .sandboxed_git_within(id, &args, None, DIFF_LIMIT, DIFF_OUTPUT_MAX)
            .await?;
        listed(&out, ignore_whitespace)
    }

    /// The diff over `range` (only `ask`'s path's, taken literally), redacted and then cut to
    /// [`CHANGES_DIFF_MAX`], and whether any was cut; why not, when it can't be read whole (over
    /// [`CHANGED_MAX`], or git failed).
    async fn change_diff(
        &self,
        id: &str,
        range: &str,
        ask: &ChangesAsk<'_>,
    ) -> Result<(String, bool), String> {
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
        let out = self
            .sandboxed_git_within(id, &args, None, DIFF_LIMIT, CHANGED_MAX)
            .await?;
        let mut text = crate::secrets::known().redact(&String::from_utf8_lossy(&out));
        let (keep, truncated) = diff_cut(&text, CHANGES_DIFF_MAX);
        text.truncate(keep);
        Ok((text, truncated))
    }
}

/// The files `git diff --raw --numstat -z` lists in `out`, in git's order, each with its counts.
/// With `ignore_whitespace`, a file only whitespace changed is left out on any git (CHANGES-3).
fn listed(out: &[u8], ignore_whitespace: bool) -> Result<Vec<Changed>, String> {
    // `:<modes> <shas> <letter>` then its path, for each file; then `<added>\t<deleted>\t<path>`.
    let mut files: Vec<Changed> = Vec::new();
    // The files only whitespace changed, as an older git lists them under
    // `--ignore-all-space`: git 2.43 keeps them in `--raw` with no `--numstat` line, where
    // 2.54 and later leave them out. A plain `M` with its mode kept, and counted `0 0` or not
    // at all (a binary's is `-`).
    let mut counted: Vec<String> = Vec::new();
    let mut contentless: Vec<String> = Vec::new();
    let mut kept_mode: Vec<String> = Vec::new();
    let mut fields = out.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let Some(field) = fields.next() {
        let field = String::from_utf8_lossy(field);
        if let Some(raw) = field.strip_prefix(':') {
            let letter = raw.rsplit(' ').next().unwrap_or_default();
            let status = match letter {
                "A" => "added",
                "D" => "deleted",
                _ => "modified",
            };
            let Some(path) = fields.next() else {
                return Err(format!("git listed {raw} with no path"));
            };
            let path = String::from_utf8_lossy(path).into_owned();
            let mut modes = raw.split(' ');
            if letter == "M" && modes.next().is_some_and(|old| modes.next() == Some(old)) {
                kept_mode.push(path.clone());
            }
            files.push((path, status, 0, 0));
        } else if let Some((added, rest)) = field.split_once('\t')
            && let Some((deleted, path)) = rest.split_once('\t')
            && let Some(file) = files.iter_mut().find(|f| f.0 == path)
        {
            counted.push(path.to_owned());
            if (added, deleted) == ("0", "0") {
                contentless.push(path.to_owned());
            }
            // A binary file's `-` counts 0.
            file.2 = added.parse().unwrap_or(0);
            file.3 = deleted.parse().unwrap_or(0);
        }
    }
    // Under `--ignore-all-space`, leave out what git 2.54 and later leave out themselves, on
    // any git (CHANGES-3): a mode change, an empty new file or a binary stays listed.
    if ignore_whitespace {
        let whitespace_only = |path: &String| {
            kept_mode.contains(path) && (!counted.contains(path) || contentless.contains(path))
        };
        files.retain(|f| !whitespace_only(&f.0));
    }
    Ok(files)
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

    /// CHANGES-3: under `--ignore-all-space`, git 2.43 lists a re-indented file in `--raw` with no
    /// `--numstat` line, where git 2.54 leaves it out: both read the same. A binary (`-`), a file
    /// whose mode alone changed and an empty new file (`0 0` each) stay listed on both.
    #[test]
    fn changes_3_whitespace_alone_is_left_out_on_any_git() {
        let older = ":100644 100644 8352675 1592e5c M\0bin.dat\0:000000 100644 0000000 e69de29 A\0empty.txt\0:100644 100644 422c2b7 0cd5ff0 M\0f.js\0:100644 100755 587be6b 587be6b M\0mode.sh\0-\t-\tbin.dat\x000\t0\tempty.txt\x000\t0\tmode.sh\0";
        let newer = ":100644 100644 8352675 1592e5c M\0bin.dat\0:000000 100644 0000000 e69de29 A\0empty.txt\0:100644 100755 587be6b 587be6b M\0mode.sh\0-\t-\tbin.dat\x000\t0\tempty.txt\x000\t0\tmode.sh\0";
        let names = |out: &str, ignore| -> Vec<String> {
            listed(out.as_bytes(), ignore)
                .unwrap()
                .into_iter()
                .map(|f| f.0)
                .collect()
        };
        for out in [older, newer] {
            assert_eq!(
                names(out, true),
                ["bin.dat", "empty.txt", "mode.sh"],
                "{out:?}"
            );
        }
        // Not asked to ignore whitespace: what git lists stays, the re-indented file too.
        assert_eq!(
            names(older, false),
            ["bin.dat", "empty.txt", "f.js", "mode.sh"]
        );
        // An addition or a deletion counted `0 0` (an empty file) stays.
        let gone = ":100644 000000 e69de29 0000000 D\0was.txt\x000\t0\twas.txt\0";
        assert_eq!(names(gone, true), ["was.txt"]);
    }

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
