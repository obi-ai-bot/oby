# Preflight before every push

- **Date:** 2026-11-20
- **Status:** In force
- **Replaced:** Nothing
- **Changed by:** Nothing
- **Contracts and scenarios:** [ci.md](../contracts/ci.md) (CI-69)

## The owner's words

> "All three CI fixes"

The owner's words in the orchestrator's conversation, approving three fixes to CI: this one, docs that can lead code without turning `dev` red, and a `full-ci` pull request that runs every Rust test.

## Where AGENTS.md states it

- How we work

## Notes

On 2026-11-20, 45 of 154 runs of `ci.yml` failed, and about 35 of those on checks that take seconds and ran only on GitHub: the Brain's "every documented route is served" (`retired_8`) and "puts every top-level module in an area", obid-spec's coverage gate (a scenario `tools/obid-spec/scope.json` doesn't classify, a route with no workflow), and `cargo fmt` and clippy. Each cost a pull request a run, and in the merge queue a group.

`scripts/preflight.sh` runs those before a push, in about a minute once its builds are warm, and says for each miss what to change. It replaces none of the Mac's check (`scripts/oby-check.sh`) or GitHub's: it moves the cheapest failures to before the push.
