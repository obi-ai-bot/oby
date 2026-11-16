<!-- Answer every section. One that doesn't apply says "none", and stays. -->

## Task and scenarios

<!-- The task key or issue this serves, and the scenario ids it covers (FOLDERS-3). With none: "none: no product behaviour changes". -->

## What and why

<!-- What changed, and why this is the smallest useful change. -->

## Surfaces

<!-- Tick each one this changes, and say how. -->

- [ ] obid (`core/`)
- [ ] the Electron app (`apps/electron`)
- [ ] the iPhone and Watch apps, and what they decode from obid's API (add-only)
- [ ] the API docs, `docs/api/<area>.md` (index `docs/runner-api.md`)
- [ ] contracts in `docs/contracts/`
- [ ] stored job and config files on disk
- [ ] the check (`scripts/oby-check.sh`, `tools/oby-check-blok`, `factory/checks.sh`)

## Dispatcher, not harness

<!-- The lane: `dispatcher`, `between-owners` or `agent` (AGENTS.md, "Dispatcher, not harness"). Then, for each state, endpoint, tool or model call this adds to obid, what Claude Code, Codex and Cursor already do there and why Oby still needs it; "none" when it adds nothing to obid. -->

## Checks run

<!-- The commands you actually ran and what they printed (AGENTS.md, "How we work"). -->

## Failure modes

<!-- One line each: what can go wrong, and the test that covers it. -->

## Security path

<!-- Yes or no, and which path if yes: `merge::SECURITY_PATHS`, MERGING-15 (such as `AGENTS.md`, `CLAUDE.md`, `.claude/`, `.github/workflows/`). -->
