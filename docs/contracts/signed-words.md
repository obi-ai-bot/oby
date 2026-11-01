# The owner's words, signed

The business behavior contract for the Electron app signing what the owner types or clicks ([#1611](https://github.com/obi-ai-bot/oby/issues/1611)), from [the template](../business-contract-template.md). Scenarios are numbered `WORDS-n`; the tests that prove them are named after them: the app's in `apps/electron/src/renderer/src/test/words.test.ts` and `apps/electron/scripts/test-core.mjs` (`npm run test:core`), obid's in `core/crates/obid/tests/secretspec.rs` (`words_5_…`).

## Feature and status

- **Outcome:** Words the owner types or clicks in the Electron app (Home's box, a conversation, a task's page, a task's new name, a question's answer, a permission's Allow or Deny) count as the owner's own in obid, as the Swift apps' do. Their conversation isn't marked, the tasks it starts keep their check's declared secrets, and the owner's Bypass carries on to the follow-ups their words start (BYPASS-7). Words the owner didn't type or click stay unsigned, and obid marks them as before.
- **Status:** **Implemented** (#1611). obid already checked `X-Oby-Words` (docs/runner-api.md, "Words the owner typed"); the Electron app signed only a limit-stopped job's continuation. Since #1632 a task's words open its conversation, so unsigned words marked every conversation the app started.
- **Scope:** the Electron app (main, the preload, the page's Content-Security-Policy, the window's sandbox). **Out of scope:** obid's verification (unchanged), the Swift apps (they sign already), offers' Start and Cancel, Later, a task's Mode (no words the owner typed; unsigned as before), `POST /v1/jobs` and `POST /v1/memory/suggestions/{id}/remember` (the app sends neither: each gets its exact shape here when it does).

## Behavior

### Actors and ownership

- **The owner** presses Enter in a box, or clicks (Send, an option, Allow).
- **Main** (`src/main/words.ts`, `device.ts`, `index.ts`) decides. It signs with this Mac's device key only when all of these hold:
  1. it saw a real keystroke (Enter) or mouse-down reach the window itself (`before-input-event`, `before-mouse-event`), less than 2 seconds before, not used by another signature (each one signs once);
  2. the request is one of the app's words requests, in exactly the shape the app sends it (`wordsOf`, `src/shared/words.ts`): `POST /v1/plans` (`said`; an answer's `question` and `option`; `conversation` or `new`; `folders`, `id`, `attachments`, `jobs`), `POST /v1/jobs/{id}/messages` (`text`, `attachments`, `conversation`), `PATCH /v1/jobs/{id}` (`title` only), `POST /v1/inbox/{id}/answer` (`answer` only), `POST /v1/jobs/{id}/permission` (`id`, `decision`, `message`). Any other key (a check, a mode, a repository, a base, an account) and it signs nothing. Working folders (`folders`) go signed only when every one is a folder the owner chose in main's own dialog this run (`pick-folders`): a recent folder from an earlier run, or one the page made up, sends the words unsigned;
  3. the preload asked for it.
- **The preload** (`src/preload/index.ts`) asks only when the request's words are text the owner's own event held: the box they pressed Enter in, and for a click, the boxes of the form clicked in (Send) and the label of the `<button>` clicked (never a card's text). Only `isTrusted` events count, and a box or button that isn't visible (`checkVisibility` with opacity and CSS visibility) holds nothing. Each held text signs once, within 2 seconds.
- **obid** checks the signature as it does the Swift apps' and decides what the words may do.

### Decisions and invariants

- **Nothing is signed because the page asks.** Before #1611 main signed no ordinary words on purpose: the page shows text from outside (agents' output, GitHub, other people's words), and a script that got into it could have had main sign any words as the owner's, so the tasks they start get the check's declared secrets. The page still can't: main signs only right after a keystroke or click it saw itself, one request each, and only the app's own shapes.
- **The window is sandboxed** (#1641): `sandbox: true`, with `contextIsolation` and no `nodeIntegration`, and `app.enableSandbox()` for any window added later; a packaged app refuses `--no-sandbox` (it exits). The page's process, the preload's world included, has no Node (no `process`, `require`, files or programs) and runs in Chromium's sandbox; the preload is CommonJS and requires only `electron`'s `contextBridge` and `ipcRenderer`. Before it, a script that took the renderer over had Node, the Mac's files and programs, already worse than the ceiling below. Now it reaches only what the preload hands main over IPC, and main checks each sender (`trusted`) as before; escaping takes a Chromium sandbox escape.
- **What a compromised page can still do.** The preload still shares the page's process (in a world of its own), so a script that takes the whole renderer over can pass the preload's check. What it still can't pass is main's: within 2 seconds of each real Enter or click of the owner's in Oby's window, it can have one request of its own choosing signed, words of its choosing in one of the shapes above. Those words count as the owner's: words to Oby that ask for a task, with a check of their choosing, start a vetted task whose check gets the owner's declared secrets. It can't sign without the owner's input, more than once per input, or a body with any other key. **The owner accepted this ceiling on 2026-10-30** (Wellington, merging #1638), as he accepted `claudeOwnSandbox`'s race on 2026-10-27: it closes only with words read in a UI main owns. The window's sandbox (above) doesn't change it: the ceiling is what's left inside the sandbox. Keeping script out of the page stays the first line: React's escaping, no remote content, the window's own frame only (`trusted`), and a Content-Security-Policy in every build (`vite.config.ts`: the page's own scripts only, no network).
- **Exactly the words held.** Spaces don't count (the box's lines come back as the page joins them). Anything more, less or else goes unsigned, a repository put in front of the words (`In owner/name: …`, the design's `?everything` only) too.
- **An answer to Oby's question** sends again the words it answers (`said`): those may be words to Oby this app signed before; its `option` must be held now.
- **Unsigned is never refused by the app.** Words that aren't signed (a rename that ends when the window loses focus, a send obid didn't answer and that goes again later, words that wait for their files' upload) go unsigned, and obid marks them as it always did. Only a limit-stopped job's continuation needs the signature (docs/runner-api.md, `accountLimit`), as before.

### Failure and recovery

- The device key can't be read or the device can't register (the Keychain unavailable, obid refuses it): the words go unsigned, never dropped, and main logs why. The device registers once a run, one registration at a time (`home/pairing` is written by one at once).
- A signature obid doesn't take (the clock, a device it removed) leaves the words marked, as unsigned ones are; obid logs why.

## Interfaces and dependencies

`X-Oby-Words: <device> <signedAt> <signature>` (docs/runner-api.md, "Words the owner typed"), signed to the millisecond over the body as main sends it. No change to obid's API.

## Acceptance scenarios

- **WORDS-1** Main signs the app's words requests only, in exactly their shapes: a job body, a message or plan with a mode or check, a title with a base, settings, a task's mode, a check, a conversation's title, Later and any non-text field are never signed.
- **WORDS-2** Words the owner's keystroke or click held are signed, each held text once (with spaces joined differently); words the page made up, nothing held, more than was held, or a repository put in front are not. A permission is signed by its own button's label.
- **WORDS-3** An answer to Oby's question signs the words it answers when they were signed before and the option is held; neither alone, and words signed before don't go signed to a task.
- **WORDS-4** Main's signature verifies with the device's key over `oby-words\n<METHOD>\n<path without query>\n<SHA-256 of the body>\n<signedAt>`.
- **WORDS-5** (BYPASS-7, through obid's real routes) A signed `POST /v1/jobs` in Bypass is vetted; a signed message starts a vetted follow-up in Bypass; the same words unsigned start one that isn't vetted, in the owner's default mode.
- **WORDS-6** In the real app against a scratch obid: the task Home's box starts is vetted, and so is the follow-up the owner's words on its page start. The page's Content-Security-Policy blocks nothing the app loads.
- **WORDS-8** Words with working folders are signed only when the owner chose every one in main's own dialog this run; one more, or one never chosen, sends them unsigned.
- **WORDS-7** Main signs once per real keystroke or click it saw, within 2 seconds; in the real app, words the page's script sends with no keystroke or click start a follow-up that isn't vetted.
- **WORDS-9** In the real app, the window's process runs sandboxed (`app.getAppMetrics()` says `sandboxed: true` for its process: that is the check that proves it, since the page lacks `process` and `require` with `contextIsolation` alone too), the page reaches neither, and the preload's bridge (`window.app`, obid through main) still answers. A packaged app refuses `--no-sandbox`.

### Test it when it's done (a real Mac)

1. Type a task in Home's box: it starts, and `home/jobs/<id>.json` says `"vetted": true`.
2. On its page, tell it something: the follow-up is vetted, and in Bypass when the task was.
3. Answer an agent's question with an option, and with Tell it something; Allow a command: the task stays vetted.
