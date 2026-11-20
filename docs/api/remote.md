# obid only: other computers

The runner's side of "other computers" ([contracts/remote.md](../contracts/remote.md)): the owner's window on this Mac uses another Mac's or Linux box's obid, reached over any network they already have or an SSH tunnel, never through the Obysphere.

## `obid pair-computer`

What the person's own SSH runs on the other computer, as the user obid runs as there, when they add it from their Mac (COMPUTERS-7). It's placed with obid, never installed over SSH.

```bash
ssh -o BatchMode=yes -T -- <their alias> '/path/to/obid pair-computer --config /path/to/config.json' < name-and-key
obid pair-computer          # macOS, Oby.app's own obid: ~/Library/Application Support/Oby/obid/config.json
```

- **stdin:** the Mac's name on the first line, its device key on the second (P-256, X9.63 uncompressed, base64: the key the Mac app signs the owner's words with), and nothing else. Neither goes on the command line, where `ps` would show it. Anything else (no key, a key that isn't one, a third line, more than 4 KiB) prints `That isn't a device key.` on stderr and exits 2, having written nothing. So an `authorized_keys` line can make it a key's forced command (`restrict,port-forwarding,permitopen="127.0.0.1:<gateway>",command="…"`), and that key can do nothing but pair.
- **What it does:** names the key in obid's `home/pairing` (0600), the owner's presence as `POST /v1/devices` already takes it ([phone-pairing.md](phone-pairing.md)); registers it with `POST /v1/devices` on that obid's own API, with its API token, which stays on that computer; and makes a one-time invitation with `POST /v1/pairing/invite` `{"url": "http://<its gateway>", "name": <the Mac's name>, "computer": true, "presence", "device": <the key's id>}` (COMPUTERS-8: never through the Obysphere), its `presence` from `home/invite-computer` as `obid invite-computer`'s is, so redeeming it binds that key to the pairing (COMPUTERS-15).
- **stdout,** when it paired: one JSON line, `{"device": {"id": "…"}, "invite": {"url", "name", "hostname", "link", "secret", "code", "expiresAt"}}`. It goes back over SSH only. The Mac redeems `invite.secret` through its tunnel to the gateway (`POST /v1/pairing`, [phone-pairing.md](phone-pairing.md), "Redeeming") for its device token, and signs the owner's words there with its key, registered as `device.id`.
- **Where it reaches obid:** the addresses that obid bound when it last started, from `home/addresses.json` (COMPUTERS-9), a port the system picked included; the configuration's `listen` and `gateway` while that file isn't there.
- **Exit 1,** one line on stderr, nothing paired: the configuration doesn't read; obid has no `gateway` (`This obid has no gateway, so no other computer can reach it: …`); it never started there (no `api-token`); it doesn't answer (`obid doesn't answer on 127.0.0.1:<port>: is it running?`); or it refused a step (`obid refused /v1/devices (403 Forbidden): …`). After a refusal `home/pairing` is taken out again.

## Direct addresses

With `direct` in obid's configuration ([configuration.md](configuration.md)), obid serves its gateway's own router over HTTPS on those addresses too (COMPUTERS-10): the same `POST /v1/pairing`, device tokens and allowlist as the gateway's listener ([phone-pairing.md](phone-pairing.md)), and never the pairing endpoints, which stay on the loopback API. Without a `gateway` the configuration doesn't load.

- **Its certificate** is this computer's own: self-signed P-256 (subject `CN=<the machine's name>`, `notAfter` 99991231235959Z), made the first time obid serves a direct address and kept in `home/keys/direct-certificate.der` and `home/keys/direct-key.p8` (PKCS#8, both 0600). Its SHA-256 is its pin, which a window trusts in its place: no CA, no name. A start keeps it; it's made anew only when its files are gone or the key isn't the certificate's, and then every window that pinned the old one refuses it until the computer is added again.
- **`GET /v1/status`** names each address under `gateway.direct`: `{"address": "0.0.0.0:8443", "state": "listening", "pin": "sha256:<64 hex>"}`, or `{"address", "state": "down", "detail"}` when it can't listen there (a warning in the log, never a stop). obid's log says `obid direct listening` with each address and pin; stdout keeps only its two lines, which launchers read first.
- **A handshake** has 10 seconds; one that doesn't finish, or fails, is dropped and holds up no other.

## `obid invite-computer`

What the other computer's owner runs at it to add it to their Mac with a code (COMPUTERS-13), as the user obid runs as there.

```bash
obid invite-computer --config /path/to/config.json
obid invite-computer          # macOS, Oby.app's own obid
```

- **What it does:** reads the running obid's status for its direct addresses ([Direct addresses](#direct-addresses)), writes a one-time `home/invite-computer` (0600, good for 2 minutes and once) as the owner's presence at this machine, and asks for `POST /v1/pairing/invite` `{"url", "name", "computer": true, "presence"}`.
- **stdout:** one JSON line, `{"code": "123456", "link": "oby-computer:…", "name", "fingerprint": "ABCD 1234 …", "expiresAt"}`. `fingerprint` is the pin's first 16 bytes in groups of four hex digits, to compare by eye; the link carries the whole pin.
- **Exit 1,** one line on stderr: obid isn't running or never started here; or `This computer has no address another computer can reach. Set "direct" in obid's configuration, or add it over SSH instead.`; or obid refused.

## A computer's invitation

`POST /v1/pairing/invite` with `"computer": true` ([phone-pairing.md](phone-pairing.md)):

- **`computerLink`** in the answer, when obid serves direct addresses: `oby-computer:` and base64url JSON, `{"v": 1, "name", "routes": [{"id", "kind": "lan" | "tailnet" | "vpn" | "public" | "loopback", "via": {"type": "https", "host", "port", "pin": "sha256:<hex>"}}]}`. An address on every interface (`0.0.0.0`, `[::]`) is each of the machine's own addresses but loopback and IPv6 link-local; at most 8, in the owner's order.
- **Its own slot:** a computer's invitation replaces the computer's before it, never a phone's, and a phone's never replaces it. It lasts 10 minutes. A wrong code counts against every open invitation; the fifth ends it.
- **`presence`:** what `obid invite-computer` or `obid pair-computer` wrote in `home/invite-computer`. When it holds, or with the owner's device's `approval`, the computer it pairs may add its own key (`POST /v1/devices` through the gateway, for 10 minutes); otherwise it pairs one that can't.
- **`device`:** with `presence` or `approval` only, a registered key bound to no pairing that's still paired (`obid pair-computer`'s): redeeming the invitation binds it to the pairing it makes (COMPUTERS-15). Without either, or for a key that's another pairing's, `403` and nothing made.

## A computer's token

A pairing a computer's invitation made is a computer's (`"computer": true` in `GET /v1/pairing/devices`), and its device token differs from a phone's through the gateway and its direct addresses:

- **Its routes (COMPUTERS-14):** the gateway's list, as a phone's ([phone-pairing.md](phone-pairing.md)), and nothing past it, so every promise that a route stays on obid's loopback API holds for a computer too. Among them: the vault, all three routes ([vault.md](vault.md), VAULT-1 to 5; vault secrets go in only on the Mac, the owner's word of 2026-11-20); pairing (`/v1/pairing/devices`, `/v1/pairing/invite`); every sign-in, Tetrix's, ChatGPT's, GitHub's and an account's ([chatgpt-sign-in.md](../contracts/chatgpt-sign-in.md), core/AGENTS.md "Tetrix"); accounts but `PATCH /v1/accounts/{id}`, `PUT /v1/settings/agents/{agent}` and `/v1/settings/chatgpt` ([accounts.md](accounts.md), "Not on the gateway"); `POST /v1/accounts/{id}/background` ([chatgpt-sign-in.md](../contracts/chatgpt-sign-in.md)); adding folders and a conversation's workspace ([folders.md](folders.md)); attachments ([attachments.md](../contracts/attachments.md)); projects ([projects.md](projects.md)); `GET /v1/alerts` and `DELETE /v1/devices/{id}` ([alerts.md](alerts.md)); renaming and deleting artifacts ([artifacts.md](artifacts.md)); a job's changes and Undo ([jobs.md](jobs.md)); `POST /v1/repos/check` and `/v1/repos/setup` ([github.md](github.md)); `PUT /v1/obid/sphere/owner` and cancelling an exchange ([obysphere.md](obysphere.md), SPHERE-21, OBIREQ-5). Past the list is `404` (`That route isn't available through the gateway.`). For now a computer reaches what a phone does; the window switching computers adds each route it needs to a computer's list by name, with the screen that needs it, each reviewed on its own.
- **One route as written:** each segment of the path is decoded before the list is read, whoever's token it carries, and the list reads the decoded path (`/v1/%70airing/devices` is `/v1/pairing/devices`, off it). obid is sent that same path, encoded the one standard way (every byte but letters, digits, `-`, `.`, `_` and `~` as `%XX`), with the request's query: so `/v1/jobs/<id>/rep%6Frt` reaches the report, as the list read it, never another route. A segment that's empty, `.` or `..`, holds a backslash or NUL, or has an escape that isn't two hex digits, isn't UTF-8 or decodes to any of those, a `/` or a `%`, is `404` (`That route isn't available through the gateway.`).
- **The owner's words (COMPUTERS-15):** an `X-Oby-Words` header through it counts only signed by the key bound to that pairing: the key `obid pair-computer` named, or the one the computer added with `POST /v1/devices` through the gateway after a code. Any other key's, a registered one included, is `403` (`Those words aren't signed by this computer's own key. Add this computer again.`) before the route sees it.
- **Removing it (COMPUTERS-16):** `DELETE /v1/pairing` on the gateway (or a direct address) with its own token ends its pairing, as `DELETE /v1/pairing/devices/{id}` does on that computer's loopback, and forgets the key bound to it: `204`. Its token is `401` from then on, and the key needs the owner's presence there again. A phone's token is `404` there; none is `401`.

## Adding a computer with a code, the Mac's side

`obid::remote::code::redeem(link, code, name, key)`: it decodes the link, reaches its first route that answers (`GET /v1/status`) with that route's pin, then sends `POST /v1/pairing` `{"credential": <code>, "name"}` there and `POST /v1/devices` `{"name", "publicKey"}` with the token it got. No route reached is `Oby can't reach {name} right now. It tried …`, each route with why: a certificate not the pinned one says `another computer answered: its certificate is …`. The window's endpoint for it comes with the computers this Mac keeps.

## `home/addresses.json`

obid writes it (0600) each time it starts, once its API, gateway and direct addresses are bound: `{"api": "127.0.0.1:<port>", "gateway": "127.0.0.1:<port>", "direct": ["0.0.0.0:<port>"]}`, `gateway` and `direct` left out when there are none. Only `obid pair-computer` reads it.

## `POST /v1/pairing/invite` with `"computer": true`

See [phone-pairing.md](phone-pairing.md): the invitation is for the given `url` only, never through the Obysphere, even while obid is a member of one; without a `url` it's `400` (`Another computer is invited to a url: its gateway's.`).
