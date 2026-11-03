# Continue with ChatGPT

The business behavior contract for connecting a person's ChatGPT plan to Oby, from [the template](../business-contract-template.md). Scenarios are numbered `CHATGPT-n`; the tests that prove them are named after them (`chatgpt_1_…` in `core/crates/obid/tests/chatgpt_api.rs` and `core/crates/obid/src/chatgpt.rs`, `CHATGPT-…` in `apps/electron`).

OpenAI's words below were re-read from the pages themselves on 2026-11-03 (https://developers.openai.com/siwc/token-sharing-open-source and its siblings, and https://developers.openai.com/siwc/ui-ux-guidelines).

## Feature and status

- **Sangam's screen:** `SetupCard.tsx` (first run), `SettingsView.tsx` Agents, `Composer.tsx` usage panel, the limit card in `live.ts`. His ChatGPT row is at `Connectors.tsx:55`.
- **Outcome:** someone with no Codex login connects their ChatGPT plan in the browser with one click and no code to type, and Codex jobs run on it. They can add several ChatGPT accounts.
- **Status:** **Implemented against a stand-in OpenAI, not yet proven with a real sign-in.** The owner's decisions of 2026-11-02: obid may hold ChatGPT tokens, for ChatGPT only; conversations on any connected agent, yes; offer ChatGPT accounts at limits, yes; OpenAI's required words, no objection; unattended jobs, yes. **It must not ship in a release until the rule below is in `AGENTS.md`**, which needs the owner's click (MERGING-13), and until "One real sign-in" below has run.
- **Scope:**
  - In: Codex accounts with their own folder, on the Electron app.
  - Phones: display only (their screens are later work; CHATGPT-18 proves the new keys decode).
  - Out: Claude Code (its own login, as Anthropic's rules require), Cursor (`cursor-agent login` already works), API-key billing, a usage meter, self-hosted VMs, an idle refresh sweep.

## Behavior

### Actors and ownership

- **obid** is the OAuth client (crate::chatgpt): the loopback listener, the code's exchange, the id token's check, storage, refresh, revocation, the relay (crate::chatgpt_relay) and the model list.
- **Apps** open the URL and show state. They never touch a token.
- **Phones** never start a sign-in; those routes stay off the gateway.

### Triggers and prerequisites

- The person clicks Continue with ChatGPT on a Codex account with a folder.
- The Codex CLI must be installed, because jobs run on it (`notInstalled` stays).
- A login already on the Mac is still adopted with no click, so the button shows only when there is none.
- A `409 ownLogin` on `with=chatgpt` makes the app add a new account and sign that one in: `default_config` seeds a `codex` account with no folder, and the app's own sign-in picks it first.
- The device code stays as a secondary action.

### States and transitions

`not connected → signing in → connected`, or `→ connected, plan use off` if the scope is missing.
`connected → sign in needed` on a terminal refresh error, or once the refresh token is 30 days old.

- `signedIn` depends on the refresh token and a marker, never on the access token's expiry.
- Switching kinds removes the old login only after the new one succeeds. Nobody moves without the owner's click: a ChatGPT account is never a conversation's automatic failover, and a turn it took doesn't fail over off it either. Offered at a limit (#1563), it moves only on the owner's click.

### Words

OpenAI's, from its UI/UX page, which reads as guidance with examples, not MUST and SHOULD:

| Where | Words | How the page puts it |
|---|---|---|
| The action that starts the sign-in | **Continue with ChatGPT**, with its logo | "Use Continue with ChatGPT when the action starts the sign-in flow" |
| Near the composer or model selector | **Using ChatGPT plan**, with a **Manage usage** link | "display Using ChatGPT plan near the composer or model selector" |
| A usage page | **Manage usage**, opening ChatGPT usage settings | "place a Manage usage link near the usage summary" |
| First sign-in only | A welcome, "such as" **You're using your ChatGPT plan**, *Eligible usage in this app uses your ChatGPT plan. Manage usage in your ChatGPT settings.*, **Got it** | "Show a welcome modal only the first time" |
| A usage limit | **Usage limit reached**, *Review your plan or this app's limit in ChatGPT settings.*, **Manage usage** primary | "Make Manage usage the primary action" |

The page names no address for "ChatGPT usage settings"; `https://chatgpt.com/settings/usage` is what this contract's first test used and is to be confirmed by the real sign-in. Every other word and all placement is Sangam's.

### Failures and recovery

| Case | Result |
|---|---|
| Consent denied; id token signature, `iss`, `aud`, `exp` or `nonce` fails; callback `client_id` differs | No tokens kept, any issued are revoked. `dynamic_agent_client` is never saved. The row says why within one poll. |
| A callback naming another `Host` or `state`, or none | Not from this sign-in: refused with a page, and the sign-in goes on, so a page can't end it. |
| Returning sign-in with a different `sub`, or an issued `client_id` already on another account | Nothing replaced. The same `sub` under a new client is a separate account: one person with Personal and Team is two valid registrations. |
| Scope `chatgpt.tokens.use.direct` missing | Sign-in kept, plan use off, no run starts on it. Choosing again repeats OAuth with `prompt=consent`, which OpenAI's page still supports; never on an ordinary sign-in. |
| `invalid_grant` on the code's exchange | The code is discarded; the next try reuses the issued client. |
| 401, token near its end (within 10 minutes) | The relay refreshes and sends the same request again. Codex never sees it, no process restarts, no `RETRY_WAITS` spent. |
| 401 on a token at least 10 minutes from expiry, or a second 401 straight after a refresh | The job ends with the request id. No sign-in prompt unless a refresh failed terminally. |
| `invalid_grant`, `invalid_refresh_token`, `token_expired`, `refresh_token_expired`, `refresh_token_invalidated`, `refresh_token_reused` | Tokens cleared, record kept so the client id survives. Sign in again. On `refresh_token_reused`, the vault is read again first and cleared only if it still holds that token. |
| `invalid_client` | Reported as Oby's configuration, no sign-in prompt. |
| 429 `subscription_sharing_usage_limit_exceeded`, before or after the stream began | #1563's flow unchanged, with no reset time. Resume follows LIMITS-1 and says no time. An inferred hold is never a decision's `resetAt`. |
| 403 `subscription_sharing_user_not_eligible`, `chatpass_v2_*`, `subscription_sharing_route_not_supported`, `{"detail":…}`; 400 `subscription_sharing_unsupported_capability`; 401 `subscription_sharing_invalid_user` | Stated with the request id, no retry, no OAuth loop, no billing switch. |
| 503, `subscription_sharing_usage_unavailable`, `subscription_sharing_user_unavailable`, or a network failure | Credentials kept. Codex's own retries (30 requests in about 24 seconds), then the job's bounded `RETRY_WAITS`. |
| Revoke | Only a 200 counts. A network failure or 5xx gets at most three tries within 10 seconds, then removal completes with `revoked:false`. |
| A token set obid won't keep (mismatch, account removed mid-exchange or mid-refresh) | Revoked best effort, then dropped. |
| A refresh that fails for now (a network failure, a 5xx, a 429, anything unlisted) | No other POST until its wait ends: `Retry-After` when it says, else 30 seconds doubling to 5 minutes. A token that still works goes on being used. |
| A refresh answer of 200 that isn't whole (a bad `expires_in`, no access token) | Its rotated refresh token is kept anyway, and the next use refreshes with it. |
| A refresh in flight when a new sign-in or a device login takes the account | It writes nothing. Over a new sign-in it's only dropped, since revoking might end that session too (S8); over a device login it's revoked. A terminal answer about a token already replaced ends nothing. |
| `accounts.json` doesn't read at start, an entry its checks leave out, a seeded account taken out of `config.json` | No login is revoked or deleted: mending the file brings them back. Only an entry whose account the owner removed is taken back at start. |

**Refresh tries not to lose a rotated token, and can't always.** Each account's refresh runs in one spawned task callers wait on, so stopping a job can't cancel it. A new token is held for the scrub the moment it comes, and written at once. If the write fails, the record stays in memory and is retried, with no other refresh meanwhile, and runs use the token it holds. The answer itself can be lost: after a timeout or a dropped connection the same refresh token is sent again. If OpenAI hadn't rotated it, that works. If it had, the answer is `refresh_token_reused`, the login is over and the person signs in again, since OpenAI's pages describe no grace period. A crash between the answer and the write ends the same way.

### Invariants and boundaries

- **The access, refresh and id tokens never leave obid's process, except to OpenAI.** Codex's `base_url` is obid's loopback relay and its `env_key` holds the job's broker token, which dies with the job. The relay takes only `POST …/responses` and `GET …/models`, adds the current access token and forwards to `https://api.openai.com/v1`.
  - Why (spike S0, measured 2026-11-03 under obid's real profile with real `codex` 0.162.1): a process in another job's box read a marker from Codex's environment with `KERN_PROCARGS2`, by pid. So a token in Codex's environment could be read by any box.
  - The broker token itself can be read that way, like `OBY_MCP_TOKEN` today, and calls the relay while the job lives. What that reaches is model calls on the plan, never a token, and it dies with the job.
  - This differs from OpenAI's setup, which passes the token in the child's environment. What reaches OpenAI is the same request. Its pages don't address a relay.
- Every OAuth call uses `redirect::Policy::none()` and fixed OpenAI endpoints. Only a debug build's tests can name another origin, on 127.0.0.1, from the environment, never from `config.json`.
- The id token isn't kept after verification. OpenAI suggests keeping it for `id_token_hint`, which only skips the account selector; `login_hint` carries the email instead.
- Every token is scrubbed from every answer, log line and push, by `Vault` learning the record's tokens as forms of one name (`Secrets::learn_forms`), including replaced access tokens until they expire, and again after a restart.
- `agent` stays `"codex"`; no new enum value. Billing never switches silently. Nothing leaves the Mac except to OpenAI and, sealed, to the owner's paired devices.

## Interfaces and dependencies

All add-only.

- `GET /v1/accounts/{id}/sign-in?with=chatgpt`: same answer shape, no code. A plain GET stays device auth. A sign-in of the other kind under way answers `busy`.
- `AccountJson` gains optional `login:"chatgpt"` and `loginEmail`. `sub` is the identity, never the email. It also gains `signingIn:true` while a sign-in is under way, since a `label` can be there already (plan use off, too old).
- `DELETE /v1/accounts/{id}` stays 204, and answers 200 `{"revoked":false}` when revocation wasn't confirmed.
- No new reason word. Callback failures use `label` with `signedIn:false`, and the app's wait ends when the tried account stops saying `signingIn`.
- `GET` and `PUT /v1/settings/chatgpt` `{"welcomed": bool}`, off the gateway, for the welcome shown once.
- `POST /v1/agent/chatgpt/{job}/responses`, `GET /v1/agent/chatgpt/{job}/models`: the relay, with the job's broker or a conversation turn's token only.
- **Storage:** one vault entry per account, `oby.chatgpt.<id>`, holding access and refresh tokens, scopes, expiry, `earliest_refresh_at`, client id, `sub`, email, host id and replaced access tokens. A non-secret marker in the account's folder (`chatgpt.json`) holds email, whether it's signed in, plan use, expiry and refresh age; `has_login`, `signed_in`, `login_problem` and `login_note` branch on it. One `ext_agent_host_id`, `urn:uuid:<v4>`, per Mac (`home/chatgpt-host`). Removal: revoke, delete the entry, `retire`. At start, obid revokes and deletes an entry with no account only when the owner removed that account; absence alone proves nothing.
- **Models:** the picker stays static. At launch obid checks the chosen model against the account's cached `/v1/models`. A null, absent or unlisted model takes the first `visibility=="list"` slug, and the job's own `model` and its transcript say so.
- **Usage:** none exists. Accounts show "No use reported yet" and route as unknown. obid skips the periodic read and runs with `usage:false`, `priced:"plan"`.
- **Codex:** `-c model_provider="openai_chatgpt_plan"` with `base_url` the relay, `env_key="OBY_MCP_TOKEN"`, `wire_api="responses"`, `requires_openai_auth=false`, `supports_websockets=false`, `cli_auth_credentials_store="ephemeral"`, and `features.tool_search=false`, since OpenAI's page says configurations that emit Responses `tool_search` fail. `clientInfo.name` is `Oby`, matching `agent_name_hint`.

## Acceptance scenarios

- **CHATGPT-1:** Given no Codex login, with and without the seeded `codex` account, when the person clicks and consents, then `signedIn` is true, there is no `auth.json`, and tokens are only in the vault.
- **CHATGPT-2:** Given a Codex login on the Mac, then it's adopted with no click and no ChatGPT prompt.
- **CHATGPT-3:** A plain GET still runs `codex login --device-auth`, and a device-login account behaves as today.
- **CHATGPT-4:** A callback with a wrong `Host`, `state`, signature, `aud` or `client_id`, or a second callback, is refused with no tokens, and a failure that ends the sign-in shows on the row within one poll.
- **CHATGPT-5:** A returning sign-in with a different `sub`, or a client already on another account, replaces nothing.
- **CHATGPT-6:** Scope missing keeps the sign-in, with plan use off and no run.
- **CHATGPT-7:** Access token expired, refresh token valid: `signedIn` stays true.
- **CHATGPT-8:** Two runs at once with an expiring token cause one refresh.
- **CHATGPT-9:** A terminal refresh error clears tokens, asks for sign-in, and keeps the client id.
- **CHATGPT-10:** While a job runs, no process of the owner's user finds an OpenAI token in any pid's environment, or in `CODEX_HOME`. (Measured from outside the boxes: S0 showed a box reads what an unboxed reader does.)
- **CHATGPT-11:** Every token generation is scrubbed from a `/v1` answer, `for_agent` and the scan, each checked again after a restart.
- **CHATGPT-12:** A job outlives its token and finishes on the same thread, `RETRY_WAITS` unspent.
- **CHATGPT-13:** A usage limit shows the #1563 card with no reset time, Manage usage first, and an inferred hold is never `resetAt`.
- **CHATGPT-14:** A 503 keeps credentials and backs off; a 403 not-eligible doesn't retry; a repeated 401 ends the job.
- **CHATGPT-15:** Removal revokes; a failed revoke completes with `revoked:false`; an exchange or refresh in flight writes nothing afterwards and revokes.
- **CHATGPT-16:** Switching kinds removes the old login only after success.
- **CHATGPT-17:** The welcome shows once, even with `localStorage` emptied.
- **CHATGPT-18:** Older phones decode the new keys, with no new enum value.
- **CHATGPT-19:** Stopping a job mid-refresh and a failed vault write leave the account signed in. So does a lost answer OpenAI hadn't acted on; one it had comes back `refresh_token_reused` and ends in sign in needed.

## Spikes

Measured on 2026-11-03 with `codex` 0.162.1 and no account:

- **S0: passed.** Real `codex` in obid's real box, with a broker token in `env_key`, reached a loopback relay, which added the real token for a fake upstream: a whole turn completed. The relay saw only the broker token, the upstream only the real one. Codex asked only `POST /responses`. Inside another job's box under the full profile, `sysctl(KERN_PROCARGS2, pid)` read a marker from Codex's environment, and `/bin/ps -E` was refused.
- **S5: Codex drops the code; the relay doesn't need it.** Codex's error for a 429 is only `exceeded retry limit, last status: 429 Too Many Requests`, so `limits.rs` couldn't tell a usage limit from a rate limit. The relay reads the status and body before Codex does, so the code decides, and Codex's words don't. Requests per failing turn: 401, 403 and a mid-stream failure, 6 in about 6 seconds; 503 or 500, 30 in about 24 seconds; 400 or 429, 1. A 400's body becomes the turn's error word for word, so the relay answers 400 with its own sentence for what must not be retried.
- **S6: passed.** `danger-full-access` with `approvalPolicy: "untrusted"`: a shell command came as `item/commandExecution/requestApproval`, accepted, and ran in the box.

## One real sign-in

What only a real account can prove, for the owner's first sign-in. None of these outcomes is assumed:

- **S1:** a job past 60 minutes: the relay renews it, and `earliest_refresh_at`'s real values don't get in the way. *If it fails:* add a refresh before the hour.
- **S2:** the `oby` and `tetrix` MCP tools with `tool_search` off. *Safe default now:* off, as OpenAI's page requires. *If it fails:* stop; the feature waits.
- **S3:** helper agents. *Default now:* as Codex offers them; a helper's first word shows as a tool call. *If it fails:* turn them off for these accounts.
- **S4:** real-token `account/rateLimits/read`. *Default now:* never asked. *If it answers:* usage can show later.
- **S7:** a second account, re-adding one (does `dynamic_agent_client` return the same client?), a conversation turn. *If it fails:* conversations skip these accounts.
- **S8:** does revoking an old refresh token after a same-`sub` sign-in end the new session? *Safe default now:* a replaced refresh token isn't revoked; it runs out by itself. *If it doesn't:* revoke it.
- **S9:** the other way round: a same-`sub` sign-in that fails after its code is spent (say, an id token that doesn't check out) revokes the tokens it was just given. Does that end the session the account already had? *Default now:* it revokes. *If it does:* a failed same-`sub` sign-in drops its tokens instead of revoking them.

## Test it when it's done

1. A fresh account through first run, with Codex installed and no login.
2. A job over 70 minutes that uses Oby's MCP tools.
3. Remove the account and see it gone in ChatGPT settings.
4. An existing device-login account unchanged after upgrade.

## The rule for `AGENTS.md`

`AGENTS.md` is a security path, so this waits for two reviews by different tools or the owner's click (MERGING-13). Add after the logins rule:

> ChatGPT is the one provider obid signs in to itself (the owner's decision of 2026-11-02, for ChatGPT only): a Codex account signed in with "Continue with ChatGPT", OpenAI's Sign in with ChatGPT for open-source and local apps, has obid as the OAuth client (PKCE, a loopback redirect on 127.0.0.1), its tokens in the vault as `oby.chatgpt.<account>` (0600), taken out of every answer, log line and push. No run gets any of its tokens: Codex reaches OpenAI through obid's relay with the job's broker token, which dies with the job. Claude stays on Claude Code's own login, as Anthropic's rules require, and Cursor is unchanged. An account signed in with `codex login` works as below, and none moves without the owner's click.

Also change: "Every Codex run gets `auth.json`" → "Every Codex run on a `codex login` gets `auth.json`"; "Oby never reads, stores or forwards the user's agent logins" gains this exception.
