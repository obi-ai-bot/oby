# Vault: secrets put in over loopback

The business behavior contract for listing, putting in and removing the owner's vault secrets through obid's loopback API, from [the template](../business-contract-template.md). The API is [docs/api/vault.md](../api/vault.md) ("obid only: the vault").

## Feature and status

- **Outcome:** The Mac app lists the vault's names, adds a value and removes one without a terminal. The value goes only in a loopback request's body, never in a conversation, a turn or a log, so pasting a key into the conversation (where it reaches the model) is no longer the easy way.
- **Status:** **Implemented** (VAULT-1 to VAULT-5). No screen calls the routes yet: Settings › Vault (Sangam's) is wired in R11-13.
- **Scope:** `GET /v1/obid/vault`, `PUT /v1/obid/vault/{name}` and `DELETE /v1/obid/vault/{name}`, and the Electron app's signing of the two writes. Outside it: the Settings page (R11-13), the secret ask (R11-14), ciphertext at rest (#1029), the `obid vault` command (unchanged), and any route that returns a value, reveals one or renames a secret.

## Behavior

### Actors and ownership

- **obid** checks every write, saves it in the vault (`home/vault/secrets.json`) and takes the value out of everything from the moment it is written, as `obid vault add` does (`Vault::add`, `Vault::remove`). It decides what the routes answer.
- **The Electron app** signs the two writes in main with its device key (docs/contracts/signed-words.md, WORDS-17): a PUT of exactly `{value}` by the label of the page's `Add Secret` button, a DELETE with no body by its `Remove Secret` button's, its own so a click on any other Remove can't stand for it. The value sits in a password box the preload never reads, so the label, not the value, is what the click held; the signature still covers the exact body, the value included.
- **A paired phone** never reaches these routes: the gateway and the phone relay keep `/v1/obid/…` off.

### Rules

- **Loopback only.** The three routes are on obid's loopback API with its token. None is on the gateway's list.
- **Names only out.** `GET` answers `{"names": [...]}`, sorted, without any name starting `oby.` (obid's own: Tetrix's and ChatGPT's logins). No answer of these routes holds a value.
- **Signed writes.** `PUT` and `DELETE` need the owner's signed words over the exact request (`X-Oby-Words`), checked first: an unsigned or wrongly signed write is `403` and changes nothing, before its body or name is read.
- **What a PUT takes.** A body of exactly `{"value": "<string>"}`, at most `vault::LONGEST` plus 4096 bytes (a bigger one is `413`). A name the vault refuses, or one starting `oby.`, and a value it refuses (too short or long, a common word or a weak pattern) are `422` with obid's words, and nothing is saved. Otherwise the value is saved (replacing one of that name), with the whitespace around a paste dropped, and the answer is `204`.
- **What a DELETE takes.** No body. A name the vault refuses or one starting `oby.` is `422`, and nothing is removed. A removed secret is `204`; a name the vault doesn't hold is `404`.
- **Never echoed.** No answer, error, log line or event of these routes holds the value or the request's body; an error names the secret, never its value.

### Failure and recovery

- While the vault can't be read, every `/v1` request is `503` with the reason, these too, and nothing is saved (docs/api/vault.md, "No vault secret"). A store that fails after a write's checks is `503` with its reason, naming the secret only.
- A refused write changes nothing, so the owner sends it again corrected.

## Acceptance scenarios

- **VAULT-1.** A signed `PUT /v1/obid/vault/staging-key` with a strong value answers `204` with no body; `GET /v1/obid/vault` lists `staging-key` with no value and no `oby.` name; the next `/v1` answer that would hold the value shows `[vault: staging-key]`; a signed `DELETE` answers `204`, a second `404`, and the name is no longer listed. Test: `vault_1_a_put_value_is_redacted_at_once` (core/crates/obid/tests/vault_api.rs).
- **VAULT-2.** Signed PUTs of a weak value, a bad name, an `oby.` name, a body with another key and a value that isn't a string are each `422`, with an error that doesn't hold the value; a body past the cap is `413`; a signed DELETE of an `oby.` name is `422`; afterwards the names and the vault's file are unchanged. Test: `vault_2_a_refused_value_saves_nothing` (core/crates/obid/tests/vault_api.rs).
- **VAULT-3.** Through the gateway, with a paired device's token and the owner's signed words, `GET /v1/obid/vault`, `PUT /v1/obid/vault/x` and `DELETE /v1/obid/vault/x` are each `404`, and nothing is saved. Test: `vault_3_the_gateway_never_reaches_the_vault` (core/crates/obid/tests/gateway_api.rs).
- **VAULT-4.** While the event stream is open, a signed PUT, a GET, a refused PUT, an unsigned PUT, a DELETE and an answer that would carry the value leave the value in none of the answers, none of the event frames and not in obid's log. Test: `vault_4_the_value_is_in_no_log_event_or_answer` (core/crates/obid/tests/vault_api.rs).
- **VAULT-5.** An unsigned PUT, and a PUT signed for another body, are `403` and save nothing; an unsigned DELETE of a saved name is `403`, and the name is still listed. Test: `vault_5_an_unsigned_write_is_refused` (core/crates/obid/tests/vault_api.rs).
