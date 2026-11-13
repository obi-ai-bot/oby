# Retired: what has no caller leaves

The business behavior contract for roadmap milestone R03, "Delete what has no caller", from [the template](../business-contract-template.md). Each R03 task adds its own section here, with its own RETIRED scenarios.

## R03-01: the telemetry flag

### Feature and status

- **Outcome:** obid has no setting that suggests telemetry exists, and Oby still sends nothing. A `config.json` an older obid or install.sh wrote with `"telemetry"` in it keeps working: the key is ignored and named, as `council` is.
- **Why:** obid parsed and wrote a `telemetry` key, and nothing read it. `git grep -n '\.telemetry'` over `core/`, `apps/`, `mcp/`, `install/` and `cloud/control-plane/` found only its own copy in `Config::parse` and tests' asserts.
- **Status:** **implemented** (RETIRED-1 and RETIRED-2).
- **Scope:** The `telemetry` field of obid's configuration (`core/crates/obid/src/config.rs`), the first configuration obid writes (`default_config`, `core/crates/obid/src/app.rs`) and the one install.sh writes. Out of scope: an opt-in sender of failure reports (later work, R09); the guard in `core/crates/obid/src/memory.rs` that refuses a memory turning telemetry on, and its sentence; AGENTS.md's "No telemetry unless the user turns it on", still true; the agents' own telemetry switches (`DISABLE_TELEMETRY`).

### Behavior

- **Actors and ownership:** obid owns its configuration keys. A top-level key it doesn't know is ignored, listed in `Config.ignored` and named in its log at start: `<path>: telemetry isn't a key this obid knows: ignored`. Nothing changes for the apps.
- **Failures and recovery:** None new: an older key never stops obid. Every install that has `"telemetry": false` logs that one warning at each start, as for any retired key.

### Invariants

- Oby sends no telemetry.
- The released files in `core/crates/obid/tests/fixtures/released/` are never edited: those that carry `"telemetry": false` are what that release wrote ([released-files.md](released-files.md)).

### Interfaces and dependencies

- **Persistence:** `config.json`, which obid and install.sh write without the key. No API changes.

### Acceptance scenarios

- **RETIRED-1** (implemented). Given a `config.json` with `"telemetry": true` that an older obid wrote, when obid starts, then it starts, `Config.ignored` names `telemetry`, and its log says `telemetry isn't a key this obid knows: ignored`. Tests: `retired_1_an_old_telemetry_key_is_ignored_and_named` (core/crates/obid/src/config.rs) and `optional_parts_never_stop_obid` (core/crates/obid/tests/status.rs). `released_3_released_config_starts` (core/crates/obid/tests/released_files.rs) covers the released files that carry the key.
- **RETIRED-2** (implemented). Given no `config.json`, when obid writes its first one (`default_config`, with and without Codex) or install.sh writes one, then it has no `telemetry` key. Test: `retired_2_a_new_configuration_has_no_telemetry_key` (core/crates/obid/tests/install.rs).
