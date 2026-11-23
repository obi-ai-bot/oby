---
name: verify-redaction
description: Rerun and read obid's bounded-exhaustive LOGS-1 redaction tests, and tell a bug from a test error. Use when a change touches `redact`, `try_redact`, `Snapshot::redact`, `redacted`, `taken_out`, `copies`, `joined`, `unwrapped`, `needles_of`, `needles_for`, `runs` or `SHORTEST` in core/crates/obid/src/secrets.rs.
---

# Verifying redaction

obid takes every value it holds out of text before the text goes to its log, an answer or an event. Three tests in `core/crates/obid/src/secrets.rs`'s `mod tests` try every pair of short values against each other. This skill says what they hold, what they don't reach, and how to read a failure.

## What they hold

- **Property 1.** After `Secrets::redact`, `Secrets::try_redact` or `Snapshot::redact`, no form `needles_for` makes of a learned value is left whole in the text.
- **Property 2.** In a text made only of the values' bytes and padding, no byte of a value is left outside the `[redacted: …]` markers once redacted.
- **The harm.** A vault value or a login, or part of one, lands in obid's log, an answer or an event (LOGS-1 in docs/contracts/logs.md, VAULT-4 in docs/contracts/vault.md).

## Coverage record

The code: `core/crates/obid/src/secrets.rs`, through its real functions: `Secrets::default()`, `learn`, `forget`, `redact`, `try_redact`, `snapshot().redact`, which all go through `redacted`, and so `taken_out`, `copies`, `joined`, `hits`, `decodings`, `runs` and `needles_of`; the forms come from `needles_for`, with `SHORTEST` = 8.

The join decision, `joined`, is also proved by Kani in `secrets::proofs::logs_1_an_overlapping_span_is_joined_whole`, run with `cd core && cargo kani -p obid --tests --harness secrets::proofs::`.

The families, over values learned as `x` (A) and `y` (B):
- **(a) overlap.** Every 8-byte A, k = 1 to 7, and every B that begins with A's last k bytes: `c` + A + B[k..] + `c`. 64,770 texts (A equal to B skipped).
- **(b) one holds the other.** Every 9-byte A, with B its first 8 bytes or its last 8: `c` + A + `c`. 1,024 texts.
- **(c) adjacent.** Every ordered pair of distinct 8-byte A and B: `c` + A + B + `c`. 65,280 texts.
- **(d) repeated.** Every 8-byte A, learned alone (`y` forgotten): `cAcAc`. 256 texts.

The tests:
- `logs_1_no_form_of_a_value_survives_overlap_nesting_adjacency_or_repeats`: property 1 on (a), (b), (c) and (d).
- `logs_1_no_part_of_a_value_is_left_when_one_holds_another_or_they_touch`: property 2 on (b), (c) and (d).
- `logs_1_no_part_of_overlapping_values_is_left`: property 2 on (a).

## The bound

Values are 8 bytes over the two letters `a` and `b` (9 bytes for A in (b)), overlaps run from 1 to 7 bytes, and `c` pads each text and sits between the copies in (d). Each text goes through all three functions. Property 1 reads the plain text of every family and, for (a), (b) and (d), the whole text encoded four ways too: standard base64, URL-safe base64, lower-case hex, and every byte as `%XX`.

## Not covered

- `Held` and `held_tail`, the log writer's tail across writes (LOGS-2).
- `names_in`.
- Text over lines: `unwrapped` is reached only through a line break, and none of these texts has one.
- Values with bytes outside `{a, b}`.
- More than two values at once.

## Sync

SYNC: bdf7e5d1f65aa5c37a6c5a91bc8599df615a1e2b

The commit the tests were last checked against. Move it forward only after you have read the covered code at the new commit and confirmed the tests still describe it.

## Run it

```sh
cd core && cargo test -p obid --lib secrets::tests::logs_1_
```

An open finding's test is ignored; to see its counterexample, run the same command with `-- --ignored`. Measured at 50e200d5e on an Apple-silicon Mac with the build already made: the tests take 30.1 seconds, 31.7 seconds from command to result.

## Reading a failure

A failure names the family, the function (`redact`, `try_redact` or `snapshot`), the encoding (`plain` or which one), A and B (`none` for (d)), the text and the output, and for property 1 the needle that survived.

- A marker the stripping missed is the test's error: an `a` from the word `redacted` in what is "left".
- A value's byte left outside a marker, or a form of a value left whole, is a bug in the code.

## New forms

Property 1 reads `needles_for` itself, so a form added there is checked with no edit to the tests.

## Open findings

None.
