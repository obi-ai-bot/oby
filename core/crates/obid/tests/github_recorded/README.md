# GitHub, recorded

A tape recorder held up to obid while it talks to GitHub. Each test here drives one GitHub path
through obid's API on a scratch obid, with a stand-in `gh` that answers from fixture files and
writes down every call it gets. The test then compares two things with what is kept in this
folder:

- **`<path>/calls.json`**: every `gh` call obid made for that path, in order: its arguments, the
  names of its environment variables (never their values), its stdin, its working folder, and the
  text of any file it handed `gh` (`--input`, `--body-file`, `body=@…`).
- **`<path>/answers.json`**: every answer of obid's that carries GitHub's data: a job's
  `pullRequest`, `issue`, `issueUrl`, `published`, `/v1/github*` answers, inbox `github` items.

Any difference fails the test and prints the lines that changed. These recordings are the baseline
for moving GitHub behind another interface (wave 3, phase A): a move must leave them exactly as
they are.

## The pieces

| File | What it is |
|---|---|
| `../fakes/gh` | The recording `gh`. Its routes and how they match are in its header. |
| `recorder.rs` | Copies a path's routes into the test's folder, reads the calls back, makes them comparable, compares, and prints the diff. |
| `<path>.rs` | One path's test, `github_recorded_<path>`. |
| `<path>/routes.json` | What the stand-in `gh` answers, first match wins, and the fixture files it names. |
| `<path>/calls.json`, `<path>/answers.json` | The recordings. |

## What is made comparable, and nothing else

Each rule is commented where `recorder.rs` applies it. Arguments, URLs, JSON keys and every value
GitHub gave stay exactly as they were.

- The test's temporary folder is `<FOLDER>`, the crate's folder (obid's working folder) `<CRATE>`.
- obid's process id in a temporary folder's name is `<PID>`.
- UUIDs are `<UUID>`.
- Commit hashes are `<commit-n>`, numbered by first appearance, so a call and an answer that name
  the same commit still have to agree. Their 12- and 7-character short forms the same way.
- The values of keys obid stamps from its own clock (`checkedAt`, `expiresAt`, `createdAt`, …:
  `CLOCK_KEYS`) are `<TIME>`.
- `__CF_USER_TEXT_ENCODING`, which macOS adds to every process, is left out of the environment's
  names.
- A block of **reads** that repeats right after itself is kept once: a test that waits by asking
  obid again, or obid's own timers, read GitHub a number of times that depends on the machine. A
  block holding a **write** (a request with a method, a pull request's create, edit, merge or
  review, a rerun, a sign-in) is never folded.

## Re-recording

Only for a change that is **meant** to change what obid asks GitHub or shows of it:

```bash
cd core && GITHUB_RECORDED=bless cargo test -p obid --test github_recorded
```

Then read the diff of `calls.json` and `answers.json` in `git diff`: every line that moved must be
one the change meant to move. The pull request says it re-recorded, names the paths, and explains
each change to a recording. A move that is meant to change nothing (phase A's) never re-records:
a red test there is the move changing GitHub's behavior.

A call no route answers is still recorded, and fails as GitHub's 404 does; a change that makes a
new call adds a route for it in `<path>/routes.json` before re-recording.
