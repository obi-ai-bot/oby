# Keychain: no password prompts from Oby

The business behavior contract for what obid asks of the macOS Keychain, from [the template](../business-contract-template.md). Issue [#1679](https://github.com/obi-ai-bot/oby/issues/1679). Scenarios are numbered `KEYCHAIN-n`; the tests that prove them are named after them (`keychain_1_…`).

## Feature and status

- **Outcome:** Starting or upgrading Oby never makes macOS ask the owner's Mac password, and obid answers while it starts. The owner's words of 2026-10-31: "WE NEED TO REMOVE THIS FOREVER, having to put the password 4 times is stupid."
- **Status:** **Implemented.** Until #1679, an obid starting in the Mac app's runner home with no `vault/secrets.json` copied the Swift runner's vault items (service `com.deskree.oby.runner.vault`) out of the Keychain. The Swift app made those items, so macOS asked the owner's password for each one, and obid answered no request until all were answered (4 prompts on the owner's Mac).
- **Scope:** obid on macOS: its vault, Tetrix's login, and the agent logins it copies (`claudeKeychain`, `cursorKeychain`). **Out of scope:** the Electron app's own device key (docs/contracts/signed-words.md) and the Swift apps.

## Behavior

### Actors and ownership

- **obid** keeps Oby's own secrets (the vault: Tetrix's login, the owner's connections) in `home/vault/secrets.json` only (0600), and reads Claude Code's and Cursor's own Keychain logins with `/usr/bin/security`.
- **Claude Code and cursor-agent** write their Keychain logins with `/usr/bin/security`, so macOS lets that program read them back without asking.
- **The apps** show what obid says: Tetrix's `error` in Settings, an account's `label`, a task's failure.

### Scenarios

- **KEYCHAIN-1:** obid never reads or writes the Keychain for its own secrets: no Security framework in obid, no Keychain store for the vault, no copy out of the Keychain. A configuration's `{"store": "keychain"}` is read as the file.
- **KEYCHAIN-2:** A Mac upgrading from a vault in the Keychain starts with no prompt. A saved Tetrix connection whose login isn't in the file is no longer connected, and says once, in Settings: "Connect Tetrix again: Oby no longer reads its logins from your Keychain, so the one kept there didn't carry over." The old Keychain items stay, unread.
- **KEYCHAIN-3:** A Claude Code or Cursor Keychain login that macOS doesn't give obid (the owner chose Deny, or answered nothing within 60 seconds, or the Keychain was locked) isn't asked for again until obid restarts. That account isn't signed in; its label says "Restart Oby, then Always Allow", and a run on it says "Claude Code keeps its login in your Keychain, and macOS asks once whether Oby may read it: that was denied or not answered, so Oby won't ask again until it restarts. Restart Oby and choose Always Allow."

### What the owner can still see

- No prompt in the usual case: Claude Code and cursor-agent write their items with `/usr/bin/security`, and obid reads them with it.
- A prompt only when an item was made by another program (an older Claude Code, a tool that writes the Keychain itself) or the login Keychain is locked. Then macOS asks once per item; Always Allow ends it for good, and Deny or no answer ends it until obid restarts (KEYCHAIN-3). A value is read when a run starts and while the keeper refreshes a login, off the request path; Cursor's is also read when an app asks `GET /v1/status`, at most once a minute, so that one answer can wait for the owner's choice, once.
- While obid starts it asks only whether an item exists, which reads no secret and never prompts for an item, each answer within 5 seconds.

## Invariants

- No vault secret is ever read from or written to the Keychain.
- No Keychain value is asked for twice in one obid run after macOS refused it.
- obid's API is add-only: no key or route is removed; `tetrix.error` gains one text.

## Tests

| Scenario | Test |
|---|---|
| KEYCHAIN-1 | `vault::tests::keychain_1_the_vault_never_touches_the_keychain`, `vault::tests::where_the_vault_is` |
| KEYCHAIN-2 | `tetrix::tests::keychain_2_a_login_left_in_the_keychain_asks_to_connect_again` |
| KEYCHAIN-3 | `jobs::logins::tests::keychain_3_a_refused_login_is_asked_for_once` |

## Test it when it's done (on a real Mac)

1. On a Mac with Oby Electron 0.1.0 and its vault items in the Keychain, move `~/Library/Application Support/Oby/Runner/vault/secrets.json` aside, install the signed release with this obid, and open Oby. No password prompt; Home shows within seconds.
2. Settings › Connectors: Tetrix asks to be connected again, with the text above. Connect it; jobs use it.
3. `~/Library/Logs/obid.log` has no `Keychain` line from the vault.
