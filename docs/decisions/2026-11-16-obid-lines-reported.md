# The Rust core's lines are reported, never limited

- **Date:** 2026-11-16
- **Status:** In force
- **Replaced:** The rule in AGENTS.md's words, which gave it no date: "The Rust core's production lines outside the Sphere stay under `obid_lines_limit` in `scripts/oby-check.sh`, or the check fails. A cut lowers the limit, and the owner's word raises it."
- **Changed by:** Nothing
- **Contracts and scenarios:** [ci.md](../contracts/ci.md) (CI-35, CI-39 to 41, CI-49, CI-50)

## The owner's words

> "this ceiling makes no sense at all"

The owner's words, as task B00-25 relays them.

> "Land first"

The owner's words, answering whether B00-25 lands before B00-26, as task B00-25 relays them.

## Where AGENTS.md states it

- Dispatcher, not harness

## Notes

The limit began at 90,500 on 2026-11-05, was raised about 15 times, each raise a pull request of its own with a payback line, and stood at 102,450 when it went. It stopped features rather than shaping them: A00-70 waited on 83 lines. A count of lines can't tell whether obid does the agents' work instead of the dispatcher's, which is what "Dispatcher, not harness" protects.

The check now prints obid's production lines and the change's difference from its base on every run of its limits block, and never fails on either (CI-49, CI-50). `jobs_rs_limit`, `tests_sleep_limit` and `agents_md_bytes_limit` are unchanged, and the raise rules (CI-39 to 41) hold for `jobs_rs_limit` alone.

B00-26 replaces the limit with checks of a task's lane. Until it lands, nothing automatic watches obid's size, as the owner chose.
