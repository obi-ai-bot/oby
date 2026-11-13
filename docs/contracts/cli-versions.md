# CLI versions: which Claude Code, Codex or cursor-agent a job ran on

The business behavior contract for reading each agent CLI's version, from [the template](../business-contract-template.md). Issue #1096, roadmap task R01-07, milestone R01 Safety net.

## Feature and status

- **Outcome:** The owner, and any bug report, can say which Claude Code, Codex or cursor-agent version a job ran on, and `GET /v1/status` lists each tool's binary and version. Failures such as #1867 and #1095 can then be tied to a CLI version.
- **Status:** **Implemented** (`core/crates/obid/src/cli_versions.rs`, `core/crates/obid/src/jobs/versions.rs`; `docs/runner-api.md`, `GET /v1/status`, "Job" and "Failure record"). CLIVERSIONS-1 to 4 are implemented.
- **Scope:** Reading and caching each tool's version, `tools` on `GET /v1/status`, `agentVersion` on the job and on the failure record, and one log line per change. Outside it: the tested and known-bad version table (R01-09: nothing compares versions or warns about one), Settings › Accounts or any other screen showing a version (#1097), conversation turn records and the brain's runs (R05), Obi-to-Obi review jobs, the `clientInfo` that Codex and Cursor are sent, re-reading on a timer, events for a version change, a new endpoint, a setting for the read, and moving this into the provider interface (R08).

## Behavior

### Actors and ownership

- **obid** owns the reads: it finds each tool's binary, runs its `--version`, keeps what it read, stamps it on jobs and failure records, and logs a change. No model call decides anything here.
- **The apps** show nothing yet and send nothing new: the keys are add-only, and older apps ignore them.

### Rules

- **The binary.** A tool's binary is the configuration's `bin` for it, else its default name (`claude`, `codex`, `cursor-agent`), found as a run finds it (`process::resolve_bin`). Not found, its path and version are none and nothing runs.
- **The read.** obid runs `<binary> --version` with a 5 second limit, its environment cleared but for the locale, terminal, proxy and CA variables, the login `PATH` (`accounts::login_path`), and its `HOME`, `TMPDIR` and working folder one empty folder obid made for it (0700, new for each obid). Its stdin is empty and its stderr dropped. No account variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `CURSOR_CONFIG_DIR`, `XDG_CONFIG_HOME`), login, token or account folder reaches it.
- **The version.** The first word it printed on stdout that, after an optional leading `v`, starts with a digit, holds a `.` and is made only of letters, digits and `.+-`, at most 64 characters (`2.1.317 (Claude Code)` is `2.1.317`, `codex-cli 0.163.0` is `0.163.0`). A non-zero exit, a run past 5 seconds, one that can't start, or no such word is none.
- **Kept.** obid keeps each tool's last read with the binary's resolved path, size and modified time, and runs `--version` again only when one of those differs. A failed or hung read is kept like a good one.
- **When it looks.** Status looks at the binary again only once the tool's last look is 60 seconds old; before that it answers what it kept. A round start looks at it every time (a stat, no process), so a changed binary is read at the next round.
- **At start.** obid reads all three tools in the background as it starts, so the first status usually finds them kept. Starting never waits for them.
- **Status.** `GET /v1/status` carries `tools`, one entry per tool in the order `claude`, `codex`, `cursor`: `{agent, path, version}`, `path` the binary's absolute path or null, `version` a string or null. The three are read side by side, so one status waits on a read for at most about 5 seconds.
- **The job.** As each round of a job starts, obid reads its tool's version in the background and sets the job's `agentVersion` to it, replacing the earlier round's. The round never waits for it. A move to another tool starts a new run and so a new stamp; a turn inside a round keeps its round's.
- **The failure record.** A failure record copies its job's `agentVersion` when it is written.
- **The log.** When a read's version differs from the one kept for that tool (the first read since obid started included), obid logs one line: `<Tool> at <path> is version <version>`, or `<Tool> at <path> has no version obid can read`. It is decided as the read is kept, so reads that overlap log a change once. A read whose version didn't change logs nothing.

### Failures and recovery

- **A version that can't be read** is null. It never blocks, delays, fails or reroutes a job, and status always answers.
- **A read that hangs** is ended at 5 seconds and kept as none until the binary changes.
- **A failed background read** for a round leaves `agentVersion` none, and the job goes on unchanged.
- **A job that fails within moments of its round's start** may have its failure record written before the stamp lands; that record's `agentVersion` is then null.
- **A restart** forgets what was read; obid reads again as it starts.

### Invariants

- No login, token, account folder or account variable ever reaches a `--version` run.
- The process runs only for a binary whose path, size or modified time changed, and never while obid holds the store's lock.
- Every key is add-only and optional when read: a job file or failure record written before this change loads, with `agentVersion` null.

## Interfaces and dependencies

- **API:** `GET /v1/status` gains `tools`; the Job gains `agentVersion`; the failure record gains `agentVersion` (`docs/runner-api.md`). Older runners don't send them.
- **Persistence:** `agentVersion` in `home/jobs/<id>.json` and `home/failures/<id>.json`, left out when none.
- **Clients:** none change (#1097 shows it in Settings later).

## Acceptance scenarios

- **CLIVERSIONS-1** (implemented). Given `claude` is on the login PATH, when an app reads `GET /v1/status`, then `tools` lists claude with its resolved path and the version its `--version` printed. Test: `cliversions_1_status_lists_each_tool_version` (core/crates/obid/tests/cli_versions.rs).
- **CLIVERSIONS-2** (implemented). Given a tool whose `--version` fails or hangs past 5 s, when status is read, then its version is null and status still answers. Test: `cliversions_2_a_failing_version_is_null` (core/crates/obid/tests/cli_versions.rs).
- **CLIVERSIONS-3** (implemented). Given a job ran a round on codex 0.170.0, when the job file is read, then `agentVersion` is `"0.170.0"`. Test: `cliversions_3_a_round_stamps_agent_version` (core/crates/obid/tests/cli_versions.rs).
- **CLIVERSIONS-4** (implemented). Given the binary at the resolved path changes (new modified time), when the next round starts, then obid reads the version again and logs the change once. Test: `cliversions_4_a_changed_binary_is_read_again` (core/crates/obid/tests/cli_versions.rs).

The parser's rules are tested in `core/crates/obid/src/cli_versions.rs`, and the failure record's key in `core/crates/obid/src/failures.rs`.
