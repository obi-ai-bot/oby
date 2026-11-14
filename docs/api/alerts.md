### `POST /v1/devices`

Registers a device's publishing key, once per device: the iPhone's (named "iPhone") or a Mac's (named after the Mac, `Host.current().localizedName`):

```json
{"name": "Wellington's MacBook Air", "publicKey": "BJ3x…"}
```

`publicKey` is the P-256 public key, X9.63 (65 bytes, uncompressed), base64. It returns `{"id": "d7c1…"}`. Only the runner's owner can register (the tailnet header, or the API token); the runner lists devices in its log.

- A key the runner knows returns its id right away, so the apps register every time they connect. That adds nothing.
- A new key goes in only with the owner's presence, in one of three ways (otherwise `403` with one line to show):
  - **Vouched for** by a device already registered: `"endorsement": {"device": "<its id>", "signedAt": "…", "signature": "…"}`, ECDSA P-256 with SHA-256 (DER, base64) over `"oby-device\n<publicKey>\n<signedAt>"`, within 2 minutes, once (kept used in `devices.json`, as approvals are). The apps sign only on their owner's tap or click.
  - **A paired phone's own key, from an invitation the owner made**: a request through the gateway (or the Obysphere relay) from a phone paired with an invitation the owner made on their own device (`POST /v1/pairing/invite` with its `approval`, below) registers one key, bound to that pairing, within 10 minutes of redeeming it. A pairing from an invitation the API token made alone, or one older than that, adds no key (`403`: `To add this iPhone's publishing key, pair it again from Oby on your Mac, then open Oby on the iPhone within 10 minutes.`), so the API token alone (an MCP client, a leaked token) can't invite, redeem and register a key that approves. A second new key from the same pairing is refused (`This iPhone registered its publishing key already. To register another, pair it again from your Mac.`), and revoking the pairing (`DELETE /v1/pairing/devices/{id}`) forgets its key.
  - **Named in `home/pairing`**: the file holds exactly this `publicKey` (whitespace around it is ignored) and was written in the last 2 minutes. The registration that it names uses it up at once; one older than 2 minutes is removed when it's looked at, and a file naming another key admits nothing. A request through the gateway never uses it. The Mac app writes it for the runner on the same Mac only on its owner's click (the first Publish, Confirm or Disconnect that runner doesn't know this Mac's key for, or Settings' Try again), and removes it right after, whatever happened; it never writes it when it merely connects. For a runner on another machine the `403` says the command for its owner to run there: ``To add this device, run `echo '<publicKey>' > <home>/pairing` on obid's machine, then try again within 2 minutes.`` An empty file (`touch`), as pairing was opened until 2026-10-24, admits nothing.
- A Mac whose new key went in this way revokes the ones it replaced (`DELETE /v1/devices/{id}`, signed by the new key): its Touch ID-only key (until 2026-10-23) and its Touch ID or password key (until 2026-10-24).
- Why: agents run on the runner's machine, so through its tailnet address they arrive with the owner's header, and a token can leak; with the API token alone, or a paired phone's token, nothing can add a key that signs a publish, a Confirm or a connection change. Agents can't write `pairing`, because `OBY_HOME` is hidden inside their sandbox. A program that can write obid's home can rewrite `devices.json` too: that is outside what this guards.

### `DELETE /v1/devices/{id}`

Forgets a device's publishing key, with a registered key's signature for it (its own, or another device's):

```json
{"approval": {"device": "d7c1…", "signedAt": "2026-10-24T12:00:00Z", "signature": "MEUC…"}}
```

- `signature`: ECDSA P-256 with SHA-256 (DER, base64) over `"oby-revoke-device\n<id>\n<signedAt>"`, within 2 minutes, once.
- It returns `{"revoked": "<id>"}`; `404` for a device it doesn't know, `403` for an approval that isn't a registered key's for this device. Not on the gateway.

### `POST /v1/notifications`

Hands the runner the app's device token, so it can tell the phone when a job needs you:

```json
{"token": "5b0e9c…", "topic": "com.deskree.oby", "environment": "production"}
```

- `token`: the device token from `registerForRemoteNotifications`, in hex. `topic`: the app's bundle ID. `environment`: `"development"` (a build from Xcode) or `"production"` (TestFlight and the App Store). The app sends it on every launch once notifications are allowed (it asks the first time something said on the phone starts a job).
- It returns `{"push": true}` when the runner pushes, `{"push": false}` when it has no APNs key (the token isn't kept). `422` for a token, topic or environment that isn't one of those.

Two seconds after a job changes, the runner sends an alert push (`apns-push-type: alert`, the app's bundle ID as topic, priority 10) for each job whose need changed. Nothing else notifies: steps, rounds, and jobs that end needing nothing (stopped, or done without commits) only move the Live Activity.

| The job | Title | Body | `category` |
|---|---|---|---|
| Done with commits it couldn't publish by itself | `Couldn't publish: <title>` | `publishError` (else the summary's first line) | `ready` |
| `failed` | `Failed: <title>` | the reason (the summary's first line) | `job` |
| `interrupted` | `Interrupted: <title>` | the reason | `job` |
| Oby's review stopped asking for changes | `Changes requested: <title>` | `Oby's review found a blocker it asked to fix before. Take it from here.` | `job` |
| Its agent asked a question (`question`), which waits in the inbox | `Question: <title>` | the question | `job` |
| Its agent waits for the user's OK (`permission`), which waits in the inbox | `<permission title>?`, like `Run swift test?` | `<title>: <detail>` | `permission` |

```json
{"aps": {"alert": {"title": "Couldn't publish: Review oby #33", "body": "Couldn't push oby/review-33: the remote hung up."},
         "category": "ready", "sound": "default", "thread-id": "jobs"}, "job": "9f08fd10"}
```

- `job` is the job's `id`: tapping the notification opens it. The `ready` category offers **Publish** (it unlocks the phone and brings the app forward on the job, where it publishes) and **Open**; `job` offers **Open**; `permission` is for **Allow**, **Always allow** and **Deny** (`POST /v1/jobs/{id}/permission` with the job's `permission.id`) and **Open**. On obid, the agent's question (#858) alerts as `job`: its options are its own, so it's answered in the app.
- `apns-collapse-id` is the job's `id`, so a newer state replaces the older notification.
- When a job stops needing you (published, from the iPhone or the Mac, dismissed, or running again after a message), the runner sends a background push (`apns-push-type: background`, priority 5), `{"aps": {"content-available": 1}, "clear": "<job id>"}`, and the app removes that job's notification. The app also removes them whenever it opens and sees the job needs nothing.
- The runner remembers what it last said for each job in `apns/notices.json`, so a restart doesn't say it again, and jobs a restart interrupted notify once it's back.

**On obid** (#452, partly ported: Firebase Cloud Messaging for the Android app, and the iPhone through Deskree's sealed relay, below; no APNs key of its own). The above is the Swift runner's.

What alerts, on the Mac and the phones, and the one list both alert from ([`GET /v1/alerts`](#get-v1alerts), below): [alerts.md](contracts/alerts.md), ALERTS-1 to 8.

```json
{"token": "<FCM registration token>", "platform": "fcm", "topic": "com.deskree.oby"}
```

- `platform`: `"fcm"` for an Android phone's FCM registration token; absent or `"apns"` is the iPhone's bare device token as above, which `obid` answers `{"push": false}` and doesn't keep (it has no APNs key: the iPhone registers through Deskree's relay instead, below). `token`: 1 to 4,095 printable ASCII characters, no spaces. `topic`: `"com.deskree.oby"` or left out, for FCM; an APNs registration's `topic` and `environment` aren't looked at.
- It returns `{"push": true}` when the token is kept and `obid` can send (a Firebase service account is in the vault, below), `{"push": false}` when it can't (nothing kept), `422` for a token, platform or topic that isn't one of those, and `503` when the vault can't be read, as every answer.
- Tokens are kept in `home/push/fcm.json` (0600, written to a new file renamed into place) as `[{"token", "device"}]`, at most 20, the oldest out; a token handed over again becomes the newest. `device` is the paired device whose gateway request handed it over (the gateway passes it to the API inside obid, where no socket can claim it), or null when it came on the loopback API with the API token. A device keeps one token: its new one replaces the old. Revoking the device (`DELETE /v1/pairing/devices/{id}`) drops its tokens, and a device whose pairing ran out is sent nothing, so a lost phone gets no more alerts. FCM answering `404` `UNREGISTERED`, or `400` `INVALID_ARGUMENT` about the registration token, drops it.
- **When.** As the Swift runner: two seconds after a job changes, each job whose need changed. obid's jobs never ask a question (`question` is null). A `done` job whose Oby review needs the owner (`reviewNeedsYou`, #525) alerts once per pushed head, urgent: `Couldn't review: <title>` (body its `reviewError`) when no reviewer could review it, else `Changes requested: <title>` (`Oby's review still asks for changes. Take it from here.`), both `job`. Its other alerts, in the Mac's words (ALERTS-WORDS), are `Ready to publish: <title>`, or `Couldn't publish: <title>` with a `publishError` (`ready`, body `publishError`, else the summary's first line), `Failed: <title>` and `Needs you: <title>` for an interrupted one (`job`, body the job's `reason`, else the summary's first line), and `Needs your OK: <title>`, or `Question: <title>` for the agent's question (`permission`, body the request whole: the permission's title, then its detail when the title doesn't hold it, 200 characters at most). What was last said for each job is kept in `home/push/notices.json`, so a restart doesn't say it again and jobs a restart interrupted notify once it's back; the first time, what the jobs already need isn't news. While no phone can hear it (no token or registration from a paired device; for FCM, no service account), nothing is recorded as said, so a phone that registers later is told every alert still open.
- **What's sent.** `POST https://fcm.googleapis.com/v1/projects/{project_id}/messages:send`, one data-only message per token (no `notification`, so the app draws it), every value a string, `android.collapse_key` the job's `id`, `android.ttl` `"86400s"`:

```json
{"message": {"token": "…", "data": {"job": "9f08fd10", "title": "Couldn't publish: Review oby #33", "body": "Couldn't push oby/review-33: the remote hung up.", "category": "ready"},
             "android": {"priority": "HIGH", "collapse_key": "9f08fd10", "ttl": "86400s"}}}
```

- When a job stops needing you, `data` is `{"clear": "<job id>"}` with `android.priority` `"NORMAL"`, and the app removes that job's notification.
- Every alert, FCM's and the relay's, is built from the jobs and the inbox with the vault's secrets, plain or encoded, already taken out (`[vault: <name>]`, as in an answer), so one in a job's title or a question is taken out and the alert still goes, and a value longer than the alert's cut leaves no piece. While the vault can't be read nothing is sent or remembered, and the next look after it's fixed says it. An alert's title and body then pass the secret scan (`crate::scan`, gitleaks too): a finding, or a scan that can't run, sends nothing for that job, nor tries it again, and the log says so with the job's id only. Sending never holds up the API; a failure is logged with FCM's status and error code, never the token or the access token.
- **The service account.** The owner's vault secret `oby.fcm.serviceAccount` ([obid only: the vault](#obid-only-the-vault)): the Google service account's JSON (`project_id`, `client_email`, `private_key` as PEM PKCS#8 RSA, `token_uri`, which must be `https://`). It lives where the vault lives, never reaches an agent, an answer, a log line or a push, and its private key and every access token it gets are taken out as `obid`'s own secrets too. `obid` signs a JWT with it (RS256; `iss` the `client_email`, `scope` `https://www.googleapis.com/auth/firebase.messaging`, `aud` the `token_uri`, an hour), trades it at `token_uri` for an access token (`grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer`) and uses that until 5 minutes before it ends. It's read again for each registration and each burst of alerts, so adding, replacing or removing it needs no restart.
- **What the owner does.** In the Firebase console, open the project that has the Android app (`com.deskree.oby`) › Project settings › Service accounts › Generate new private key, which downloads a JSON file. On the machine `obid` runs on, as its user: `obid vault --config <path to config.json> add oby.fcm.serviceAccount < ~/Downloads/<the file>.json`, then delete the downloaded file. The phone's next `POST /v1/notifications` answers `{"push": true}`.

**The iPhone, through Deskree's relay.** Apple's push key is Deskree's, so `obid` never talks to APNs: the iPhone registers with Deskree's relay (the control plane, which holds the key), then hands `obid` its push id, a key to sign pushes with (derived from the `pushSecret` the relay gave it, which `obid` never gets), and a key of its own that `obid` seals every alert to. The relay and Apple only ever carry a sealed blob; the iPhone's Notification Service Extension opens it. Nothing to set up on `obid`.

```json
{"apns": {"pushId": "…", "pushKey": "<base64url, 32 bytes>", "sealKey": "<base64url X25519 public key, 32 bytes>", "relay": "https://oby-control.deskree.com"}}
```

- `pushId`: the relay's id for this iPhone, 1 to 512 printable ASCII characters, no spaces. `pushKey`: what `obid` signs its pushes with, 32 bytes in base64url (padding optional); the phone derives it from its `pushSecret` and never hands `obid` the secret itself. `sealKey`: the iPhone's X25519 public key (CryptoKit's `rawRepresentation`), base64url, padding optional; `obid` seals a test alert to it, and one it can't seal to (not 32 bytes, a low-order point) is refused. `relay`: the control plane `obid` pushes through, its `pushRelay` (Deskree's, `https://oby-control.deskree.com`, unless config.json says another), written as an origin in any case (`https://Oby-Control.deskree.com/` is the same); any other is refused, so a device token can't point `obid`'s pushes anywhere else.
- It returns `{"push": true}` once kept, and `422` for a body that isn't one of these.
- Kept in `home/push/relay.json` (0600, written to a new file renamed into place) as `[{"pushId", "pushKey", "sealKey", "relay", "device"}]`, by the FCM tokens' rules: at most 20, the oldest out, one per paired device (its new one replaces it; a push id handed over again by another device replaces nothing), dropped when the device is revoked, and a device whose pairing ran out is sent nothing. One made with another control plane (`pushRelay` changed since) is sent nothing. The `pushKey` is `obid`'s own secret from then on (and again each time it starts), as written and as its raw bytes: taken out of every answer, log line and push. The relay answering `410` drops the registration.
- **What notifies** (the owner's rule of 2026-10-28, `push.rs`'s `notifies`: only what waits on the owner to decide or act; the phone shows nothing while Oby is in front). Two seconds after a job, the inbox or a conversation changes:

| What | `category` | Urgent | Title | Body |
|---|---|---|---|---|
| A job's agent waits for the owner's OK (`permission`) | `permission`; `permissionRule` when it offers Always allow (`always`); `plan` for a plan to approve; `job` for the agent's question | yes | `Needs your OK: <title>`, `Question: <title>` for the agent's question | the permission's title, then its detail when the title doesn't hold it |
| Oby planned something only the owner's Confirm carries out (a `confirm` item) | `confirm` | yes | `Confirm` | the item's `title`, then its `text` |
| Work waits for the owner's Start (a `start` item; one turn's offers, `offer`, are one alert) | `start` | yes | the item's `title`, or `Oby offers <n> jobs` for an offer of several | its (first item's) `text`, whole |
| An inbox item sorted `"triage": "needs_you"` | `inbox` | yes | the item's `title`; for another Oby's question (`obi.type` `question`), `Question from <obi.fromOwner>` (anything but printable ASCII as `\u{…}`) | its `text`, whole; for that question, `obi.quoted.text` |
| A job the owner started ends done with commits it couldn't publish by itself | `ready` | no | `Ready to publish: <title>`, `Couldn't publish: <title>` with a `publishError` | `publishError`, else the summary's first line |
| … `failed` or `interrupted` (not at a limit) | `job` | no | `Failed: <title>`, `Needs you: <title>` | the `reason`, else the summary's first line |
| A job's merge waits on the owner (`mergeNeedsYou`, ALERTS-7) | `job` | yes | its open `merge` card's title before `: ` (`Ready to merge`, else `Not merged`), then `: <title>` | `mergeError`'s first line, else `Not merged` |
| A queued job held for the owner's yes on a ChatGPT login (`waiting.reserved` empty, ALERTS-4) | `job` | yes | `Needs your OK: <title>` | `waiting.text` |

- Nothing else notifies: a job that ended done with nothing to decide (`Done: <title>`, FYI, built but gated out since 2026-10-28, as is Oby's message in a conversation, `message`: its page and its conversation say them), steps, rounds and other progress, a stopped or dismissed job, a job another Obi asked for, an inbox item sorted `fyi` or `ignore`, not sorted yet, or still being sorted (`pending`, until it becomes `needs_you`), a job's own `permission` item and any `merge` card (its job's alert covers them), a job that moves or waits at a limit (an `interrupted` one with `accountLimit` too) and any `limit` item (ALERTS-6), a merge waiting for a second review or for GitHub (MERGING-34, 35), a queued job held only by a weekly reserve or waiting out a limit (ALERTS-4), and the turns that tell a conversation a job ended (`ended`) or a routine ran (`routine`). Before the gate, Oby's message was pushed only while no paired device followed `GET /v1/events` through the gateway (Oby open on it), and not within two minutes of the owner's own words in that conversation; obid still keeps those rules, so letting FYI through again is one line in `notifies`.
- **One alert per thing.** Its `collapse` is the job's, the inbox item's or the conversation's id (`h-` and 32 hex digits, a digest, for one longer than 64 bytes or not printable ASCII, APNs' collapse id), and a newer alert for the same thing replaces it: a job's permission alert by its end. When an urgent alert's thing settles (answered, or gone), a short replacement with the same `collapse` goes: title `Settled`, body the old title, `category` `settled`, the same `thread`, `job`, `conversation` and `item`. A result isn't taken back. What was said for each thing is kept in `home/push/said.json`, so a restart doesn't say it again; the first time, what's already there isn't news. While no phone can hear it (no token or registration from a paired device; for FCM, no service account), nothing is recorded as said, so a phone that registers later is told every alert still open.
- **The alert**, sealed as JSON: `{"id", "at", "title", "body", "thread", "category", "job"?, "conversation"?, "item"?}`, the absent ones left out. `id` is this send's own (32 hex digits, new for every send, a `Settled` too): the phone shows an `id` once. `at` is when it was sealed, in seconds since the epoch: the phone drops one more than 24 hours old. The thing's id isn't in it; it's the relay's `collapse`, outside. `thread` is the conversation's id when there is one, else `jobs` or `inbox`. `job`, `conversation` and `item` say what a tap opens; `item` is the inbox item's id, which the alert's buttons answer (`POST /v1/inbox/{id}/answer` with `Allow`, `Always allow` or `Deny`; `Approve` or `Keep planning` for `plan`), on a job's permission `<job>-permission-<permission id>`. The title is at most 100 characters and the body 200, each on one line. Both pass the secret scan first: a finding, or a scan that can't run, sends nothing, and the log names only the alert's `id`.

```json
{"id": "5f0e8c1a3b2d4e6f8a9b0c1d2e3f4a5b", "at": 1800000000, "title": "Needs your OK: Android push", "body": "Run swift test swift test --filter Push", "thread": "jobs", "category": "permission", "job": "9f08fd10", "item": "9f08fd10-permission-p1"}
```

- **Sealing.** HPKE (RFC 9180) base mode, DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and ChaCha20-Poly1305 (CryptoKit's `.Curve25519_SHA256_ChachaPoly`), `info` the UTF-8 of `oby push v1`, no associated data, to the `sealKey`, with a fresh encapsulation each time. `sealed` is base64url, unpadded, of the encapsulated key (32 bytes) then the ciphertext (the alert and its 16-byte tag). The iPhone opens it with `HPKE.Recipient(privateKey:ciphersuite:info:encapsulatedKey:)` (the first 32 bytes) and `open(_:)` (the rest). `core/crates/obid/src/push.rs`, `cryptokit_opens_what_obid_seals`, holds a vector CryptoKit opened on macOS 26 (an alert with its `id` and `at`), and one CryptoKit sealed that `obid` opens.
- **The send.** `POST <pushRelay>/v1/push`, `Content-Type: application/json`, one per registration, with `Oby-Push-Mac`: base64url, unpadded, of HMAC-SHA256 keyed by the `pushKey`'s raw 32 bytes (decoded, never its base64url text), over the body exactly as sent. `collapse` is the thing's id, `urgent` whether it needs the owner, `at` the time now in seconds since the epoch (the relay refuses one more than 5 minutes off):

```json
{"at":1800000000,"collapse":"9f08fd10","pushId":"…","sealed":"…","urgent":true}
```

- An alert whose `sealed` would be longer than 3,072 characters (the relay's limit, so Apple's payload stays under 4 KB) isn't sent; titles and bodies are cut and lose their control characters so none is.
- Every alert goes to every phone at once, each push waiting at most 10 seconds for the relay, so a slow relay holds up none of the others. A `2xx` is taken. A `429`, a `5xx` or no answer is tried once more, the same send (its `id` too), after the relay's `Retry-After` (at most 10 seconds). `410` drops the registration, and so does the third `401` in a row. Anything else is logged with the relay's status only (its body isn't read; never the push id, the key or the blob). An urgent alert, or its `Settled`, that no phone took is said again at the next look; a result or a message isn't. Sending never holds up the API.
- **What leaves the Mac, and to whom.** To the control plane at `pushRelay` (Deskree's by default): the push id, the sealed blob, the `collapse` (a job's, an inbox item's or a conversation's id, or its digest), whether it's urgent, the time, and the MAC. Apple gets what the relay passes on. Neither sees a title, a body, the push key or anything else `obid` holds; the relay sees when and how often alerts go, and which are urgent.

### `GET /v1/alerts`

**obid only, loopback only** (ALERTS-1, [alerts.md](contracts/alerts.md); not on the gateway: the Mac's main process reads it with the API token). What waits on the owner now, as the phones are told it: the same jobs and open inbox the pushes look at, the vault's secrets taken out first (`[vault: <name>]`), then the gate above ("What notifies"), in the same words. Read-only, with no event of its own: a client reads it again after a `job`, `inbox`, `turn`, `conversation` or `routine` event.

```json
[{"key": "9f08fd10", "what": "permission:p1", "title": "Needs your OK: Android push", "body": "Run swift test",
  "category": "permissionRule", "thread": "jobs", "job": "9f08fd10", "item": "9f08fd10-permission-p1",
  "urgent": true, "answers": ["Allow", "Deny"]}]
```

- `key`: the thing it's about, the relay's `collapse` above (a job's, an inbox item's or an offer's id, or its `h-` digest); an alert replaces the one with the same `key`. `what`: changes when what it needs does (`permission:<id>`, `review:<head>`, `merge:<card>`, `consent`, `end:<finished>`, `open`), so an alert whose `what` changed is said again.
- `title`, `body`, `category`, `thread`, `job`, `conversation`, `item`: as the relay's sealed alert, `job`, `conversation` and `item` left out when it isn't about one. `urgent`: whether the relay is told it's urgent.
- `answers`: the words its buttons give, decided once in obid (`push.rs`, `answers`): `["Allow", "Deny"]` for a job's command or edit, when the banner shows the request whole (at most 120 characters on at most 3 lines, without control or format characters: `push.rs`'s `fits`, the measure the Mac's banner had); `["Start"]` for a `start` item offering it, alone (not one of several offered), from words that read nothing untrusted (`offer.untrusted`), that fits too; else `[]`, answered in the window (the agent's question, a plan, a Confirm, another Oby's question, a long request). Each answer goes where the window's does (`POST /v1/jobs/{id}/permission`, `POST /v1/inbox/{id}/answer`), signed as the window's are: the list carries words, never a token or a signature. The Mac app (`apps/electron/src/main/notify.ts`) answers each on its `item` (`POST /v1/inbox/{item}/answer`, a job's permission's item too), Allow and Deny signed with `X-Oby-Words`; it draws no button on a `confirm`, keeps another Oby's question's words as its card shows them (PEER-30), and remembers what it said by `key` and `what`.
- Every alert's title and body pass the secret scan first: one it stops, or can't scan, isn't listed. `503` while the vault can't be read, as every answer.

