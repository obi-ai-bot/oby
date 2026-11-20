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

## `home/addresses.json`

obid writes it (0600) each time it starts, once its API and gateway are bound: `{"api": "127.0.0.1:<port>", "gateway": "127.0.0.1:<port>"}`, `gateway` left out when there's none. Only `obid pair-computer` reads it.

## `POST /v1/pairing/invite` with `"computer": true`

See [phone-pairing.md](phone-pairing.md): the invitation is for the given `url` only, never through the Obysphere, even while obid is a member of one; without a `url` it's `400` (`Another computer is invited to a url: its gateway's.`).
