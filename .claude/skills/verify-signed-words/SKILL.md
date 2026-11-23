---
name: verify-signed-words
description: Re-run the generated-input test of main's signed words (WORDS-7, WORDS-1, WORDS-16) and read what it covers. Use when a change touches `apps/electron/src/main/words.ts`; `wordsOf` or `WITHIN_MS` in `apps/electron/src/shared/words.ts`; `ownersWords`, `fillSecret` or `confirming`'s `takeInput` calls in `apps/electron/src/main/device.ts`; or the input handlers (`before-input-event`, `before-mouse-event`) in `apps/electron/src/main/index.ts`.
---

# Verifying main's signed words (WORDS-7, WORDS-1, WORDS-16)

The contract is `docs/contracts/signed-words.md`; the test came with task V01-12. It is `apps/electron/src/renderer/src/test/words.property.test.ts`.

## 1. The properties and the harm

- **One signature per real input (WORDS-7).** Each real Enter or click that main saw (`input`) lets main sign one request, and only less than 2 s (`WITHIN_MS`) after it. No input is used twice. A Space on a button (`space`) counts only for a pressed Confirm (`mayConfirm` with an action in `PRESSED`), never for words.
- **Only the app's own shapes and chosen folders (WORDS-1, WORDS-8, WORDS-18).** `wordsHeader` signs a request only when it is one of the app's words requests in exactly its shape (`wordsOf`), and every folder it carries, or every folder of its project as main read them, was picked in main's own dialog this run (`picked`). Any other method, path, id, extra key, key of the wrong kind or unpicked folder gets `null`.
- **The again rule (WORDS-16).** Through `ownersWords`, a signature with no new input is only the very same keyed request, byte for byte, that main signed on an input and obid didn't answer (status 0). It also needs `takesKeys()` true, less than 10 s after the first signature, and at most 2 times. Every resend counts against the 2, even while `takesKeys()` is false.

The harm when one breaks: a script that took the page gets the owner's words signed for requests the owner never made. The accepted risk is one request of its choosing per real input, no more (AGENTS.md, "Approvals").

## 2. What the test covers

Every case is built from a seeded PRNG and carries its expected answer from how it was built. The test never calls `wordsOf`, `owners` or `takeInput` to work out an answer.

- **WORDS-7, forward clock:** 1,500 sequences of 1 to 12 steps over `input(at)`, `space(at)`, `takeInput(at)` and `mayConfirm(item, clicked, at)`. The items are the four `PRESSED` actions, `delete_job` and `{}`, and `clicked` is true or false. Gaps are drawn from {0, 1, 500, 1,999, 2,000, 2,001, 5,000} ms, so the window's edge is hit on both sides. Every call's answer is checked against a model of the unused inputs and Spaces: the oldest in the window is used first, and an input before a Space.
- **WORDS-7, clock stepped back:** 500 sequences, each stepping its clock back once by 1 to 5,000 ms. The window may stretch when the clock goes back, so only the end-of-sequence counts are checked: true takes are at most the inputs seen, and true takes plus pressed, clicked Confirms are at most the inputs plus Spaces seen.
- **WORDS-1:** 2,000 requests through `wordsHeader`, from the shape list `SHAPES` in the test file. It has at least one entry for each of the 20 `wordsOf` branches that return words. Each entry is checked unmutated (signed) and with every mutation that applies to it (`null`, but the `?query`, which `wordsOf` strips by design, except on the artifact import). Random draws then fill up to 2,000. The mutations are:
  - another method;
  - `?x=1`, a trailing `/`, a `..` segment before the last one;
  - an id the shape refuses (an empty segment, a non-UUID artifact id, `current` for a computer's removal);
  - one extra key `check`;
  - one key's value swapped for another kind (number, string, array, object, `null`);
  - a folder never picked, a project whose read folders include one never picked, and a project whose folders main couldn't read.

  The test asserts that every entry and every applicable mutation was checked.
- **WORDS-16:** 1,000 sequences of 1 to 12 `ownersWords('POST', '/v1/jobs/j1/messages', body, owners)` calls, on fake `Date`. Each step draws:
  - an `input()` first or not;
  - `owners`;
  - obid's status, 0 or 200;
  - `takesKeys()`;
  - a body from the case's pool: two keys, other text, keys reordered, unkeyed. About half the steps resend the previous body.
  - a gap from {0, 1, 500, 1,999, 2,000, 2,001, 5,000, 9,999, 10,000} ms.

  Whether obid received `words` is checked against the again rule's model on every call. The run asserts that each of these happened at least 10 times: signed on an input, signed again with no input, refused a third time, refused past 10 s, refused while obid takes no keys.
- **Keeping cases apart:** each WORDS-7 sequence starts `WITHIN_MS` past the latest time before it and drains first. Each WORDS-1 case uses folder names of its own, with the seed and case number in them, since `picked` only grows. Each WORDS-16 case moves the clock 10 s on, drains inputs, and uses request keys of its own (at most 4 kept, under `KEPT`). No `vi.resetModules`.

## 3. Read only, and left to other tests

- **Read only:** `notificationWords` signs with no window input by design (WORDS-15), on main's own notification callback. This test doesn't call it.
- **Left to other tests:**
  - The ECDSA signature itself (`signedHeader`, node's `sign`): library code. `words.test.ts` checks its sentence.
  - Where inputs come from: `before-input-event` (Enter, Space) and `before-mouse-event` (mouseDown) in `apps/electron/src/main/index.ts`. The test calls `input` and `space` as those handlers do.
  - The preload's `owners`, which held text the words are: WORDS-2 and WORDS-3 in `words.test.ts`.
  - `fillSecret` and `confirming`: `device.test.ts`.

## 4. Open findings

None. A finding goes here only by section 7's last rule, shown failing on `origin/dev`.

## 5. Sync

SYNC: 09197a491f54cc132e0146e708f07bb7a3e74924

## 6. The one command

```
cd apps/electron && npx vitest run src/renderer/src/test/words.property.test.ts
```

To replay a failure, put its seed in front: `OBY_SEED=<n> npx vitest run src/renderer/src/test/words.property.test.ts`. Add `-t "WORDS-7"`, `-t "WORDS-1"` or `-t "WORDS-16"` to run one property.

## 7. A bug or a test error

A failure prints the seed and its context: a case's number, its steps with each call's answer and the expected answer, as `seed 20261122, case 29 (replay: OBY_SEED=20261122): expected false at step 10`; or, for WORDS-1's coverage and WORDS-16's outcome counts, the cases run and what was checked or reached.

- **The generator's error.** The expected answer is judged by the property in section 1 and `docs/contracts/signed-words.md`, never by what the code does now. Only when it is wrong by them is the generator in error: fix its model, and never weaken an answer, a count or a mutation to match the code. A change that means to alter a property changes the contract first.
- **A bug the change under review made.** A signature with no unused input in the window, an input used twice, a wrong shape or an unpicked folder signed, or an extra signature with no input is a bug in main. When the change under review brings it, the change is fixed until the test is green. The property is never skipped, removed or weakened for it, and no task file stands in for the fix.
- **A bug already on the base.** Only a property that fails the same way on `origin/dev` before the change becomes a finding. Show it first: run the section 6 command with the same `OBY_SEED` in a checkout of `origin/dev` (`git worktree add <scratch> origin/dev`), and keep its failing output. Then the property stays as written as `it.skip` with the finding's key in its title, a task file goes in `factory/queue/tasks/` with that output, and section 4 lists it. A property green on `origin/dev` and red on the change is the change's bug, above.

## 8. New shapes

A new words shape in `wordsOf` adds its one entry to `SHAPES` in the test file: its method, path, body, its `id` (and a `refused` id when an empty segment isn't refused), `query: false` when a `?query` makes it unsigned, and `folders` or `project` when it carries them. The coverage assertion then checks it unmutated and with every mutation that applies.

## 9. The test still bites

On a committed tree, apply one break to `apps/electron/src/main/words.ts`, run the command in section 6, and see the named test fail. Then `git restore apps/electron/src/main/words.ts` and see it green. Never commit a break.

1. `return inputs.shift() !== undefined;` → `return inputs[0] !== undefined;` (an input is never used up). `WORDS-7: main signs at most once per real input, within 2 s, whatever the order` fails.
2. `if (!wordsOf(method, path, body) || !ownFolders(body, project)) return null;` → `if (!wordsOf(method, path, body)) return null;` (folders unchecked). `WORDS-1: main signs only the app’s own shapes and chosen folders, whatever the mutation` fails.
