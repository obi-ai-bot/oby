---
name: verify-lane-secrets
description: Re-run the Kani proofs that no lifecycle change clears a job's `ran_unproxied` (LANE-3a's mark, read by LANES-6), and read what they cover. Use when a change touches `ran_unproxied`, `job/lifecycle.rs`, `Jobs::ran_unproxied`, `checking_with_turn`'s `raw_network` or `code_refuses_secrets`.
---

# Verifying that a job's unproxied mark stays (LANES-6, LANE-3a)

## 1. The property and the harm

Once a job's `ran_unproxied` is true, it stays true after every status and every command `transition` answers (`NextRound`, `RunEnded` of each `EndKind`), as `Change::apply` writes it and `Job::upgrade` loads it. `as_opened` keeps it when either job it reads had it.

The harm when it breaks: a job whose agent once ran without obid's proxy gets the owner's declared secrets at a later check, after a Try again, an end or an answer. The secrets then reach a check whose agent could reach anything.

## 2. What is proved

Both harnesses are in `job::lifecycle::proofs` (core/crates/obid/src/job/lifecycle.rs). Each job they build comes from `any_bare_job()`: `Job::default()` with `kani::any()` for `status` (through `any_status`), `round`, `rounds`, `recovery.tries` and `ran_unproxied`, a session (`Some("s")`) or none, and another Obi's `ObiRef` or none. Every other field stays at its default; it doesn't call `fill`.

- `lane_3a_no_lifecycle_change_clears_ran_unproxied`: for any such job and any command from `any_command`, a job whose mark was true still has it after `transition`, `Change::apply` and `Job::upgrade`.
- `lane_3a_words_answered_as_opened_keep_ran_unproxied`: for any two such jobs `opened` and `now`, `as_opened(&opened, now)` has the mark whenever `now` had it, whether or not the round is taken back.

## 3. The `opened` case

`as_opened` never reads `ran_unproxied`: its answer carries `now`'s mark. A mark `opened` had is also in `now`, because `now` is the same job read later and no writer clears it: the first harness (section 2), the writer grep (section 9) and the read writers (section 4) stand behind that. With it, the second harness gives "true when either had it".

## 4. What is read, not proved

These are async or I/O-bound `Jobs` methods Kani can't build.

The refusal the mark feeds: `Jobs::ran_unproxied` (jobs/lanes.rs) → `raw_network` in `Jobs::checking_with_turn` (jobs/check_run.rs, `spec.network && (!sandbox.confines_egress() || self.ran_unproxied(id))`) → `Jobs::code_refuses_secrets` (jobs/secrets.rs).

The writers outside the lifecycle, none of which sets it false:

- `Jobs::launched` (jobs/lanes.rs): `(j.own_lane, j.ran_unproxied) = (own, j.ran_unproxied || own)`.
- `Jobs::kept_unproxied` (jobs/lanes.rs): `= true`.
- `Jobs::create` (jobs.rs): `new.ran_unproxied || of…ran_unproxied`, a follow-up taking its parent's.
- `brain::written_in` (brain/mod.rs): a `let` that ORs in `laid_unproxied`.
- `carry_marks` (jobs/keep.rs): `|=`.
- `publish_artifacts` (jobs/publishing.rs): `|=` on its copy.
- The artifact layout in jobs/files_at_work.rs: `= true`.
- Struct-literal fields (routes.rs, jobs.rs, brain/actions.rs) set a `NewJob`'s starting value, which `Jobs::create` ORs in.

## 5. The tests that cover the read paths

- `lanes_6_an_own_jobs_check_has_no_network` and the `lane_3a_*` tests in jobs/lanes.rs (`lane_3a_a_mark_that_cant_be_kept_is_an_error`, `lane_3a_a_new_follow_up_takes_the_relays_unproxied_flag` and the relay ones).
- `lane_3a_a_flag_that_cant_be_read_counts_as_flagged` (brain/open_artifact.rs).
- `lane_3a_fix_ci_in_a_flagged_conversation_withholds_the_secrets` (tests/github_work.rs).
- `lane_3a_a_follow_up_the_brain_starts_in_a_flagged_conversation_withholds_the_secrets` and `lane_3a_a_message_relayed_from_a_flagged_conversation_marks_the_running_job` (tests/conversation_harness.rs).
- `artifacts_24_an_unproxied_jobs_version_withholds_secrets_from_the_job_it_reaches` (jobs/files_at_work.rs).

## 6. The `spec.network` assumption

Every agent spec sets `network: true` today (jobs/turns.rs, jobs/lanes.rs, jobs/logins.rs), so `raw_network` follows `ran_unproxied` whenever the sandbox confines egress. A spec with `network: false` would let a marked job's check get the secrets on that term alone; `check_spec`'s own `spec.network = false` is the check's box, set after `raw_network` is read.

## 7. Open findings

None.

## 8. Sync

SYNC: 09197a491f54cc132e0146e708f07bb7a3e74924

## 9. The one command, and the writer grep

```
cd core && cargo kani -p obid --tests --harness job::lifecycle::proofs::lane_3a_
```

```
grep -rnE 'ran_unproxied *\|?=[^=]|ran_unproxied\) *=[^=]' core/crates/obid/src
```

Every hit must be `= true`, `|=`, or an `||` that includes the old value. At the SYNC commit it prints exactly nine hits: jobs.rs (2), brain/mod.rs, jobs/lanes.rs (2), jobs/keep.rs, jobs/files_at_work.rs (2) and jobs/publishing.rs. A struct-literal `ran_unproxied:` is a `NewJob`'s starting value, which `Jobs::create` ORs in (and `any_bare_job`'s, in the proofs). A new hit that can set it false is a bug, or a writer this skill must prove or read.

## 10. A bug or a harness error

`VERIFICATION:- FAILED` with the failed assertion on `ran_unproxied`, its trace through `Change::apply`, `Job::upgrade` or `as_opened`, is a bug in the lifecycle. A failed unwinding assertion, an out-of-memory or a timeout is the harness's problem: narrow what the job is built from or split by command. Never add a `kani::assume` or `kani::unwind`, and never weaken the assertion.

## 11. The harness still bites

On a committed tree, apply one break to core/crates/obid/src/job/lifecycle.rs, run the command in section 9, see the harness `FAILED`, then `git restore core/crates/obid/src/job/lifecycle.rs` and see it `SUCCESSFUL`. Never commit a break.

1. `job.rounds = *rounds;` → `job.rounds = *rounds; job.ran_unproxied = false;` (`lane_3a_no_lifecycle_change_clears_ran_unproxied` fails).
2. `now.revision = opened.revision;` → `now.revision = opened.revision; now.ran_unproxied = opened.ran_unproxied;` (`lane_3a_words_answered_as_opened_keep_ran_unproxied` fails).
