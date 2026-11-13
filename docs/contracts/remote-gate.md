# The factory's remote gate: Linux checks on the gate VM

The business behavior contract for running the factory's gate on the Azure VM `oby-gate`, from [the template](../business-contract-template.md). Scenarios are numbered `REMOTE-n`. The design and the VM as it is: [factory-remote-gate.md](../factory-remote-gate.md). Task A00-03 built the VM's side; the gate's side is a second task.

## Feature and status

- **Outcome:** a factory gate's Linux check groups (rust, electron, scan) run on a 32-core VM, up to four at once, instead of one at a time on the owner's Mac, which keeps only the macOS-only suites.
- **Status:** REMOTE-2, REMOTE-7 and REMOTE-8 are **implemented** by `factory/remote/oby-remote-gate` and tested by `factory/tests/test_remote_gate.py` (A00-03). REMOTE-1, 3, 4, 5 and 6 are **proposed**: they need `factory/bin/gate` to use the VM, which it doesn't yet. Gates still run on the Mac.
- **Scope:** the VM's setup (`factory/remote/setup.sh`) and the script that runs a commit's Linux groups there (`factory/remote/oby-remote-gate`). **Out of scope for now:** `factory/bin/gate`'s remote path, the Mac-side path list, idle deallocation, and creating or changing any Azure resource (the owner's).

## Behavior

### Actors and ownership

- **The gate** (`factory/bin/gate`, proposed): decides where a gate runs, starts the VM, calls the remote gate over SSH, and writes `checks.md`.
- **The remote gate** (`oby-remote-gate`, on the VM): takes a slot, fetches the commit from origin, runs the commit's own `scripts/oby-check.sh` for its Linux groups, and exits 0, 1 or 2.
- **The owner:** owns the VM, its budget and its network rule, and starts it by hand until the gate does.

### Triggers and prerequisites

- `setup.sh` has run against the host: the pinned tools, `/srv/oby/repo.git` and four slots.
- The commit to check is on origin (`obi-ai-bot/oby` is public, so no token is needed).
- The caller keeps the remote gate's standard input open, as `ssh` without `-n` does.

### States and transitions

- **Waiting for a slot:** all four held; it says so once and tries again every second.
- **In slot N:** fetch and checkout under one repository lock, then the plan, then each Linux group in order (rust, electron, scan), every one to its end. swift is never run there.
- **Ended:** 1 when a group failed, else 2 when one couldn't judge, else 0, with a last line naming the verdict, the slot and the seconds.

### Failures and recovery

- A commit origin doesn't have: nothing runs, exit 2.
- A dropped connection (standard input's end), a failed write or SIGHUP, SIGTERM or SIGINT: the check's whole session is stopped and the slot is free.
- A plan that can't run, a missing clone, or a group whose setup (`cargo fetch`, `npm ci`) fails: exit 2, couldn't judge.

### Invariants and boundaries

- No two gates ever share a slot's worktree or build folder.
- The VM holds no secret, token, key or login; it fetches from the public repository with no credential.
- `oby-check-blok` on the VM is built from `origin/dev`, never from a commit being checked.
- The only Azure actions the factory takes are starting and deallocating `oby-gate` and reading its state.

## Interfaces and dependencies

- `oby-remote-gate <commit> <base>`: a full 40-character SHA, and `origin/<branch>` or a full SHA. Output streamed; exit 0, 1 or 2 (128 + a signal when stopped).
- `factory/remote/setup.sh [ssh-host]` and `setup.sh --pins [ci.yml]`: the pins are `.github/workflows/ci.yml`'s (`RUSTUP_TOOLCHAIN`, `GITLEAKS_VERSION` and `GITLEAKS_SHA256`, `node-version`).
- Depends on `scripts/oby-check.sh`'s `OBY_CHECK_PLAN` and `OBY_CHECK_ONLY`, and on SSH host `oby-gate`.

## Acceptance scenarios

- **REMOTE-1** (proposed). Given `remote_gate` is set and the VM is deallocated, when a Rust gate runs, then the gate starts the VM, runs rust and scan there, and `checks.md` says `HOST: oby-gate`.
- **REMOTE-2** (implemented). Given four gates are running on the VM, when a fifth starts, then it waits for a free slot, and no two gates ever share a slot's build folder. Tests: `test_remote_2_two_gates_use_different_slots`, `test_remote_2_fifth_waits_for_a_free_slot`.
- **REMOTE-3** (proposed). Given the VM can't be reached, when a gate runs, then it runs on the Mac within 3 minutes and `checks.md` says why.
- **REMOTE-4** (proposed). Given a job that touches `core/crates/obid/src/sandbox.rs`, when its gate runs, then the macOS-only rust tests also run on the Mac and both verdicts must pass.
- **REMOTE-5** (proposed). Given no gate for 30 minutes, then the VM is deallocated, which `az vm get-instance-view` shows as `PowerState/deallocated`.
- **REMOTE-6** (proposed). Given `remote_gate` is `null`, then the gate behaves exactly as before. The existing factory tests and a docs-only gate show this.
- **REMOTE-7** (implemented). Given a remote gate running a check, when its caller's connection drops, then the check's processes stop and its slot is free again. Test: `test_remote_7_dropped_connection_frees_the_slot`.
- **REMOTE-8** (implemented). Given a commit that origin doesn't have, when a remote gate is asked to check it, then it runs nothing and exits 2, naming the commit. Test: `test_remote_8_commit_not_on_origin_runs_nothing`.

The tests run on the Mac with no VM: `python3 factory/tests/test_remote_gate.py`, from the repository root. The run on the real VM, with its times, is in task A00-03's pull request and in [factory-remote-gate.md](../factory-remote-gate.md).
