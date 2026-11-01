# Sign-in errors

The business behavior contract for why a sign-in didn't take, shown under its own row of Sangam's first run (#1579), from [the template](../business-contract-template.md). Scenarios are numbered `SIGN-IN-n`; the tests that prove them are named after them (`sign_in_1_…` in `core/crates/obid/tests/sign_in_reasons.rs`, `sign_in_13_…` in `core/crates/obid/tests/github_api.rs`, `SIGN-IN-10:` to `SIGN-IN-12:` in `apps/electron/src/renderer/src/test/signIn.test.ts`).

## Feature and status

- **Outcome:** A new owner connecting Claude Code, Codex, Cursor, GitHub or Tetrix on first run sees, under that row, one plain sentence saying why it didn't connect, and the row's Connect again. Never a log line, a CLI's output, a stack trace or an HTTP status.
- **Status:** **Implemented** (#1579). Sangam's design (PR #1564: `SetupCard.tsx`'s `Row` and its `why`, `FirstRun.tsx`'s Tools and Knowledge steps) is the spec for where it shows. His screen has no words for these failures yet, so the sentences below are proposals in `signInWhy.ts`, his to rewrite.
- **Scope:** the first run's rows (Tools: the three agents and GitHub; Knowledge: Tetrix), and the same rows in the first conversation's setting up. **Out of scope:** Settings › Accounts and Connectors, which still show obid's `error` (a follow-up can use the same words); the GitHub reviewer's sign-in; Tetrix's `error` after a refresh.

## Behavior

### Actors and ownership

- **obid** decides why a sign-in can't start and says it as a stable `reason` beside its `error` on the agents', GitHub's and Tetrix's sign-in routes. A `reason` is a word, never a sentence, and is add-only: never renamed or removed.
- **The Electron app** shows, under the row, its own sentence for the `reason`; for no `reason`, one it doesn't know, or obid not answering, the plain one: `Couldn’t connect <name>. Try again.` It never shows obid's `error` on a row. The sentence stays until the next try of that row.

### Reasons

| `reason` | When | Status | The row says |
|---|---|---|---|
| `notInstalled` | the tool's CLI (or gh) isn't found by an absolute path | `503` | `Cursor isn’t on this Mac.`; GitHub: `GitHub’s own tool (gh) isn’t on this Mac.` |
| `ownLogin` | the account is the tool's own login, which the tool signs in itself | `409` | `Sign in with Claude Code itself on this Mac, then Connect again.` |
| `folderUnsafe` | the account's login file, its lock or folder is a link or not the person's (`409`), or its folder isn't one or can't be made (`502`) | `409`, `502` | `Codex’s login folder isn’t one Oby can sign in through.` |
| `keychainOff` | Claude Code on macOS with `claudeKeychain` off | `409` | `Oby can’t use Claude Code’s login in your Keychain yet.` |
| `busy` | a task of the account is queued or running, it's being removed, or GitHub's sign-in is starting | `409` | `Codex is busy with a task. Try again when it’s done.`; GitHub: `GitHub’s sign-in is already starting. Try again in a moment.` |
| `didNotStart` | the login (or gh) gave no link or code, or couldn't start; Tetrix's sign-in server didn't register Oby | `502` | `GitHub’s sign-in didn’t start. Try again.` |
| `codeRefused` | Claude Code didn't sign in with the code pasted | `422` | `That code didn’t work. Connect again for a new one.` |
| `badAddress` | a Tetrix address that isn't HTTPS (or loopback HTTP), one with a login, a fragment or a query other than `?org=<uuid>` (the organization a login in several answers for, #1667), or an unknown preset | `422` | `That isn’t a Tetrix address Oby can use.` |
| `noSignIn` | that Tetrix's metadata, sign-in server or client can't be found or used | `502` | `That Tetrix doesn’t say where to sign in.` |

A sign-in that started and then ended without signing in (its page closed, gh or the Tetrix exchange failing, 15 minutes for an agent or GitHub, 10 for Tetrix) puts Connect back with `Didn’t finish signing in. Try again.` The app knows it ended when `GET /v1/github` or `GET /v1/tetrix` says no sign-in is under way and nothing is signed in. obid makes that answer true only once it is: `GET /v1/github` reads the sign-in before gh's status, and a sign-in's end forgets the status before it shows as ended, so a status read that began before the end keeps nothing.

### Invariants

- No sentence under a row holds a token, a path, a device code or anything a CLI printed: the words are the app's, picked by a fixed word.
- obid's `error` stays as it was for the Swift apps and Settings; `reason` is a new key, so older apps ignore it.
- A sign-in's own code still shows while it's under way (Codex's, GitHub's): that's the row's code, not an error.

## Interfaces and dependencies

`reason` on the error body of `GET /v1/accounts/{id}/sign-in`, `POST /v1/accounts/{id}/sign-in`, `POST /v1/github/sign-in`, `POST /v1/github/reviewer/sign-in`, `DELETE /v1/accounts/{id}` (`busy`) and `POST /v1/tetrix/sign-in` (runner-api.md, "obid only: accounts", "GitHub's sign-in", "Tetrix").

## Acceptance scenarios

- **SIGN-IN-1:** a tool whose CLI isn't on this Mac → its sign-in answers `503`, `reason` `notInstalled`. (gh missing answers the same; not tested, since every Mac that builds obid has one.)
- **SIGN-IN-2:** the tool's own login (no folder) → `409` `ownLogin`.
- **SIGN-IN-3:** an account whose `auth.json` is a link → `409` `folderUnsafe`.
- **SIGN-IN-4:** an account with a task running → `409` `busy`.
- **SIGN-IN-5:** Claude Code on macOS with `claudeKeychain` false → `409` `keychainOff`.
- **SIGN-IN-6:** a login or gh that prints nothing and ends → `502` `didNotStart`, for an agent and for GitHub.
- **SIGN-IN-7:** a code Claude Code refuses → `422` `codeRefused`.
- **SIGN-IN-8:** a Tetrix address over plain HTTP off loopback → `422` `badAddress`.
- **SIGN-IN-9:** a Tetrix address where nothing answers → `502` `noSignIn`.
- **SIGN-IN-10:** the app shows the reason's sentence under the agent's, GitHub's and Tetrix's rows, and none of obid's `error` (a path, a code, a token).
- **SIGN-IN-11:** no reason, an unknown one, or obid not answering → `Couldn’t connect <name>. Try again.`
- **SIGN-IN-12:** GitHub's or Tetrix's sign-in that ends without signing in → the row back to Connect with `Didn’t finish signing in. Try again.`; the next try clears it.
- **SIGN-IN-13:** a `gh auth status` read that began while gh's sign-in was under way and answers after it ended → the next `GET /v1/github` with no sign-in under way says `signedIn` true.
- **SIGN-IN-14:** a Tetrix address ending in `?org=<uuid>` (either case) signs in, the id written in lower case, and every call goes there with it; any other query, a malformed id or anything after it → `422` `badAddress`. Without it, a login in several organizations has every call refused (HTTP `400`), and the job says Tetrix's reason and to add `?org=<your organization's id>` (#1667).

**Test it when it's done (real Mac):** on first run, rename `cursor-agent` out of the PATH and Connect Cursor (`Cursor isn’t on this Mac.`); start GitHub's sign-in and close the browser's page, then cancel gh's device code (`Didn’t finish signing in. Try again.`); type `http://tetrix.example/mcp` in Connect Tetrix (`That isn’t a Tetrix address Oby can use.`).
