//! The Mac's own paths taken out of what obid sends to GitHub (docs/contracts/publishing.md,
//! PUBLISHING-39 to 41): a pull request's title and description, the squash commit Oby writes,
//! and Oby's review. It runs right before the secret scan, on the same texts, so what the scan
//! reads is what goes out.
//!
//! A path inside a job's worktree becomes the path in the repository (`calc.py`, not
//! `/Users/<name>/Library/Application Support/Oby/Runner/worktrees/<job>/calc.py`), one inside
//! any job's worktree under obid's `worktrees` folder too, and any other path under the person's
//! home folder becomes `~/…`. Agents link files by their full path; on GitHub that path names the
//! Mac's user and Oby's home, which "Privacy first, local first" keeps on the Mac.
//!
//! It owns no state and writes nothing: it rewrites the text it's given. Words someone else wrote
//! in an agent's commits stay as they are (they are the agent's commits, pushed as they are), and
//! so do the owner's own signed words (`brain::owners_word`), which go as they were shown.

use std::path::{Path, PathBuf};

/// Where the Mac's own paths start, longest first when rewritten.
#[derive(Debug, Clone, Default)]
pub struct Places {
    /// The job's own worktree(s): a path in one becomes the path in the repository.
    worktrees: Vec<String>,
    /// obid's `worktrees` folder: a path in any job's worktree there becomes the repository's.
    roots: Vec<String>,
    /// The person's home folders: anything else under one becomes `~/…`.
    homes: Vec<String>,
}

impl Places {
    /// For a job's texts: its worktree, the folder of worktrees it's in, and the person's homes.
    pub fn of_job(worktree: Option<&str>) -> Places {
        let mut places = Places::default();
        if let Some(worktree) = worktree.map(Path::new).filter(|p| p.is_absolute()) {
            places.add_worktree(worktree);
            if let Some(root) = worktree.parent() {
                places.add_root(root);
            }
        }
        for home in crate::jobs::person_homes() {
            places.add_home(&home);
        }
        places
    }

    /// For obid's texts that name no job: every job's worktree under `home` (obid's), and the
    /// person's homes.
    pub fn of_home(home: &Path) -> Places {
        let mut places = Places::of_job(None);
        places.add_root(&crate::api::worktrees(home));
        places
    }

    pub fn add_worktree(&mut self, path: &Path) {
        push_forms(&mut self.worktrees, path);
    }

    pub fn add_root(&mut self, path: &Path) {
        push_forms(&mut self.roots, path);
    }

    pub fn add_home(&mut self, path: &Path) {
        push_forms(&mut self.homes, path);
    }

    /// `text` with every path this names rewritten: `<worktree>/a/b.py` and
    /// `file://<worktree>/a/b.py` read `a/b.py` (the worktree itself `.`), `<root>/<job>/a/b.py`
    /// reads `a/b.py`, and `<home>/x` reads `~/x` (the home itself `~`). A prefix counts only
    /// where a path starts (not after a letter, digit or `/`) and ends at a `/` or past the path,
    /// so `/Users/al` never takes `/Users/alice`'s.
    pub fn rewrite(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for (list, kind) in [
            (&self.worktrees, Kind::Worktree),
            (&self.roots, Kind::Root),
            (&self.homes, Kind::Home),
        ] {
            for prefix in list {
                out = replace_prefix(&out, prefix, kind);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Worktree,
    Root,
    Home,
}

/// The path as written, `%20`-encoded (as a Markdown link writes a space) and as the file system
/// resolves it (`/var` is `/private/var` on a Mac), each kept once, longest first.
fn push_forms(list: &mut Vec<String>, path: &Path) {
    let mut forms: Vec<PathBuf> = vec![path.to_path_buf()];
    if let Ok(real) = std::fs::canonicalize(path) {
        forms.push(real);
    }
    for form in forms {
        let plain = form.to_string_lossy().trim_end_matches('/').to_owned();
        if plain.len() < 2 {
            continue;
        }
        for form in [plain.replace(' ', "%20"), plain] {
            if !list.contains(&form) {
                list.push(form);
            }
        }
    }
    list.sort_by_key(|p| std::cmp::Reverse(p.len()));
}

/// Every `prefix` in `text` that starts a path rewritten as `kind` says, a `file://` right before
/// it taken out too; any other occurrence is kept.
fn replace_prefix(text: &str, prefix: &str, kind: Kind) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        let (before, from) = rest.split_at(at);
        let after = &from[prefix.len()..];
        let lead = before.strip_suffix("file://").unwrap_or(before);
        let starts_path = !lead
            .chars()
            .next_back()
            .is_some_and(|c| is_path_char(c) || c == '/');
        let rewritten = match (starts_path, after.strip_prefix('/'), kind) {
            (false, _, _) => None,
            (true, Some(tail), Kind::Worktree) => Some(("", tail)),
            (true, Some(tail), Kind::Home) => Some(("~/", tail)),
            // `<root>/<job>/rest`: the job's folder name goes with the root.
            (true, Some(tail), Kind::Root) => tail
                .find('/')
                .filter(|&n| n > 0 && tail[..n].chars().all(is_path_char))
                .map(|n| ("", &tail[n + 1..])),
            (true, None, _) if after.starts_with(is_path_char) => None,
            (true, None, Kind::Worktree) => Some((".", after)),
            (true, None, Kind::Home) => Some(("~", after)),
            (true, None, Kind::Root) => None,
        };
        if let Some((with, tail)) = rewritten {
            out.push_str(lead);
            out.push_str(with);
            rest = tail;
        } else {
            // Not a path's start: keep this occurrence and look past its first character.
            let step = from.chars().next().map_or(1, char::len_utf8);
            out.push_str(before);
            out.push_str(&from[..step]);
            rest = &from[step..];
        }
    }
    out.push_str(rest);
    out
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '%')
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/someone";
    const WORKTREE: &str =
        "/Users/someone/Library/Application Support/Oby/Runner/worktrees/3f2a9c1e";

    fn places() -> Places {
        let mut places = Places::default();
        places.add_worktree(Path::new(WORKTREE));
        places.add_root(Path::new(
            "/Users/someone/Library/Application Support/Oby/Runner/worktrees",
        ));
        places.add_home(Path::new(HOME));
        places
    }

    /// PUBLISHING-39: an agent's summary that links its files by their worktree path reads them
    /// by their path in the repository, Markdown links, `file://` links and `%20` spaces too.
    #[test]
    fn publishing_39_worktree_paths_become_repository_paths() {
        let summary = format!(
            "Added [calc.py]({WORKTREE}/calc.py) and [the test](file://{WORKTREE}/tests/test_calc.py:12).\n\
             Also see [notes]({}/docs/notes.md) and `{WORKTREE}/README.md`.\n\
             Ran it in `{WORKTREE}`.",
            WORKTREE.replace(' ', "%20")
        );
        let out = places().rewrite(&summary);
        assert_eq!(
            out,
            "Added [calc.py](calc.py) and [the test](tests/test_calc.py:12).\n\
             Also see [notes](docs/notes.md) and `README.md`.\n\
             Ran it in `.`."
        );
        assert!(!out.contains("/Users/"), "{out}");
    }

    /// PUBLISHING-39: another job's worktree under obid's `worktrees` reads as the repository's
    /// path too, its job's folder name dropped.
    #[test]
    fn publishing_39_another_jobs_worktree_too() {
        let out = places().rewrite(
            "Its first round: /Users/someone/Library/Application Support/Oby/Runner/worktrees/77aa/src/main.rs:3",
        );
        assert_eq!(out, "Its first round: src/main.rs:3");
    }

    /// PUBLISHING-40: any other path under the person's home reads `~/…`, and the home itself `~`.
    #[test]
    fn publishing_40_home_paths_become_tilde() {
        let out = places().rewrite(
            "Logs are in /Users/someone/Library/Logs/x.log and (/Users/someone). file:///Users/someone/a.txt",
        );
        assert_eq!(out, "Logs are in ~/Library/Logs/x.log and (~). ~/a.txt");
    }

    /// PUBLISHING-40: only where a path starts, and only the whole name: another person's home,
    /// a longer name, or the same words inside a URL stay as they are.
    #[test]
    fn publishing_40_only_whole_paths() {
        let text = "/Users/someoneelse/x, https://example.com/Users/someone/y, a/Users/someone/z, /Users/Shared/q";
        assert_eq!(places().rewrite(text), text);
    }

    /// Text with no path in it is unchanged.
    #[test]
    fn text_with_no_path_is_unchanged() {
        let text = "Fixed the parser.\n\n> quoted\n- Check: `make test` passed.";
        assert_eq!(places().rewrite(text), text);
    }
}
