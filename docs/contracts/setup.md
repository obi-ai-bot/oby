# Setup: a job's worktree is ready before its agent starts

The business behavior contract for what obid does to a job's worktree after it checks it out and before its agent's first turn, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` ("Git LFS and submodules", `repos[].setup`).

## Feature and status

- **Outcome:** An agent starts in a worktree it can build in, without spending its first turns on what obid can do for it, and without obid handing it, or anyone else, a login it doesn't need.
- **Status:** **Implemented**, SETUP-1 since 2026-11-08. A repository's own setup line is PUBLISHING-23, and its LFS pointers PUBLISHING-20 ([publishing.md](publishing.md)).
- **Scope:** The owner's own jobs' worktrees. **Out of scope:** Oby's review's worktree, the rebase onto a moved base and the check's clean checkout, whose submodules stay empty; Git LFS files, which stay pointers (PUBLISHING-20).

## Behavior

- **Actors and ownership:** obid fills the submodules; the agent never runs `git submodule`, and its box has no login to.
- **Triggers:** a job's worktree is made from a commit with a `.gitmodules`.
- **Decisions:** which submodules (the commit's gitlinks); which credentials (the owner's gh login on the repository's own `https` host only); whether the clone now holds someone else's commits (a submodule not on the repository's host under its owner).
- **Observable results:** the job's step while it fills them, and the ones it couldn't name on its step.
- **Failure and recovery:** a submodule that fails is named with git's reason and the job goes on without it; the others still fill.
- **Invariants:** obid runs it as its own fetches, unboxed, with its safe git settings and `protocol.file.allow=never`; git's own credential helpers never answer for a submodule, and no host but the repository's gets the owner's login.

## Interfaces and dependencies

`git::fill_submodules` and `git::submodule_credentials` (core/crates/obid/src/git.rs), called where `Jobs::run_owner_job` makes the worktree; `git::taint` for a submodule from elsewhere.

## Acceptance scenarios

- **SETUP-1:** A job whose commit has a `.gitmodules` gets its submodules from obid before its agent starts. Its step is `Getting <repo>'s submodules` while obid runs `git submodule update --init --recursive -- <path>` for each gitlink the commit has, in the worktree, unboxed as obid's fetches are, with git's safe settings, `-c protocol.file.allow=never` (`always` only with `allowFileRemotes`, the tests' fixture) and `git::LFS_POINTERS`. git's own credential helpers are reset for every host, and the owner's gh login (`publish::fetch_credentials`) answers only for the repository's own `https` host (`credential.https://<host>.helper`), so a submodule on another host, or on one whose name only starts like it, gets no credentials; with no gh, or an ssh remote, no helper answers at all (ssh uses the person's own keys, as obid's ssh fetches do). One that fails is named on the step with git's first `fatal:` line (at most 200 characters): `Getting <repo> ready: its submodule \`<path>\` wasn't filled (<why>), so what needs them won't work`, after PUBLISHING-20's LFS words with ` and ` when both hold, and the job goes on. A submodule, nested ones too, whose URL isn't on the repository's host under its owner (`github.com/<owner>/…` as https, ssh or `git@host:owner/…`) marks the clone as holding someone else's commits (`git::taint`, `a submodule from <url>`), as obid's other fetches do. A `.gitmodules` with no gitlink fills nothing and says nothing. The catch: a submodule's own `.gitmodules` URL is followed wherever it points, so a private one on another host fails rather than signs in. (`jobs_api::setup_1_a_jobs_submodules_are_filled_before_its_agent`, `git::tests::setup_1_another_hosts_submodule_gets_no_credentials`, `git::tests::setup_1_a_submodule_elsewhere_is_someone_elses`.)

Test it when it's done, on a real Mac: ask Oby to look around a repository with a public submodule on GitHub and one on another host, and see the agent find both filled; then point one at a repository that isn't there and see its step name it while the job goes on.
