# An Obysphere's policy, read and written from Oby

The business behavior contract for reading and writing an Obysphere's policy from the owner's own app, from [the template](../business-contract-template.md). Roadmap tasks R10-06 (the engine, `factory/queue/done/R10-06.md`) and R10-07 (the screen); what the Sphere offers was found by the spike R10-05 (below). Scenarios are numbered `POLICY-n`; the tests that prove them are named after them (`policy_1_…`).

## Feature and status

- **Outcome:** An admin of an Obysphere adds, from their own Oby, the rule that lets every member ask every other a question or a review, so a new Obysphere stops refusing every question (`policy_denied`). Any member's Oby can read the rules.
- **Status:** The engine is **implemented** in the Electron app (`apps/electron`): `readPolicy`, `writePolicy`, `starterRule` and `addStarterRule` in `src/shared/sphere.ts`, carried out by `src/main/sphere.ts` and reached through the preload (POLICY-1 to 4). Nothing shows them yet: the screen, its words and the offer on a `policy_denied` refusal are R10-07's, POLICY-5 to 7, still to come.
- **Scope:** reading the policy, and adding the members rule at the version the page read. **Out of scope:** the screen (R10-07); a page call that sends rules, edits or removes one (a later editor, #1220); anything in obid or in obi-sphere; caching the policy, the admin flag or a token; polling or pushing policy changes.

## Behavior

### Actors and ownership

- **Main** (the Electron app's main process) holds the owner key (`ownerKey()` in `src/main/sphere.ts`) and is the only place a token for the policy exists. For each call it makes the owner's own new 5-minute assertion, exchanges it at the Sphere for a short-lived token (`directory:read` to read, `admin directory:read` to write) and calls the Sphere with it. It decides what it signs and writes: the rules it reads again, with the members rule added.
- **The page** sends only the version it read (`window.app.sphereSetPolicy(version)`), right after the owner's click, and gets back a `PolicyAnswer`: `{ sphere, policy, admin, refused, why }`. It never gets a token or the key, and can't hand main rules, a scope or an address.
- **obid** holds nothing new: main reads its membership (`GET /v1/obid/sphere`: `url`, `sphere`, `owner`) and nothing else. It has no policy route and never holds an owner or admin token.
- **The Obysphere** says who may write (its `admins`, and the `admin` it grants or leaves out of an exchange) and keeps the policy's version.

### Triggers and prerequisites

- Read: the page calls `window.app.spherePolicy()`. Write: the page calls `window.app.sphereSetPolicy(version)` with the version it read.
- obid's membership is `joined`; else the answer is `not_joined`, and no owner key is used and nothing reaches a Sphere.

### States and transitions

| The page asks | What main does | `refused` |
|---|---|---|
| Read | A `directory:read` token; `GET /v0.1/spheres/{sphere}` and `GET …/policy` with it. `admin` is true only when the Sphere's `admins` lists this owner. | `null` |
| The members rule, with no click or key main saw itself just before (`takeInput` in `src/main/words.ts`, as Publish) | Nothing: no owner key used, nothing read or written. "Adding the rule takes your own click." Each click or key allows one write. | `failed` |
| The members rule, at the version read, right after a click | Reads again. A version other than the page's: nothing asked for `admin`, the policy as now. | `changed` |
| | The owner isn't in `admins`: nothing asked for `admin`. | `not_admin` |
| | A rule with id `members-ask` is there already: nothing written, the policy as read. | `null` |
| | Else a new assertion asking `admin directory:read`; with `admin` granted, one `PUT …/policy` with `{expected_version, rules}`: every rule as read, in order, then the members rule. The new policy, `admin` true. | `null` |
| A write whose exchange doesn't grant `admin`, or whose `PUT` is `403` | No `PUT` (or nothing replaced). "Only the Obysphere's admins can change its rules." | `not_admin` |
| A write answered `409` `conflict` | The policy read again with a new `directory:read` token, never the admin one. A version other than the one written at: nothing more written, the policy as now, "The rules changed meanwhile: …". The same version: the Sphere's `message` as `why`, never parsed, as one line of plain text (control and direction characters made spaces, at most 300 characters). | `changed` / `failed` |

The members rule is `{"id": "members-ask", "from_owner": "*", "to_owner": "*", "actions": ["question", "review_request"], "repos": ["*"]}` (ask-an-oby.md). These words are carried in `why` until R10-07's screen sets what a person reads.

### Failures and recovery

- obid not answering, the Sphere unreachable, an exchange that gives no token, an unexpected status or a policy Oby can't read: `refused` `failed`, with `why` in words; never thrown at the page. The page can ask again.
- A sender that isn't the window's own page, or a version that isn't a safe integer: `null` ([ipc.md](ipc.md)). Main opens no dialog for any of these answers.

### Invariants and boundaries

- The owner key never leaves main, and is used only inside these calls, through the `ownerKey` thunk. It signs a write's `admin` assertion only right after a click or key main saw reach the window itself (`before-mouse-event`, `before-input-event`), one write each: a script that took the page over can't add the rule on its own ([signed-words.md](signed-words.md)).
- `why` is data for the page to show as text only (no HTML or markdown), never given to an agent: the Sphere's own words in it are one plain line.
- Each write makes a new assertion (a new `jti`) and a new exchange. The admin token is sent on exactly one request, the `PUT`, and then dropped: not kept, logged or answered. No token or assertion appears in any `why` or answer.
- Every request to the Sphere uses `redirect: 'error'`: a redirect would carry an assertion or a token to another address.
- Nothing is kept between calls: no policy, admin flag or token in main.

## Interfaces and dependencies

- **Renderer ↔ main:** the preload's `window.app.spherePolicy()` (channel `sphere-policy`, no arguments) and `window.app.sphereSetPolicy(version: number)` (channel `sphere-set-policy`, decoder `safeInteger`), each answering a `PolicyAnswer` or `null` ([ipc.md](ipc.md)).
- **Main ↔ the Sphere:** `POST /v0.1/oauth/token` (RFC 8693, the owner's own assertion), `GET /v0.1/spheres/{sphere}`, `GET` and `PUT /v0.1/spheres/{sphere}/policy`, as found below.
- No obid route and no runner API change (`docs/runner-api.md` is unchanged).

## Acceptance scenarios

- **POLICY-1** Given an admin owner on a local obi-sphere with policy `[]` at version 0, when main adds the members rule at version 0, then the answer is version 1 with `refused` `null`, `GET …/policy` lists `members-ask`, and the next member question crosses instead of `policy_denied`; rules already there stay first and unchanged, and a `members-ask` already there writes nothing. `policy_1_the_starter_rule_lets_members_ask` in `apps/electron/scripts/test-sphere.mjs` (a real obi-sphere and two real obids, `npm run test:sphere`) and in `apps/electron/src/renderer/src/test/sphere.test.ts` (a fake Sphere).
- **POLICY-2** Given a member who isn't an admin, when the page asks to write, then no `PUT` is sent (an exchange that leaves `admin` out sends none, and the members rule for an owner not in `admins` never asks for `admin`) and the answer is `not_admin`, its words naming the admins; not joined answers `not_joined` with no owner key used and nothing sent. `policy_2_a_member_cannot_write` in `sphere.test.ts`.
- **POLICY-3** Given the policy changed since it was read, when main writes at the old version, then nothing is replaced, the policy is read again with a new `directory:read` token, and the answer is `changed` with the new policy and "changed meanwhile"; a `409 conflict` at an unchanged version answers `failed` with the Sphere's message. `policy_3_a_stale_version_replaces_nothing` in `test-sphere.mjs` (a real obi-sphere) and in `sphere.test.ts`.
- **POLICY-4** Given any policy write, when it runs, then every request has `redirect: 'error'`, each write makes its own exchange with a new `jti`, the admin token is on exactly one request (the `PUT`), no answer carries a token or an assertion, and the preload names no `ownerKey`, `assertion` or `access_token`; a write the page asks for with no click or key main saw itself just before is refused (`failed`, "Adding the rule takes your own click.") and writes nothing, and one click allows one write. `policy_4_the_owner_key_stays_in_main` in `sphere.test.ts`, `policy_4_the_members_rule_is_written_only_right_after_a_click_main_saw` in `apps/electron/src/renderer/src/test/ipc.test.ts`.
- **POLICY-5 to 7** (R10-07, to come): the screen that shows the rules, its button, and the offer on a `policy_denied` refusal.

### Test it when it's done (on a real Mac)

1. With R10-07's screen: an admin's Oby on a new Obysphere adds the members rule, and a member's question to another member arrives.

## What the Sphere offers

What obi-sphere (obi-ai-bot/obi-sphere, Dmytro's) lets a client do with a Sphere's policy, and how it answers. Each answer is marked:

- **read**: from obi-sphere's `spec/sphere-api.md`, `spec/identity.md` and the server's source at commit `d3a5e34ae1b5e44e89c1c4129b6653c73ac78090` (`d3a5e34`), the commit `core/crates/obi-protocol/spec/` was copied from (its `README.md`);
- **run**: called on 2026-11-14 against the local Sphere of `e2e/rig.mjs`, which runs obi-sphere at `586a69420652d1cdfe62dd7e43901bf161979046` (`586a694`, `e2e/rig-sphere.ref`), on `127.0.0.1:8791`.

**The two commits agree.** Between `d3a5e34` and `586a694`, `spec/sphere-api.md`, `spec/identity.md`, `spec/schemas/`, `server/src/oauth.ts`, `server/src/auth.ts`, `server/src/routes/policy.ts` and `server/src/routes/spheres.ts` are unchanged (`git diff d3a5e34 586a694 --` on them prints nothing). The server's only changes are its client list (`server/src/clients.ts`, which now names Oby), its dependencies and one test: nothing about the policy, admins or scopes.

**How the run was made, to repeat it.** `OBY_RIG_DIR=<a scratch folder> node e2e/rig.mjs start --stand-in` (with `OBID_BIN` set to an obid already built), so the Sphere `rig` has its admin `rig-admin` and two members, `alex` and `dana`, who joined; the rig writes its rule, `alex-asks-dana` and `dana-asks-alex`, at version 1. Then a one-off Node script outside the checkout read `spherePort`, `adminKey`, `ownerKeys.you` (alex) and `ownerKeys.dana` from the rig's `rig.json`, made each owner's own assertion with `assertion(key, owner, "rig", scope)` from `apps/electron/src/shared/sphere.ts` (no `may_act`: an owner token, no actor), exchanged it at `POST /v0.1/oauth/token` (form: `grant_type=urn:ietf:params:oauth:grant-type:token-exchange`, `subject_token_type=urn:ietf:params:oauth:token-type:jwt`, `subject_token=<the assertion>`), and called the Sphere with `Authorization: Bearer <the token>`. It printed each status and body with the token taken out. Last, `node e2e/rig.mjs stop`.

**The run, in order.** Every run answer below comes from one rig, started once, in these steps; each table row names its step, and the policy's version follows from them:

1. `node e2e/rig.mjs start --stand-in`: the rig writes its two rules, version 1.
2. The reads (question 1), `GET /v0.1/spheres/rig` and the exchanges for `admin` (question 3): version 1.
3. alex's `PUT`s with the token alex can get (question 2): refused, version 1.
4. rig-admin's `PUT` at `expected_version` 1, the same rules (question 2): version 2.
5. rig-admin's `PUT`s at `expected_version` 1 and 999, then a `GET` (question 4): refused, version 2.
6. rig-admin makes alex an admin and takes it away again (question 3): version 2.
7. `node e2e/rig.mjs policy on`, which writes the rig's rules again: version 3; then rig-admin's `PUT` at `expected_version` 2 (question 4): refused, version 3.
8. rig-admin's `PUT` at `expected_version` 3 with a rule for an owner who never joined (question 4): refused, version 3; then its `PUT` at `expected_version` 3 with the members rule (question 2): version 4.

### 1. Read: `GET /v0.1/spheres/{sphere}/policy`

- **read** (`sphere-api.md` section 2, the endpoint table, and section 5; `server/src/routes/policy.ts`, `getPolicy`): any bearer token for the Sphere with `directory:read` or `admin` reads it, an owner's own token or an Obi's alike; it answers `200` with the policy, `version`, `updated_at` and `rules` (`policy.schema.json`). A new Sphere's policy is version 0 with no rules (section 3).
- **run**, step 2, each by an owner's own token:

| Who | Scope asked for | Token's `scope` | `GET /v0.1/spheres/rig/policy` |
|---|---|---|---|
| no token | | | `401`, `error.code` `unauthorized` |
| `rig-admin` | `admin directory:read audit:read` | `admin directory:read audit:read` | `200` `{version, updated_at, rules}`: version 1, the rig's two rules |
| `alex` | `directory:read` (what the app's owner exchange asks for today; the Obi's delegation, `SCOPES`, is another token) | `directory:read` | `200`, the same policy |
| `dana` | `directory:read` | `directory:read` | `200`, the same policy |
| `alex` | `audit:read` | `audit:read` | `403`, `error.code` `insufficient_scope` ("this call needs the scope directory:read or admin") |

So a plain member reads the policy with the `directory:read` token the app already knows how to get: R10-07 needs no new scope to show the rules to a member.

### 2. Write: `PUT /v0.1/spheres/{sphere}/policy`

- **read** (`sphere-api.md` section 2 and section 5; `identity.md` sections 4.2 and 4.3; `server/src/routes/policy.ts`, `putPolicy`): it needs an owner's own token (no actor) with `admin`. An Obi's token never holds `admin` and gets `403 insufficient_scope`; an owner's token without `admin` gets the same. The body is `policy-put.request`, `{expected_version, rules}`, and replaces the whole policy; the answer is `200` with the new policy, its `version` one more. A rule naming an owner other than `*` needs that owner to have joined, and a rule using an addition (`introduction`, class rules) needs every active member to take it (`409 feature_not_taken`).
- **The exchange** (read, `identity.md` section 4.3): `POST /v0.1/oauth/token` (RFC 8693) with the owner's own assertion as `subject_token`, no `actor_token`, and the scopes it grants in the assertion's `scope` (an optional form `scope` narrows them). The Sphere grants what was asked, intersected with what the caller may hold, and says which in the answer's `scope`, in a fixed order; an owner's own assertion lives at most 300 seconds and is used once, and the token lives no longer than the assertion (`expires_in` 300 or less). Errors there are OAuth's (`{error, error_description}`), not `error.schema`.
- **run**:

| Who | Call | Answer |
|---|---|---|
| `alex` (step 3) | `PUT /v0.1/spheres/rig/policy` with the token alex can get (`directory:read`), `expected_version` 1, the same rules or `[]` | `403`, `error.code` `insufficient_scope` ("this call needs the scope admin") |
| `rig-admin` (step 4) | `POST /v0.1/oauth/token`, assertion scope `admin directory:read audit:read` | `200`, `scope` `admin directory:read audit:read`, `token_type` `Bearer`, `expires_in` 300 |
| `rig-admin` (step 4) | `PUT /v0.1/spheres/rig/policy` with that token, `expected_version` 1, the same rules | `200` `{version, updated_at, rules}`, version 2 |
| `rig-admin` (step 8) | the same `PUT` with the rule ask-an-oby.md names, `members-ask` (`*` to `*`, `question` and `review_request`, all repositories), at `expected_version` 3, the version step 7's `policy on` made | `200`, version 4, the one rule |

One token served several calls in a row within its 300 seconds; a client may also exchange afresh for each write.

### 3. Admin signal: how a client learns its owner is the Sphere's admin

Two signals, both from the Sphere, and they agree:

- **The Sphere's `admins`.** Read (`sphere-api.md` section 2, `GET /v0.1/spheres/{sphere}` with any scope, and section 3, "Its admins"): `admin_owner` is the owner who made the Sphere, and `admins` lists every owner who administers it now, sorted. Only `admins` says who may write: `admin_owner` stays the maker even after an admin is added or the maker's own admin is taken away. Run, step 2: `GET /v0.1/spheres/rig` with alex's `directory:read` token and with rig-admin's token both answer `200` with `admin_owner` `rig-admin` and `admins` `["rig-admin"]`. So a member learns it with the token it already gets, before asking for `admin`.
- **The exchange's `scope`.** Read (`identity.md` section 4.3, check 6; `server/src/oauth.ts`: an owner's own token may hold `admin` only when the owner administers the Sphere): `admin` is granted or silently left out, never an error, unless nothing asked for can be granted. Run, step 2:

| Who | Scope asked for | Answer |
|---|---|---|
| `alex` (not an admin) | `admin` | `400`, `{error: invalid_scope}` ("no scope asked for can be granted") |
| `alex` | `admin directory:read` | `200`, `scope` `directory:read`: `admin` left out |
| `dana` (not an admin) | `admin directory:read audit:read` | `200`, `scope` `directory:read audit:read` |
| `alex` | assertion scope `directory:read`, form `scope` `admin` | `400`, `{error: invalid_scope}` |

  With the token it can get, a non-admin's `PUT` is `403 insufficient_scope` (question 2).
- **It changes at once.** Run, step 6: rig-admin's `PUT /v0.1/spheres/rig/admins/alex` answered `200` with `admins` `["alex", "rig-admin"]`; alex's exchange for `admin directory:read` then got `scope` `admin directory:read`. After rig-admin's `DELETE /v0.1/spheres/rig/admins/alex` (`200`, `admins` `["rig-admin"]`), the token alex got while admin answered `GET …/policy` with `401 unauthorized`, and a new exchange for `admin directory:read` got `directory:read` only. Read: `sphere-api.md` section 3 says the same ("their tokens holding `admin` stop at once").

### 4. Conflict: a `PUT` with a stale `expected_version`

- **read** (`sphere-api.md` section 1, the codes, and section 5; `server/src/routes/policy.ts`): the version must be the current one, or `409 conflict` and nothing is written. The body is `error.schema`: `error.code` `conflict` and a `message` for people; it carries no policy and no version field, so the client reads the current policy again with `GET`. The server's `message` names the current version in words, which a client may show but doesn't parse (`message` is data).
- **run**:

| Call, as `rig-admin` with an `admin` token | Answer |
|---|---|
| Step 5: `PUT` with `expected_version` 1, right after step 4's write made version 2 | `409` `{error: {code: "conflict", message: "the policy is at version 2"}}` |
| Step 5: `PUT` with `expected_version` 999, never issued | the same: `409`, `conflict`, "the policy is at version 2" |
| Step 5: `GET` after its two conflicts | `200`, version 2, the policy unchanged |
| Step 7: `node e2e/rig.mjs policy on` (version 2 to 3), then `PUT` with `expected_version` 2 | `409`, `conflict`, "the policy is at version 3" |

  A stale version and one never issued answer the same.
- **The same code for another reason.** Run, step 8: a `PUT` at `expected_version` 3, the current one, with a rule `from_owner` `nobody`, who never joined, answered `409` with `error.code` `conflict` too ("the policy names nobody, who hasn't joined this Sphere: it can name only owners who have, or *"). Read: section 5 says so. So `conflict` alone doesn't mean the policy changed meanwhile: a client that wrote rules naming owners compares the version it gets from a new `GET` with the one it sent.

### Where the client starts

- The Electron app's main process exchanges the owner's own assertion for `directory:read` to leave (`leave()`) and to read the policy (`readPolicy()`), and for `admin directory:read` to write it (`writePolicy()`, sending its `PUT` only when the Sphere granted `admin`), all in `apps/electron/src/shared/sphere.ts` (POLICY-1 to 4, above). obid asks for no owner token and calls no policy route.
- The owner's key lives in the Electron main process (`ownerKey()` in `apps/electron/src/main/sphere.ts`), never in obid or the page; the page sends only the version it read.
- obid holds no owner token and has no policy route. Its Obi's delegation, a different token from the owner's own, grants `directory:read` among others, `audit:read` too (`SCOPES` in `shared/sphere.ts`), so by `getPolicy` (read, not run) obid's own Obi token could read the policy, but never write it.

### For Dmytro

- Nothing. Everything R10-06 and R10-07 need is there: a member reads the policy with `directory:read`, `admins` and the exchange's `scope` say who may write, an admin writes with `admin` and `expected_version`, and a stale version is `409 conflict`, read again with `GET`.
