# The page's channels into main

The business behavior contract for how the Electron app's page reaches its main process, from [the template](../business-contract-template.md). Roadmap task R06-04 (`factory/queue/done/R06-04.md`); related: [#1641](https://github.com/obi-ai-bot/oby/issues/1641), the window's sandbox. Scenarios are numbered `IPC-n`; the tests that prove them are named after them (`ipc_1_…`, `apps/electron/src/renderer/src/test/ipc.test.ts`).

## Feature and status

- **Outcome:** Nothing the owner sees changes. Underneath, no channel from the page into main can forget to check who sent a message or act on an argument it didn't read first, so a page or frame that isn't Oby's own, or a script that took the page over, gets no more from main than it did, and a new channel gets the same checks by being written the one way main has.
- **Status:** **Implemented** in the Electron app (`apps/electron`): the helper in `src/main/ipc.ts`, the decoders in `src/shared/decode.ts`, every channel moved onto them. `window-chrome` and `full-screen` gained the sender check in this change; every other channel answers as it did.
- **Scope:** the 18 channels main registers for the page (below). **Out of scope:** the preload's API (`src/preload/`), the window's sandbox, context isolation and node integration, `app.enableSandbox()`, what main sends the page (`webContents.send`), what obid accepts on a path (obid stays the authority), a lint rule, and new channels.

## Behavior

### Actors and ownership

- **The page** (the renderer, through the preload's `ipcRenderer.invoke`, `send` and `sendSync`) asks main for each thing it can't do itself.
- **Main** decides who may ask (`trusted` in `src/main/index.ts`: the window's own `webContents` and its main frame, nothing else) and reads each argument with a decoder before any handler runs. Every channel is registered through `ipc(trusted)`'s `handle(channel, decoders, fn, refused, invalid?)` (invoke) or `on(channel, decoders, fn, refused?, invalid?)` (send; sendSync when it names a refusal). `src/main/ipc.ts` is the only file in `src/main` that calls `ipcMain.handle(` or `ipcMain.on(`.
- **obid** owns nothing here and stays the authority on what each `/v1/` path takes.

### Triggers and prerequisites

A message on a channel from the page. For each one, in this order:

1. The sender: anything but the window's own main frame gets the channel's refusal, and nothing else runs.
2. The arguments: argument *i* goes through decoder *i*; arguments past the decoders are ignored. One that a decoder can't read gets the channel's refusal for a bad argument (the same as the sender's, but for `obid`), and the handler doesn't run.
3. The handler runs on the decoded values.

A sendSync channel always gets an answer back (`e.returnValue`): its refusal, or the handler's.

### The decoders (`src/shared/decode.ts`)

A decoder answers `{ value }` or `undefined` when it can't read the value, so `optional`'s successful `undefined` is told apart from a failure.

- `string`, `boolean`, `safeInteger`, `https` (a string starting `https://`), `optional(d)` (`undefined` too), `oneOf(...values)`.
- `method` (exactly `GET`, `POST`, `PUT`, `PATCH` or `DELETE`) and `v1Path` (a string starting `/v1/`), for `obid`.
- Lenient, never failing, for arguments that were lenient before: `flag` (true only for exactly `true`), `orUndefined(d)` (d's value, or `undefined`) and `anything` (as it came: `obid`'s body).
- Built from them in their own files: `publishChoices` (`device.ts`), `showing`'s place (`notify.ts`), `pick-folders`' folder (`index.ts`: an absolute path, else none).

### The channels

| File | Channel | Kind | Arguments (decoders) | Refusal |
|---|---|---|---|---|
| `index.ts` | `window-chrome` | `on`, sync | none | `null` (sender check new in this change) |
| `index.ts` | `full-screen` | `on`, sync | none | `false` (sender check new in this change) |
| `index.ts` | `setup-window` | `on`, sync | `flag` | `null` |
| `index.ts` | `open-external` | `on` | `https` | nothing |
| `index.ts` | `pick-folders` | `handle` | an absolute path, else none (never fails) | `[]` |
| `obid.ts` | `obid` | `handle` | `method`, `v1Path`, `anything`, `flag` | sender: `{ status: 403, text: '' }`; argument: `{ status: 400, text: '{"error":"Not an obid call."}' }` |
| `obid.ts` | `open-login-items` | `on` | none | nothing |
| `obid.ts` | `obid-follow` | `on` | none | nothing |
| `device.ts` | `obid-confirm` | `handle` | `string`, `flag` | `{ status: 403, text: '' }` |
| `device.ts` | `obid-pair-invite` | `handle` | none | `{ status: 403, text: '' }` |
| `device.ts` | `obid-publish` | `handle` | `string`, `string`, `publishChoices` | `{ status: 403, text: '' }` |
| `device.ts` | `obid-limit-answer` | `handle` | `string`, `safeInteger`, `oneOf('move', 'resume', 'stop')`, `optional(string)` | `{ status: 403, text: '' }` |
| `notify.ts` | `showing` | `on` | the place: an object gives `{ job, conversation }` (non-strings `undefined`), else `null` (never fails) | nothing |
| `notify.ts` | `opened` | `handle` | none | `null` |
| `notify.ts` | `notifications` | `handle` | `orUndefined(boolean)` | `null` |
| `notify.ts` | `open-notification-settings` | `on` | none | nothing |
| `updates.ts` | `updates` | `handle` | `orUndefined(oneOf('check', 'install', true, false))` | `null` |
| `sphere.ts` | `sphere-leave` | `handle` | none | `'Not this app’s page.'` |

### Failures and recovery

- A refused message changes nothing in main and reaches neither obid, a dialog, the browser nor Oby's settings; the page gets the refusal above, as it did before this change.
- An `obid` call whose method or path isn't obid's goes no further: no owner's input is taken and nothing is signed (`ownersWords`). `send` in `src/main/obid.ts` keeps its own, stricter check of the method and the `/v1/` path as a second guard, with the same answer.

### Invariants and boundaries

- Who may ask is checked before anything else, on every channel.
- What the window's own page gets with readable arguments is what it got before this change, on every channel; arguments that were lenient stay lenient (`setup-window`'s non-`true`, `pick-folders`' non-absolute folder, `obid`'s non-`true` owners, `obid-confirm`'s click, `notifications`' non-boolean, `updates`' other values, `showing`'s malformed place).
- `trusted`, the window's sandbox, context isolation, node integration and `app.enableSandbox()` are unchanged, and so are the requests main signs (`words.test.ts`, `device.test.ts`).

## Interfaces and dependencies

- **Renderer ↔ main:** the preload's `window.app` (`src/preload/index.ts`), unchanged: the same channel names, arguments and answers.
- No runner API change (`docs/runner-api.md` is unchanged).

## Acceptance scenarios

- **IPC-1** Given a message on any of the 18 channels from a frame that isn't the window's own main frame, when main gets it, then its handler doesn't run (no obid call, no dialog, no page in the browser, no settings written) and the answer is that channel's refusal as listed above (`null` for most; sendSync channels get it as their `returnValue`). The window's own page still gets `window-chrome`'s and `full-screen`'s real answers. `ipc_1_a_sender_that_isnt_the_windows_page_gets_the_channels_refusal_and_nothing_runs`, `ipc_1: the window's own page still gets window-chrome and full-screen`; the test fails when a channel is added or missed.
- **IPC-2** Given the `obid` channel from the window's own page, when the method isn't exactly `GET`, `POST`, `PUT`, `PATCH` or `DELETE`, or the path isn't a string starting `/v1/`, then the answer is `{ status: 400, text: '{"error":"Not an obid call."}' }` and nothing is signed or sent to obid; `GET /v1/status` reaches it, with the owner's words only for exactly `true`. `ipc_2_obid_takes_only_its_methods_and_v1_paths_before_anything_is_signed`.
- **IPC-3** Given any channel, when an argument fails its decoder, then the handler doesn't run and the answer is the channel's refusal (`obid-confirm`'s non-string id, `obid-limit-answer`'s non-integer revision or unknown choice, `obid-publish`'s bad choices: `{ status: 403, text: '' }` with no obid call; `open-external`'s `http://` page: not opened). `ipc_3_an_argument_a_decoder_cant_read_never_reaches_the_handler`, `ipc_3: real channels refuse what they can't read, with no obid call`, `ipc_3: the decoders`.
- **IPC-4** Given the source of `apps/electron/src/main`, when the test reads every `.ts` file in it but `ipc.ts`, then none calls `ipcMain.handle(` or `ipcMain.on(`. `ipc_4_only_main_ipc_ts_registers_a_channel`; seen red with one raw `ipcMain.on` added to `sphere.ts`.

### Test it when it's done (on a real Mac)

1. `npm run dev`: the window opens with its traffic lights where the title bar expects them, full screen moves the title bar over, and setup, sign-in pages, folders, Confirm, Publish, notifications, About's updates and leaving an Obysphere work as before.
