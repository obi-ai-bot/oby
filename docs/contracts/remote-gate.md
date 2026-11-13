# The factory's remote gate: Linux checks on the gate VM

The business behavior contract for running the factory's gate on the Azure VM `oby-gate`, from [the template](../business-contract-template.md). Scenarios are numbered `REMOTE-n`. The design and the VM as it is: [factory-remote-gate.md](../factory-remote-gate.md). Task A00-03 built the VM's side; A00-10 built the gate's.

## Feature and status

- **Outcome:** a factory gate's Linux check groups (rust, electron, scan) run on a 32-core VM, up to four at once, instead of one at a time on the owner's Mac, which keeps only the macOS-only suites, the factory's own tests, and the whole check for a change to the check's own files.
- **Status:** all eleven scenarios are **implemented**. REMOTE-2, 7, 8 and 9 by `factory/remote/oby-remote-gate`, tested by `factory/tests/test_remote_gate.py` (A00-03, A00-10); REMOTE-11 by `factory/remote/setup.sh` and `factory/remote/tighten.sh`, tested by `factory/tests/test_remote_gate.py` (A00-12); REMOTE-1, 3, 4, 5 and 6 by `factory/bin/gate` and `factory/bin/job queue`, tested by `factory/tests/test_gate_remote.py` (A00-10; REMOTE-4's factory group, A00-13); REMOTE-10 by `factory/bin/gate`, tested by `factory/tests/test_gate_remote.py` (A00-13). The remote path is **off** until the owner adds a `remote_gate` key to `factory/config.json` (protected, so the owner's): until then every gate runs on the Mac as before.
- **Scope:** the VM's setup (`factory/remote/setup.sh`, and `factory/remote/tighten.sh`, which it runs last on the host), the script that runs a commit's Linux groups there (`factory/remote/oby-remote-gate`), and the gate's side: where each part runs, starting the VM, the fallback to the Mac, and deallocating the idle VM from the Mac. **Out of scope:** a check user without `sudo` on the VM, records in `~/.oby-flow/pipelines/runs.jsonl`, the VM's state and slots in the dashboard's header, and creating or changing any Azure resource (the owner's).

## Behavior

### Actors and ownership

- **The gate** (`factory/bin/gate`): reads the check's plan, starts the VM when it is stopped, runs the Linux groups there over SSH and the Mac's part on the Mac, falls back to the Mac, and writes `checks.md`. Every remote use stamps `factory/jobs/remote-gate/state.json`.
- **The queue** (`factory/bin/job queue`, every few minutes): runs `factory/bin/gate --idle`, which deallocates the idle VM. The VM has no managed identity, so it can't deallocate itself: the Mac does.
- **The remote gate** (`oby-remote-gate`, on the VM): takes a slot, fetches the commit from origin, runs the commit's own `scripts/oby-check.sh` for its Linux groups, offline, and exits 0, 1 or 2.
- **The owner:** owns the VM, its budget and its network rule, and the `remote_gate` key that turns the remote path on.

### Triggers and prerequisites

- `setup.sh` has run against the host: the pinned tools, `/srv/oby/repo.git` and four slots.
- `remote_gate` in `factory/config.json` names the VM, or `FACTORY_REMOTE_GATE` does for one run.
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
- On the Mac, the gate runs the whole check there, as without a VM, and says why in `checks.md`, when the settings are invalid, the plan can't be read, the commit isn't on origin, the VM can't be started or reached within 180 seconds, or the remote gate couldn't judge (2) or was cut off (ssh's 255, a signal). A remote failure (1) is a verdict and isn't run again on the Mac.

### Invariants and boundaries

- No two gates ever share a slot's worktree or build folder.
- The VM holds no secret, token, key or login; it fetches from the public repository with no credential.
- `oby-check-blok` on the VM is built from `origin/dev`, never from a commit being checked.
- Nothing on the check's path that setup installs or owns is writable by group or others.
- The only Azure actions the factory takes are starting and deallocating `oby-gate` and reading its state.
- Nothing goes to the VM but the commit and the base, validated on the Mac and quoted: no environment, token, login or file.
- The VM's groups build and test offline (`CARGO_NET_OFFLINE`, `npm_config_offline`), as on the Mac; only `cargo fetch` and `npm ci` before them have the network.

## Interfaces and dependencies

- `oby-remote-gate <commit> <base>`: a full 40-character SHA, and `origin/<branch>` or a full SHA. Output streamed; exit 0, 1 or 2 (128 + a signal when stopped).
- `factory/remote/setup.sh [ssh-host]` and `setup.sh --pins [ci.yml]`: the pins are `.github/workflows/ci.yml`'s (`RUSTUP_TOOLCHAIN`, `GITLEAKS_VERSION` and `GITLEAKS_SHA256`, `node-version`).
- `remote_gate` in `factory/config.json`: `{"ssh_host": "oby-gate", "resource_group": "oby-factory", "vm": "oby-gate"}`, each value matching `[A-Za-z0-9][A-Za-z0-9._-]*`. Missing or `null`: the remote path is off.
- `FACTORY_REMOTE_GATE`: the same JSON, in the environment, in the key's place for one run (the tests, a trial gate).
- `FACTORY_REMOTE_REACH_SECONDS`: shortens the 180 seconds the gate gives the VM to start and answer SSH. A test hook only, as `OBY_REMOTE_ROOT` is.
- `factory/bin/gate --idle`: deallocates the VM (`az vm deallocate --no-wait`) when its last use is 30 or more minutes old and no job is in `gate` or `merging`; `factory/bin/job queue` runs it and passes what it says to stderr.
- `factory/jobs/remote-gate/state.json`: `used`, the UTC time of the last remote use (written when a remote attempt starts and when it ends), and `settled`, the `used` the idle rule last acted on.
- `checks.md` with the remote path on: `HOST: oby-gate`, `oby-gate, mac` or `mac` after `CHECKS`, and a `## Where it ran` section (host, groups, slot, seconds, exit code, and why it fell back).
- Depends on `scripts/oby-check.sh`'s `OBY_CHECK_PLAN` and `OBY_CHECK_ONLY`, on SSH host `oby-gate`, and on `az` signed in on the Mac.

## Acceptance scenarios

- **REMOTE-1** (implemented). Given `remote_gate` is set and the VM is deallocated, when a Rust gate runs, then the gate starts the VM, runs rust and scan there, and `checks.md` says `HOST: oby-gate` and the slot; a failure there fails the gate with no run on the Mac. Tests: `test_remote_1_a_deallocated_vm_is_started_and_runs_the_linux_groups`, `test_remote_1_a_vm_failure_fails_the_gate_without_a_mac_retry`.
- **REMOTE-2** (implemented). Given four gates are running on the VM, when a fifth starts, then it waits for a free slot, and no two gates ever share a slot's build folder. Tests: `test_remote_2_two_gates_use_different_slots`, `test_remote_2_fifth_waits_for_a_free_slot`.
- **REMOTE-3** (implemented). Given the VM can't be reached within 3 minutes, the remote gate couldn't judge or was cut off, or the job's commit isn't on origin, when a gate runs, then the whole check runs on the Mac and `checks.md` says `HOST: mac` and why; output the VM gave before stays in the log, marked. Tests: `test_remote_3_an_unreachable_vm_falls_back_to_the_mac`, `test_remote_3_couldnt_judge_falls_back_to_the_mac`, `test_remote_3_a_commit_not_on_origin_runs_on_the_mac`.
- **REMOTE-4** (implemented). Given a job whose diff touches macOS-only code, when its gate runs, then once the VM's part passes the rust group also runs on the Mac, under its check lock, and both must pass; swift and factory run on the Mac whenever they are planned, in that order after rust, under the same rules: only once the VM's part passed (else listed as not run), each with its line in `## Where it ran`, and a failure fails the gate. When the whole check runs on the Mac, factory runs as part of it and not again. A touched file counts as macOS-only when its copy on the branch or the base names `target_os = "macos"` (read at gate time, so a deleted file's counts too), or when it is in `MACOS_ONLY` in `factory/bin/gate`: the files whose macOS-only tests live in other files (the box, the own lane, Keychain logins, the Codex and Cursor launches, withheld secrets, the integration tests' `support/mod.rs`, obid's `Cargo.toml` and `core/Cargo.lock`). When git can't tell, every touched file counts; when in doubt, a file joins the list. Tests: `test_remote_4_a_macos_only_path_also_runs_rust_on_the_mac`, `test_remote_4_macos_code_in_an_unlisted_file_also_runs_rust_on_the_mac`, `test_remote_4_a_deleted_file_with_macos_code_also_runs_rust_on_the_mac`, `test_remote_4_a_listed_file_whose_macos_tests_live_elsewhere_runs_rust_on_the_mac`, `test_remote_4_swift_runs_on_the_mac`, `test_remote_4_factory_runs_on_the_mac`, `test_remote_4_a_factory_failure_on_the_mac_fails_the_gate`, `test_remote_4_factory_isnt_run_when_the_vms_part_failed`.
- **REMOTE-5** (implemented). Given the gate's last remote use was 30 or more minutes ago and no job is in `gate` or `merging`, when the Mac's `job queue` runs, then it deallocates the running VM once, which `az vm get-instance-view` shows as `PowerState/deallocated`; a VM already deallocated, or one with no remote use recorded, is left alone. Tests: `test_remote_5_deallocates_only_after_30_idle_minutes`, `test_remote_5_job_queue_runs_the_idle_rule_and_keeps_its_rows`.
- **REMOTE-6** (implemented). Given `remote_gate` is missing or `null` and `FACTORY_REMOTE_GATE` is unset, then the gate behaves exactly as before: one `checks.sh` run, the same `checks.md` header, and no `az` or `ssh`. Test: `test_remote_6_unset_remote_gate_is_unchanged`.
- **REMOTE-7** (implemented). Given a remote gate running a check, when its caller's connection drops, then the check's processes stop and its slot is free again. Test: `test_remote_7_dropped_connection_frees_the_slot`.
- **REMOTE-8** (implemented). Given a commit that origin doesn't have, when a remote gate is asked to check it, then it runs nothing and exits 2, naming the commit. Test: `test_remote_8_commit_not_on_origin_runs_nothing`.
- **REMOTE-9** (implemented). Given a remote gate running a commit's groups, then each group runs with `CARGO_NET_OFFLINE=true` and `npm_config_offline=true`, after `cargo fetch` and `npm ci`, which have the network. Test: `test_remote_9_groups_run_offline`.
- **REMOTE-10** (implemented). Given `remote_gate` is set and a job's diff against the base touches the check's own files (`scripts/oby-check.sh`, `scripts/oby-check-test.sh` or anything under `tools/oby-check-blok/`, `CHECK_FILES` in `factory/bin/gate`; a deleted or renamed file's old path too), when its gate runs, then the whole check runs on the Mac as without a VM, before the plan is read: no `az`, no `ssh` and no `used` stamp. `checks.md` says `HOST: mac` and `the whole check (the diff touches the check's own files: …)`, naming at most five files, with no fallback line: `oby-check-test.sh` expects macOS's plan, so the VM would fail the change for the machine. The Mac's exit code is the verdict. Tests: `test_remote_10_a_change_to_the_checks_test_runs_the_whole_check_on_the_mac`, `test_remote_10_a_change_under_oby_check_blok_runs_the_whole_check_on_the_mac`.
- **REMOTE-11** (implemented). Given a host whose `~/.cargo`, `~/.cargo/bin`, the check tool in it, `~/.rustup`'s toolchains or `/srv/oby`'s slots are writable by group or others, when `setup.sh` runs, then it leaves none of them writable by others, vets `oby-check-blok` with origin/dev's own `scripts/oby-check.sh` rule, ends by printing their modes, and stops naming each path it couldn't tighten. Tests: `test_remote_11_setup_tightens_the_checks_path`, `test_remote_11_a_path_that_cant_be_tightened_stops_setup`, `test_remote_11_setup_applies_the_checks_own_rule`.

The tests run on the Mac with no VM: `python3 factory/tests/test_remote_gate.py` and `python3 factory/tests/test_gate_remote.py`, from the repository root. The run on the real VM, with its times, is in task A00-03's pull request and in [factory-remote-gate.md](../factory-remote-gate.md).
