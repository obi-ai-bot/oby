# Adding accounts

The business behavior contract for connecting any number of accounts of any tool easily, from [the template](../business-contract-template.md). Scenarios are numbered `ACCOUNTS-n`; the tests that prove them are named after them (`accounts_1_…` in `core/crates/obid/tests/accounts_api.rs` and `core/crates/obid/src/accounts.rs`, `accounts_18_…` in `core/crates/obid/src/jobs/logins.rs`, `ACCOUNTS-10:` to `ACCOUNTS-16:` in `apps/electron/src/renderer/src/test/addAccount.test.ts`).

## Feature and status

- **Outcome:** The owner connects another account in two clicks: Add account, the tool, and the tool's own sign-in is open. A name is optional and comes after; a login already on the Mac is one click. Each account then shows who it is and how full it is. In the owner's words of 2026-11-02: "easily connect my accounts, connect any amount of them, choose which to use, easily see the percentage."
- **Status:** **Implemented**, except the real-Mac steps below. Sangam's design has no screen for this sheet yet: the sheet's layout and every sentence on it are **proposed**, his to rewrite. OpenAI's Sign in with ChatGPT is a separate piece, not here.
- **Scope:** Settings › Agents and obid's accounts. First run's Tools step keeps its rows and its name. **Out of scope:** Settings › Accounts (`?everything` only) keeps its form; the iPhone and Android can't add an account (the gateway refuses it).

## Behavior

### Actors and ownership

- **obid** owns who an account is, what a new one is called, and whether a sign-in was another account's login again. It runs each tool's own status the way it runs that tool's sign-in: the account's own folder, or for the tool's own login the person's home; a `PATH` of the system's folders and the tool's own; no token, Claude Code, Anthropic or GitHub variable; no prompt, no job, no box.
- **The Electron app** shows the sheet and its words, and starts what obid does.

### The sheet

| Step | Shows (proposed words) | Then |
|---|---|---|
| Tools | A row per tool, `Claude Code`, `2 added` or `None yet`, `Sign in`; under a tool, its login found on this Mac (`availableLogins`): `Codex’s login`, `Found on this Mac`, `Use it` | `Use it` lists that login and closes. `Sign in` goes on at once, with no question. A sign-in that couldn't start shows its reason's sentence (docs/contracts/sign-in.md), and the account it made is gone |
| Signing in | `Finish on Codex’s sign-in page · enter ABCD-EFGHI`; Claude Code's paste field; `Open the page again`; `Cancel` | Signed in goes on by itself. `Cancel`, closing the sheet, or the page running out (15 minutes: `Didn’t finish signing in. Try again.`) ends the sign-in and takes the account off |
| Signed in | `Signed in as me@x.dev · Deskree · Team`; a field with the name obid gave it, `Name (optional)`; `Done` | A changed name is kept; anything else keeps obid's |
| The same login | `That’s Personal again`; `Done` | Nothing was added |

Settings › Agents has `Add account` above the cards, with `Found on this Mac: Codex` under it while there's one. Each card shows who the account is under its tool, and its percentage as a bar: its 5-hour session, else its week (Cursor's: its plan's included usage, or "No usage" with none, docs/contracts/usage.md, USAGE-6).

### Who an account is

`email`, `plan` and `org` on the Account, each only when the tool gave one, and nothing else of the tool's answer is kept, shown or logged:

| Tool | Asked with | `email` | `plan` | `org` |
|---|---|---|---|---|
| Claude Code | `claude auth status --json`, while `loggedIn` | `email` | `subscriptionType` | `orgName`, but not a personal login's `<someone>'s Organization` |
| Codex | `account/read` over `codex app-server`, `refreshToken: false` | `account.email` | `account.planType`, but not `unknown` | none |
| Cursor | `cursor-agent status --format json` | `userInfo.email` | none (it says none) | `userInfo.teamName` |

obid asks after every sign-in that took, for every signed-in account as it starts, and when a tool's own login is listed. A login the person changed outside Oby shows at the next start. An answer with nothing to say keeps what obid knew, but a new sign-in forgets it. An email is what holds an `@`; each value is one line, at most 254 characters (email), 120 (org) or 60 (plan), with no character that breaks a line or turns text around unseen (U+2028, U+2029, zero-width marks, bidirectional overrides). They're kept in `home/accounts.json` (`identities`, 0600).

### A new account's name

`POST /v1/accounts` no longer needs a `name` (add-only: the key stays, and a given one is checked as before). Without one the account takes its tool's name, unique per tool (`Claude Code`, `Codex 2`; a name obid picks is never its tool's own login's, `Codex` or `Cursor`, or that login could no longer be listed), until its first sign-in fills it from the plan with a capital, unique per tool (`Max`, `Max 2`, cut to room for the number). Never from the org: an org is a name someone else chose, and an account's name reaches agents unquoted (a handoff, the brain's prompt). The org shows on the card. A tool that gives no plan leaves obid's name, as Cursor does. A name the owner gave, when adding it or by `PATCH` before it signed in, is never replaced, and obid renames an account only that once. A tool with nothing to say leaves obid's name.

### The same login twice

A sign-in on an account just added (never signed in before) with the `email` (letter case aside), `org` and `plan` of another signed-in account of the same tool takes the just-made account off: folder, usage and settings, as `DELETE` does. The other account is untouched. Jobs stay off the account until this is settled. Its sign-in routes then answer `409`, `reason` `sameAccount`, `sameAs` the other account's id, until obid restarts, and the app says `That’s <name> again`.

It stays when: the tool said no email (Cursor out of reach; nothing to compare); the org or plan differs (a personal plan and a team's seat on one email are two accounts, and a plan that changed since gives an extra account, never a lost one); the account was signed in before (signing it in again as someone else is the owner's choice); the other is another tool's; or the other is signed out (that one couldn't run, and this one can). As with `DELETE`, what the tool itself keeps stays: on macOS, the Keychain item Claude Code made for the folder.

### Invariants

- The status reads never get a token: obid takes the named keys of what the tool prints, and drops the rest.
- `email`, `plan` and `org` go only to the owner's own apps (`GET /v1/status`, the `accounts` event). No agent, prompt, job, other Oby or Obysphere gets them.
- `limits.accounts` (32 by default, up to 256) is the only cap on how many accounts there are, of one tool or of all.
- No box ever gets an account's folder; agents have no code-host login: nothing here changes either.
- Older apps ignore the new keys. A new key in an Account is absent while it has no value.

## Failure and recovery

- A status that doesn't answer in 15 seconds, or isn't JSON: nothing is learned, the account stays as it is, and the same login twice isn't noticed for that sign-in.
- `accounts.json` that can't be written: what was learned is logged and lost until the next start. The account stays signed in.
- A just-made account that can't be taken off stays, as an account of its own, with a line in obid's log.
- A boxed Codex run that ends on a bare 401 while its copy's access token JWT `exp` has already passed says `The Mac's Codex login for <account> needs refreshing: run codex once.` and leaves the account signed in: the copy has no refresh token and simply expired. A revocation (`token_revoked`, `refresh_token_invalidated`, …) on that expired copy, or any login refusal while `exp` is still ahead, ends the login (`run codex login`) as before. Recovery still does not retry a login error.

## Interfaces and dependencies

`POST /v1/accounts` (`name` optional), Account's `email`, `plan`, `org`, `reason` `sameAccount` and `sameAs` on `GET` and `POST /v1/accounts/{id}/sign-in` (docs/runner-api.md, "Account", "obid only: accounts", "Why a sign-in didn't start").

## Acceptance scenarios

- **ACCOUNTS-1:** `POST /v1/accounts` with no name → `201`, its tool's name, unique per tool, its folder made; a given name is as it was; an empty or taken one is `422`.
- **ACCOUNTS-2:** its first sign-in names it from the plan, unique per tool, never the org; a name given when adding it or changed before it signed in stays; a tool with nothing to say leaves obid's.
- **ACCOUNTS-3:** after signing in, the Account has `email`, `plan`, `org` from the tool, and only those of what it printed, kept across a restart; before, none of the keys; what else the tool said is in no file, answer or log.
- **ACCOUNTS-4:** a personal Claude Code login's own `<someone>'s Organization` is no org: its name comes from its plan.
- **ACCOUNTS-5:** Cursor's email and org come from `userInfo`; with no `userInfo` (cursor-agent 2026.10.01 out of reach) there's nothing to show, and nothing breaks.
- **ACCOUNTS-6:** as obid starts, a signed-in account, a tool's own login too, gets who it is, and keeps its name.
- **ACCOUNTS-7:** a new account signed into the email, org and plan of another signed-in account of the tool is taken off with its folder; `GET` and `POST` sign-in answer `409` `sameAccount` with `sameAs`.
- **ACCOUNTS-8:** two tools' accounts may share an email; a tool with no email keeps both; an account signed in before keeps its place; a signed-out account isn't what a new sign-in is the same as.
- **ACCOUNTS-9:** with `limits.accounts` at 40, 40 accounts can be added, and the 41st is `422`.
- **ACCOUNTS-10:** the sheet has a row per tool with how many it has, and the logins found on this Mac under theirs.
- **ACCOUNTS-11:** picking a tool posts no name, starts its sign-in and opens its page at once.
- **ACCOUNTS-12:** a sign-in refused takes back off the account it made, and shows the reason's sentence.
- **ACCOUNTS-13:** Cancel ends the sign-in and takes the account back off.
- **ACCOUNTS-14:** an account gone with `sameAccount` says `That’s <name> again`; gone for any other reason, nothing.
- **ACCOUNTS-15:** a card shows who it is and a bar for its session, else its week, else none.
- **ACCOUNTS-16:** first run still connects an agent under the name it gives.
- **ACCOUNTS-17:** one email on another plan or org is another account, and both stay.
- **ACCOUNTS-18:** a boxed Codex copy whose access token's JWT `exp` has passed, ending on a bare 401, says `run codex once` and stays signed in; a revocation on that expired copy, or a 401 or revocation while `exp` is still ahead, still ends the login (`run codex login`).

**Test it when it's done (real Mac):**

1. Settings › Agents › Add account › Codex: the page opens with its code; sign in with an account Oby doesn't have. The sheet says `Signed in as <you> · Plus`; the card is named `Plus` and shows its bar.
2. Again with the same OpenAI account: `That’s Plus again`, and there's one card.
3. The same with Claude Code (paste its code), and with a Claude Team seat on the same email: two cards, the second named `Team` and showing the org under its tool.
4. Run `cursor-agent login` in Terminal, restart obid: Cursor's own login shows its email. With Cursor's login ended, it shows none and still works. **Not yet checked with a live Cursor login:** this Mac's has ended, so `userInfo` is read from cursor-agent 2026.10.01's own code, not seen in an answer.
5. Remove Codex's own login, then Add account: `Codex’s login`, `Use it`, one click.
