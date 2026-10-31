# The owner's words, signed

The business behavior contract for the Electron app signing what the owner types or clicks ([#1611](https://github.com/obi-ai-bot/oby/issues/1611)), from [the template](../business-contract-template.md). Scenarios are numbered `WORDS-n`; the tests that prove them are named after them: the app's in `apps/electron/src/renderer/src/test/words.test.ts` and `apps/electron/scripts/test-core.mjs` (`npm run test:core`), obid's in `core/crates/obid/tests/secretspec.rs` (`words_5_…`).

## Feature and status

- **Outcome:** Words the owner types in the Electron app (Home's box, a conversation, a task's page, a task's new name, a question's answer) count as the owner's own in obid, as the Swift apps' do. Their conversation isn't marked, the tasks it starts keep their check's declared secrets, and the owner's Bypass carries on to the follow-ups their words start (BYPASS-7). Words the owner didn't type or click stay unsigned, and obid marks them as before.
- **Status:** **Implemented** (#1611). obid already checked `X-Oby-Words` (docs/runner-api.md, "Words the owner typed"); the Electron app signed only a limit-stopped job's continuation. Since #1632 a task's words open its conversation, so unsigned words marked every conversation the app started.
- **Scope:** the Electron app (main and the preload) and the requests it sends obid. **Out of scope:** obid's verification (unchanged), the Swift apps (they sign already), offers' Start and Cancel, Later, a task's Mode, and the permission route's decisions (they carry no words the owner typed, and stay unsigned as before).

## Behavior

### Actors and ownership

- **The owner** presses Enter in a box, or clicks (Send, an option, away from a name they edited).
- **The preload** (`src/preload/index.ts`) records, for 30 seconds, the text that real keystroke or click held: the box they pressed Enter in, or were in when they clicked, and the label of the button they pressed. Only events the person made count (`isTrusted`), read in the preload's own world, which the page's script can't reach. When the page sends a request, the preload asks main to sign it only when the request's words are text so held (`owners`, `src/shared/words.ts`).
- **Main** (`src/main/device.ts`, `src/main/words.ts`) signs with this Mac's device key, on the routes that carry words only (`wordsOf`), whatever it's asked: `POST /v1/plans` (`said`, and an answer's `option`), `POST /v1/jobs` (`job.task`, `job.title`), `POST /v1/jobs/{id}/messages` (`text`), `PATCH /v1/jobs/{id}` (`title`) and `POST /v1/inbox/{id}/answer` (`answer`).
- **obid** checks the signature as it does the Swift apps' and decides what the words may do.

### Decisions and invariants

- **Nothing is signed because the page asks.** Before #1611 main signed no ordinary words on purpose: the page shows text from outside (agents' output, GitHub, other people's words), and a script that got into it could have had main sign any words as the owner's, so the tasks they start get the check's declared secrets. That protection stays: the page still can't make main sign text by asking. Main signs only when the preload saw the owner's own keystroke or click hold that text, and only on the words routes.
- **Exactly the words held.** Spaces don't count (the box's lines come back as the page joins them), and the repository the owner picked may go in front (`In owner/name: …`). Anything more, less or else goes unsigned.
- **An answer to Oby's question** sends again the words it answers (`said`): those may be words to Oby this app signed before; its `option` must be held now. Words signed before never go signed to anything else.
- **Unsigned is never refused by the app.** Words that aren't held (a rename that ends when the window loses focus, a pick whose label differs from what it sends) go unsigned, and obid marks them as it always did. Only a limit-stopped job's continuation needs the signature (docs/runner-api.md, `accountLimit`), as before.
- **Its ceiling:** a script already running in the page can put its own text in a visible box before the owner's real Enter or Send, and that is signed. The owner sees what's in the box they send; keeping scripts out of the page (React's escaping, no remote content, the window's own frame only) stays the first line.

### Failure and recovery

- The device key can't be read or registered (the Keychain unavailable, obid refuses the device): the request answers main's error, as a Confirm's does, and the owner sends again.
- A signature obid doesn't take (the clock, a device it removed) leaves the words marked, as unsigned ones are; obid logs why.

## Interfaces and dependencies

`X-Oby-Words: <device> <signedAt> <signature>` (docs/runner-api.md, "Words the owner typed"), signed to the millisecond over the body as main sends it. The device is registered (`POST /v1/devices`) before each signature, as a Confirm's is. No change to obid's API.

## Acceptance scenarios

- **WORDS-1** Main signs the words routes only, and only their text: settings, a task's mode, a check, a conversation's title, Later and any non-text field are never signed.
- **WORDS-2** Words the owner's keystroke or click held are signed (with spaces joined differently, or the picked repository in front); words the page made up, nothing held, or more than was held are not.
- **WORDS-3** An answer to Oby's question signs the words it answers when they were signed before and the option is held; neither alone, and words signed before don't go signed to a task.
- **WORDS-4** Main's signature verifies with the device's key over `oby-words\n<METHOD>\n<path without query>\n<SHA-256 of the body>\n<signedAt>`.
- **WORDS-5** (BYPASS-7, through obid's real routes) A signed `POST /v1/jobs` in Bypass is vetted; a signed message starts a vetted follow-up in Bypass; the same words unsigned start one that isn't vetted, in the owner's default mode.
- **WORDS-6** In the real app against a scratch obid: the task Home's box starts is vetted, and so is the follow-up the owner's words on its page start.

### Test it when it's done (a real Mac)

1. Type a task in Home's box: it starts, and `home/jobs/<id>.json` says `"vetted": true`.
2. On its page, tell it something: the follow-up is vetted, and in Bypass when the task was.
3. Answer an agent's question with an option, and with Tell it something: the task stays vetted.
