---
name: verify-security-paths
description: Re-run MERGING-15's bounded-exhaustive test of the security-path match and read what it covers. Use when a change touches `SECURITY_PATHS`, `caught_by`, `security_paths`, `fits` or `Jobs::changed_files`.
---

# Verifying the security-path match (MERGING-15)

## 1. The property and the harm

A changed path is caught exactly when it is on an entry of `SECURITY_PATHS` (core/crates/obid/src/merge.rs), in any letter case, and is not caught otherwise:

- a name entry (`AGENTS.md`) when it is the path's last part, at any depth;
- a folder entry (`.claude/`) when the path lies inside that folder at any depth, or is the folder's own name (`.claude`);
- a whole-path entry (`docs/build-and-run.md`) when it is the whole path from the root;
- `security_paths(changed, None)` returns exactly the changed paths some entry catches, in their order.

The harm when it breaks: a change to `AGENTS.md`, `.claude/` or CI merges by itself after one review, where two approving reviews by different tools were needed.

## 2. What the test covers, and its bound

The tests in `merge::tests` build every path from the `SECURITY_PATHS` array itself, each with the answer it should get from how it was built:

- **Kind:** an entry ending in `/` is a folder, one with a `/` inside a whole path, any other a name, as `SECURITY_PATHS`'s doc states.
- **Letter case:** every upper/lower assignment of an entry's ASCII letters when it has at most 10; for a longer one, every one-letter flip, every two-letter flip, all upper and all lower.
- **Prefixes:** `""`, `a/`, `src/a/`, `x/y/z/` (depth 0 to 3) and `.github/`. Names and folders are caught at every prefix, whole paths only at `""`.
- **Folder suffixes:** `f` and `a/b/f`, caught. No suffix (the folder's own name) is caught too, in its own test `merging_15_a_folder_entrys_own_name_is_caught`.
- **Near misses**, caught by no entry, at every prefix:
  - one character added before (`xAGENTS.md`, `x.claude/f`, `xdocs/build-and-run.md`);
  - one added after (`AGENTS.mdx`, `AGENTS.md.bak`, `docs/build-and-run.mdx`; a folder's before its `/`, `.claudex/f`);
  - one removed, at each position (never a folder's `/`);
  - a folder's name with `.json` for its `/` (`.claude.json`);
  - a name used as a folder (`AGENTS.md/f`);
  - a whole path under another folder (`a/docs/build-and-run.md`).
- **Not covered:** the `check_files` half of `security_paths` (`command: Some(..)`), paths longer or deeper than these, more than two flips on entries over 10 letters, and non-ASCII letters other than `ſ` and `K`.

## 3. The input assumption

Paths are what `Jobs::changed_files` (core/crates/obid/src/jobs/worktree_git.rs) reads from `git diff --name-only --no-renames -z`: never a `./`, a `//`, or a leading or trailing `/`.

## 4. This Mac's answer on `ſ` and `K`

```
d=$(mktemp -d) && cd "$d" && touch Jenkinsfile "$(printf 'Jen\xe2\x84\xaainsfile')" "$(printf 'Jenkin\xc5\xbffile')" && ls | wc -l
       1
```

APFS takes `ſ` (U+017F) for `s` and `K` (U+212A) for `k`: one name. So each entry with one `s`/`S` as `ſ`, or one `k`/`K` as `K`, is expected caught. `caught_by` folds only ASCII, so they are a finding (section 5); a whole path of them under another folder stays a near miss.

## 5. Open findings

- **V01-40** (`factory/queue/tasks/V01-40.md`): an entry in Unicode case folding (`AGENTſ.md`, `.buildKite/f`) is not caught. Test: `merging_15_a_unicode_case_fold_of_an_entry_is_caught`, `#[ignore = "finding V01-40"]`.

When a fix lands, its `#[ignore]` line goes and this section loses its line.

## 6. Sync

SYNC: 09197a491f54cc132e0146e708f07bb7a3e74924

## 7. The one command

```
cd core && cargo test -p obid --lib merge::tests::merging_15_
```

Add `-- --ignored` to run the findings' tests, red until they are fixed.

## 8. A bug or a test error

A failure names the path, the entry and the expected answer, as `".CLAUDE/f" on entry ".claude/" expected caught`. If the expected answer is wrong for that path by the property in section 1, the generator (`security_path_cases` in `merge::tests`) is in error: fix its shape, or drop that one shape with a comment saying why. Anything else is a bug in `caught_by` or `security_paths`. Never weaken an expected answer to match the code.

## 9. New entries

A new `SECURITY_PATHS` entry is covered by itself: the tests read the array, and each asserts that every entry produced at least one case.

## 10. The test still bites

On a committed tree, apply one break to `caught_by` in merge.rs, run the command in section 7, see `merging_15_every_entry_is_caught_in_any_case_and_depth` fail, then `git restore core/crates/obid/src/merge.rs` and see it green. Never commit a break.

1. `let lower = path.to_ascii_lowercase();` → `let lower = path.to_owned();` (letter case counts).
2. `.any(|(i, b)| b == b'/' && on(&lower[i + 1..]))` → `.any(|(i, b)| i == 0 && b == b'/' && on(&lower[i + 1..]))` (folders only at the root).
