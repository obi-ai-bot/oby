# obid only: other computers

The runner's side of "other computers" ([contracts/remote.md](../contracts/remote.md)): the owner's window on this Mac uses another Mac's or Linux box's obid, reached over any network they already have or an SSH tunnel, never through the Obysphere.

## `obid pair-computer`

What the person's own SSH runs on the other computer, as the user obid runs as there, when they add it from their Mac (COMPUTERS-7). It's placed with obid, never installed over SSH.

```bash
ssh -o BatchMode=yes -T -- <their alias> '/path/to/obid pair-computer --config /path/to/config.json' < name-and-key
obid pair-computer          # macOS, Oby.app's own obid: ~/Library/Application Support/Oby/obid/config.json
```

- **stdin:** the Mac's name on the first line, its device key on the second (P-256, X9.63 uncompressed, base64: the key the Mac app signs the owner's words with), and nothing else. Neither goes on the command line, where `ps` would show it. Anything else (no key, a key that isn't one, a third line, more than 4 KiB) prints `That isn't a device key.` on stderr and exits 2, having written nothing. So an `authorized_keys` line can make it a key's forced command (`restrict,port-forwarding,permitopen="127.0.0.1:<gateway>",command="…"`), and that key can do nothing but pair.
- **What it does:** names the key in obid's `home/pairing` (0600), the owner's presence as `POST /v1/devices` already takes it ([phone-pairing.md](phone-pairing.md)); registers it with `POST /v1/devices` on that obid's own API, with its API token, which stays on that computer; and makes a one-time invitation with `POST /v1/pairing/invite` `{"url": "http://<its gateway>", "name": <the Mac's name>, "computer": true}` (COMPUTERS-8: never through the Obysphere).
- **stdout,** when it paired: one JSON line, `{"device": {"id": "…"}, "invite": {"url", "name", "hostname", "link", "secret", "code", "expiresAt"}}`. It goes back over SSH only. The Mac redeems `invite.secret` through its tunnel to the gateway (`POST /v1/pairing`, [phone-pairing.md](phone-pairing.md), "Redeeming") for its device token, and signs the owner's words there with its key, registered as `device.id`.
- **Where it reaches obid:** the addresses that obid bound when it last started, from `home/addresses.json` (COMPUTERS-9), a port the system picked included; the configuration's `listen` and `gateway` while that file isn't there.
- **Exit 1,** one line on stderr, nothing paired: the configuration doesn't read; obid has no `gateway` (`This obid has no gateway, so no other computer can reach it: …`); it never started there (no `api-token`); it doesn't answer (`obid doesn't answer on 127.0.0.1:<port>: is it running?`); or it refused a step (`obid refused /v1/devices (403 Forbidden): …`). After a refusal `home/pairing` is taken out again.

## Direct addresses

With `direct` in obid's configuration ([configuration.md](configuration.md)), obid serves its gateway's own router over HTTPS on those addresses too (COMPUTERS-10): the same `POST /v1/pairing`, device tokens and allowlist as the gateway's listener ([phone-pairing.md](phone-pairing.md)), and never the pairing endpoints, which stay on the loopback API. Without a `gateway` the configuration doesn't load.

- **Its certificate** is this computer's own: self-signed P-256 (subject `CN=<the machine's name>`, `notAfter` 99991231235959Z), made the first time obid serves a direct address and kept in `home/keys/direct-certificate.der` and `home/keys/direct-key.p8` (PKCS#8, both 0600). Its SHA-256 is its pin, which a window trusts in its place: no CA, no name. A start keeps it; it's made anew only when its files are gone or the key isn't the certificate's, and then every window that pinned the old one refuses it until the computer is added again.
- **`GET /v1/status`** names each address under `gateway.direct`: `{"address": "0.0.0.0:8443", "state": "listening", "pin": "sha256:<64 hex>"}`, or `{"address", "state": "down", "detail"}` when it can't listen there (a warning in the log, never a stop). obid's log says `obid direct listening` with each address and pin; stdout keeps only its two lines, which launchers read first.
- **A handshake** has 10 seconds; one that doesn't finish, or fails, is dropped and holds up no other.

## `home/addresses.json`

obid writes it (0600) each time it starts, once its API, gateway and direct addresses are bound: `{"api": "127.0.0.1:<port>", "gateway": "127.0.0.1:<port>", "direct": ["0.0.0.0:<port>"]}`, `gateway` and `direct` left out when there are none. Only `obid pair-computer` reads it.

## `POST /v1/pairing/invite` with `"computer": true`

See [phone-pairing.md](phone-pairing.md): the invitation is for the given `url` only, never through the Obysphere, even while obid is a member of one; without a `url` it's `400` (`Another computer is invited to a url: its gateway's.`).
