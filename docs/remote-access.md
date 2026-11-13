# Remote access: reach your Mac from anywhere (#285)

> **Hidden at launch** (the owner's decision of 2026-10-23, AGENTS.md "The launch build"): Settings › Remote access isn't shown on the Mac or the iPhone, and the iPhone reaches the Mac through the user's own Obysphere instead, sealed end to end ([runner-api.md](runner-api.md), "obid only: the phone relay"; obid's side #1368, the iPhone's #1377, and the Mac's Pair a device makes a relay QR with no tailnet route, [onboarding.md](onboarding.md) steps 8–12). The control plane's side of it is gone: [Removed](#removed-from-the-control-plane-r03-02) below. The Swift Mac app's full builds, which carried `cloudflared`, went with it on 2026-11-02. [Pushes](#pushes), the sealed push relay in the same Worker, isn't hidden.

## Removed from the control plane (R03-02)

Sign-in with Apple and Google, installations, one Cloudflare Tunnel per Mac, hooks (`/h/{id}`) and the deliveries kept for a Mac that was away were removed from the control plane by R03-02 ([docs/contracts/retired.md](contracts/retired.md), "The control plane"): nothing called them any more. Git history keeps them, and this page's earlier versions describe them. The Worker in `cloud/control-plane/` now serves only `GET /v1/health` and the [push relay](#pushes); every other path answers 404 `{"error": "No such route."}`. Its sweep, every minute, now only clears ended rate-limit windows: the push relay's as soon as they end, and any other a day old.

## Working on the Worker locally

```sh
cd cloud/control-plane
cp .dev.vars.example .dev.vars    # placeholders; fill in what you have
pnpm migrate:local                # the D1 database in .wrangler/
pnpm dev --env <name>             # http://localhost:8787, with that instance's vars
pnpm test   # inside workerd (Miniflare), with APNs faked
pnpm check  # TypeScript
```

Without the APNs secrets and `PUSH_SECRET_KEY`, or with no `APNS_TOPICS`, the push routes answer 503 (`Pushes aren't set up on this control plane.`). `wrangler dev --test-scheduled` runs the sweep at `/__scheduled`.

## Pushes

How Oby reaches the iPhone when the phone isn't talking to the Mac (#398, the owner's decision of 2026-10-23, which replaced that issue's first plan): through a relay in this Worker that only ever sees sealed blobs. Apple's push key is Deskree's and can't ship inside Oby, so obid can't call APNs itself; the relay holds the key, and the alerts stay between obid and the phone. This is contract v2 (the coordinator's review of 2026-10-23): obid never holds the phone's `pushSecret`, and every push carries its time.

1. The iPhone registers its APNs device token with the relay, with no sign-in. The relay checks the token with Apple, then answers a `pushId` and a `pushSecret`.
2. The phone gives its obid the `pushId`, the `pushKey` made from the secret, and the public key alerts are sealed to (obid's side: docs/runner-api.md). It keeps the `pushSecret` to itself: only the secret re-registers or forgets the phone.
3. For each alert, obid seals it to the phone's key and posts the sealed blob to the relay with its time, signed with an HMAC under the `pushKey`.
4. The relay checks the HMAC and the time, and sends Apple a fixed alert carrying the blob. The phone's Notification Service Extension opens the blob and shows the real alert; one it can't open shows the fixed words, "Oby: Open Oby to see it."

These routes are open to any phone, outsiders included, on every instance: the phone never signs in.

**The keys.** Every HMAC is HMAC-SHA-256, and **every HMAC key is raw bytes, never base64url text**: a `pushSecret`'s or a `pushKey`'s raw bytes are its base64url decoded, 32 bytes. Every result is base64url with no padding (43 characters).

- `pushSecret` = HMAC(key: the UTF-8 bytes of the relay's own `PUSH_SECRET_KEY`, message: UTF-8 `oby-push-secret/1`, a newline, then the `pushId`). The relay makes it again whenever it needs it, so D1 keeps no secret. Only the relay makes it.
- `pushKey` = HMAC(key: the 32 raw bytes of `pushSecret`, message: UTF-8 `oby push key v1`). The phone makes it, and the relay makes it again from the `pushSecret`.
- `Oby-Push-Mac` = HMAC(key: the 32 raw bytes of `pushKey`, message: the raw request body bytes, exactly as sent).
- A `pushSecret` or `Oby-Push-Mac` that isn't exactly the relay's own 43 base64url characters (32 bytes, no padding) is refused (401), compared in constant time.

A fixed vector to check against (`test/push.test.ts`, "the keys", worked out apart from the relay with Python's `hmac`):

| | |
| --- | --- |
| `pushSecret` | `7x-XJBtltDkgComjTzh2aougWSW5C4fOPr7MutSocH4` |
| `pushKey` | `vx_9SyYw9p7qPCxR8xHlDF8WlTOsR_LRA67SHhzpTHM` |
| body | `{"pushId":"b2J5LXRlc3QtdmVjdG9yLTAx","sealed":"c2VhbGVkLWZvci10aGUtcGhvbmU","collapse":"9f08fd10","urgent":true,"at":1791806400}` |
| `Oby-Push-Mac` | `EK3ocbR_NCA7i3gJQAGEnXJe2mGFd74zqt190mk1a7s` |

### The calls

| Call | What |
| --- | --- |
| `POST /v1/push/devices` `{"apns", "topic", "sandbox"}` | From the iPhone, `Content-Type: application/json` only (415 otherwise). A web page can't send that without a CORS preflight, which this Worker refuses, so a page can't register tokens from its visitors' addresses. `apns` is the device token in hex (64 to 200 characters, kept lower-case); `topic` the app's bundle id, one of `APNS_TOPICS` (`com.deskree.oby.ios` on Deskree's instance); `sandbox` true for a development build (APNs' sandbox), false for TestFlight and the App Store. **A token new to the relay is checked with Apple before it's kept:** one background push (`apns-push-type: background`, priority 5, `{"aps": {"content-available": 1}}`, nothing shown). If Apple answers 410, `BadDeviceToken` or `DeviceTokenNotForTopic` (a wrong `sandbox` too), the answer is 422 and nothing is kept; if Apple doesn't answer, 502. Then 200 `{"pushId", "pushSecret"}`, `Cache-Control: no-store`: `pushId` is 24 base64url characters (18 random bytes), `pushSecret` 43. A token another `pushId` already has moves to this one, and the other is forgotten. Any other field is 422, a body over 1 KiB 413. Limits: 30 new phones an hour per client and 200 per network (429), 2,000 an hour over everyone, 10 checks an hour per token, and `MAX_PUSH_DEVICES` registered at once (503, with a warning in Workers Logs past 80%; the count is one row kept by triggers, so a registration reads one row, not the table). |
| `POST /v1/push/devices` `{"apns", "topic", "sandbox", "pushId", "pushSecret"}` | The phone registers a changed token for the same id: 200 with the same `pushId` and `pushSecret`. A changed token is checked with Apple first; the same token again isn't. It brings back an id the relay forgot (after Apple said the token was bad), since the secret proves it's that phone's, so the phone may call it at every launch. A wrong secret (the `pushKey` too), an id never made, or one of the two missing: 401, the same answer each time; the phone then registers anew and gives obid the new `pushId` and `pushKey`. 20 an hour per `pushId`. |
| `DELETE /v1/push/devices/{pushId}` | With `Authorization: Bearer <pushSecret>`. Forgets the phone: 204, also when it was gone already; 401 for a wrong or missing secret, the `pushKey` included, so obid can't. |
| `POST /v1/push` `{"pushId", "sealed", "collapse"?, "urgent"?, "at"}` | From obid, with `Oby-Push-Mac` (above). `at` is the push's time in Unix seconds, required; the MAC covers it, and more than 5 minutes from the relay's clock either way is 401. (The phone drops repeats and stale alerts by the sealed alert's id and time.) `sealed` is base64url, at most 3,072 characters (413), so Apple's payload stays under its 4 KB; `collapse` 1 to 64 printable ASCII characters with no space (it reaches Apple as `apns-collapse-id`, so make it opaque); `urgent` true or false, false when it's missing. Any other field is 422, a body over 4 KiB 413. A missing or wrong MAC is 401, the same for an id never made, and it's checked before D1 is read or anything is counted. Then 30 a minute and 1,000 a day per `pushId`, and the same per token, so one phone registered under new pushIds shares one allowance (429). The relay sends `POST https://api.push.apple.com/3/device/<token>` (`api.sandbox.push.apple.com` for a sandbox phone) with `apns-topic`, `apns-push-type: alert`, `apns-priority` 10 when urgent and 5 otherwise, `apns-collapse-id` when given, and `{"aps": {"alert": {"title": "Oby", "body": "Open Oby to see it."}, "mutable-content": 1, "sound": "default"}, "sealed": "<blob>"}`. Answers 200 `{"ok": true}` when Apple took it. 410 when the phone isn't registered: Apple answered 410, `BadDeviceToken` or `DeviceTokenNotForTopic` (then the relay forgets it), or it was forgotten before; obid stops pushing to it until the phone hands it over again. 429 when Apple says too many to this phone, with its `Retry-After`. 503 when the relay's own sign-in with Apple is renewed too often (`TooManyProviderTokenUpdates`), or until the APNs key and `PUSH_SECRET_KEY` are set. 502 when Apple refused the push (its reason in the error) or didn't answer. |

The provider token is an ES256 JWT (`kid` the key's id, `iss` the team, `iat` now) signed with the `.p8` key. One token is shared by every isolate for 40 minutes, since Apple wants a new one at most every 20 minutes and at least every 60. It's kept in D1's `apns_provider_token` only sealed: AES-256-GCM under a key that is the HMAC of `oby-apns-token/1` under `PUSH_SECRET_KEY`, with the key's id and team as associated data. It's made again after Apple refuses it.

### What the relay can and can't see

- **What it keeps** (D1): each phone's `pushId`, APNs token, topic and `sandbox` (`push_devices`), and how many there are (`push_device_count`). No time, no address, no Mac, no account, and no secret. The sealed provider token above is the only other thing kept.
- **The rate limits' counters** (`rate_limits`, keys `push:<window>:…`) count per window. Each window is a whole minute, hour or UTC day, so a row says which minute or day it counts, never when a push or registration came. The sweep removes each one within a minute of its window's end. Registrations are counted by client and network address, by token hash for checks, and by `pushId` for re-registrations. Pushes are counted by `pushId` and by token hash. No row links an address to a `pushId`.
- **What passes through:** the sealed blob, which the relay can't open (obid seals it to the phone's own key); `collapse`, `urgent`, `at` and the `pushId`; and the address the call came from, which Cloudflare sees. Pushes carry nothing that names a Mac, only a MAC under the `pushId`'s `pushKey`, so the relay can't tell which Mac sent a push.
- **What's logged:** a push Apple didn't take, or a token check that failed for a reason other than a bad token, logs Apple's status and reason (`push: APNs 410 Unregistered, phone forgotten`). Nothing else of a push or a registration is logged: never the token, the secret, the `pushKey`, the `pushId`, `collapse` or the blob (`test/push.test.ts` checks every log line). **Invocation logs are off** for the whole control plane (`observability.logs.invocation_logs: false` in `wrangler.jsonc`, at the top and in `env.deskree`). They would record every request's URL, headers and address: the address every phone registers from and every Mac pushes from.
- **Apple** sees what it always does: the token, the topic, the fixed alert, the sealed blob and the collapse id, plus one background push per new token.
- **What it can't do:** read an alert; learn which Mac or account a phone belongs to; push without the phone's `pushKey`; list phones (ids are 144 random bits, and a wrong id and a wrong MAC or secret get the same 401). obid can push but can't re-register or forget a phone.
- **What's left:**
  - Whoever holds `PUSH_SECRET_KEY` (the instance's operator) can make any phone's `pushKey` and push to it. They can't seal an alert the phone opens, so the phone shows only the fixed words.
  - Anyone who learns a phone's APNs token can register it for themselves. That moves the token off the phone's own `pushId` until the phone registers again. The token goes only to Apple and the relay.
  - A push replayed within its 5 minutes reaches the phone again, which drops it by the sealed alert's id.
  - Junk registrations need real tokens of this app, and stay until `MAX_PUSH_DEVICES`. The rates bound how fast they come.

### Setting it up (the owner's step)

Deploying is the owner's; nothing here is in git.

1. **Make the APNs key.** In Certificates, Identifiers & Profiles on Deskree's team: Keys › **+**, name it "Oby push relay", tick **Apple Push Notifications service (APNs)** › Configure: environment **Sandbox & Production**, and limit it to the topic `com.deskree.oby.ios` when Apple offers that. Register, then **Download** `AuthKey_<KEY ID>.p8` (Apple lets you download it once). Note its **Key ID**, and the **Team ID** from Membership details.
2. **Set the four secrets**, in `cloud/control-plane`, each read from stdin so none lands in your shell history:
   ```sh
   pnpm exec wrangler secret put APNS_KEY --env deskree < ~/Downloads/AuthKey_XXXXXXXXXX.p8
   pnpm exec wrangler secret put APNS_KEY_ID --env deskree
   pnpm exec wrangler secret put APNS_TEAM_ID --env deskree
   openssl rand -base64 32 | pnpm exec wrangler secret put PUSH_SECRET_KEY --env deskree
   ```
   Then move the `.p8` into your password manager and delete it from Downloads. `PUSH_SECRET_KEY` is in Cloudflare alone, and nobody needs to know it.
3. **Migrate and deploy:** `pnpm migrate:remote --env deskree` (it adds `push_devices`, `push_device_count` and `apns_provider_token`), then `pnpm run deploy --env deskree`. The deploy also turns invocation logs off. On an instance that already runs an older Worker, deploy first and migrate after: migration 0009 drops the tables R03-02 retired, and no running code may read them ([docs/contracts/retired.md](contracts/retired.md), "The control plane").
4. **Check it:** `curl -s -X POST https://oby-control.deskree.com/v1/push -H 'Content-Type: application/json' -d '{}'` answers `{"error":"pushId is the phone's pushId."}` (422). `Pushes aren't set up on this control plane.` (503) means one of the four secrets is missing.

Rotating the APNs key changes nothing on the phones: make a new key, put the three `APNS_*` secrets again, then revoke the old key. `PUSH_SECRET_KEY` isn't rotated: a new one makes every phone's secret and `pushKey` wrong (401), and each must register anew and hand obid its new `pushId` and `pushKey`. Another instance sets its own app's bundle id in its environment's `APNS_TOPICS` and its own team's key.

## On the iPhone

Settings › Your Mac (`apps/ios/app/ConnectScreen.swift`) links the Mac: its name (as Sharing settings call it) and its hostname, from the Mac's pairing QR code (`MacLink`'s JSON, `{"name", "hostname"}`, and `relay` when it came through an Obysphere). A Mac paired through its Obysphere, the launch build's only way, is reached through the Sphere's relay alone, sealed end to end, at home and away: no Bonjour, no tunnel ([runner-api.md](runner-api.md), "obid only: the phone relay"). A Mac paired over a direct route (a full build, with `showEverything`) is reached this way instead:

- **At home,** it looks for the Mac's gateway with Bonjour (`_oby._tcp`, the service named after the Mac), resolves its address and talks to it on the local network. `apps/ios/app/Info.plist` declares the service and the local-network use.
- **Away,** it goes through the tunnel, `https://<hostname>`.

The switch between the two keeps the conversation: only the connection is made anew (`apps/shared/RemoteRoute.swift` picks the route; `Phone.base` is where the calls go). Unlink, and the iPhone is back on the runner.
