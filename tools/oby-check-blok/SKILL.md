---
name: oby-check-affected
description: Check affected Oby suites during an agent turn.
---

Call the MCP tool `oby_check_affected` instead of running the whole
`scripts/oby-check.sh` inside your job's box. Its optional `base` defaults to
`origin/dev`. The equivalent CLI is `oby-check-blok --affected [base]`, run from
the repository root. Build both binaries first with
`cargo build --offline --manifest-path tools/oby-check-blok/Cargo.toml --bins`.
Start the stdio MCP server with `oby-check-blok mcp` from that same root.

Read the JSON verdict: `passed`, `failed` with `failing` test names, or
`unjudged` with a `reason`. CLI exit codes are 0, 1 and 2 respectively;
the one-line human summary goes to stderr. Fix failures in the affected work.
An unjudged result is not a pass: report its reason. The script owns affected
selection and skips suites that cannot run in a nested sandbox. Dependencies
must already be cached: these checks run offline.

Obid runs the full check itself after the turn. This tool only checks; it
does not publish, merge, approve or access secrets on an agent's behalf.
