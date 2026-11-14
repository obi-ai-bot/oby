### Connection

On obid (2026-10-27), connected apps are gone: `GET`/`POST /v1/connections` and `DELETE /v1/connections/{name}` answer `404` (the apps treat that as none), and jobs get no connected app. GitHub is the only outside connection (docs/trim.md, batch 2), through `gh`. The per-job broker stays: Codex and Cursor agents reach `oby_ask_owner` on `POST /v1/agent/mcp/{job}` with the job's token, and Tetrix's server is `/v1/agent/tetrix/{job}` (obid only: Tetrix), whose calls are in the job's `connectionUses`.

## Routes obid does not serve

Routes this document or the apps once named that obid doesn't serve: obid answers `404` for each (`405` where it serves another method on the same path), on loopback and through the gateway, whose allowlist names none of them. Each bullet is one route, the date it went and why. The Brain (`tools/brain`) reads this list: a route on it never counts as documented, and one obid routes again is a check (docs/contracts/retired.md, RETIRED-7 and RETIRED-8).

- `POST /v1/speech` (gone 2026-11-02): the Swift runner's speech service, deleted with it; obid never served it, and the iPhone transcribes on the phone (`PhoneSpeech.swift`).
- `POST /v1/activities` (gone 2026-11-02): the Swift runner's Live Activity push tokens, deleted with it; obid never served it and sends no Live Activity pushes.
- `GET /v1/watches` (gone 2026-11-02): the Swift runner's list of paired Apple Watches, deleted with it; obid never served it.
- `POST /v1/watches` (gone 2026-11-02): pairing an Apple Watch with the Swift runner, deleted with it; obid never served it.
- `DELETE /v1/watches/{id}` (gone 2026-11-02): unpairing an Apple Watch from the Swift runner, deleted with it; obid never served it.
- `POST /v1/jobs/{id}/start-when` (gone 2026-11-02): the Swift runner's way to change what a waiting job waits for, deleted with it; obid never served it, and a waiting job is started now or stopped.
- `POST /v1/jobs/{id}/proposals/{index}/file` (gone 2026-11-02): the Swift runner's filing of a research job's proposed issue, deleted with it; obid never served it, and its report's `proposals` is always `[]`.
- `PUT /v1/settings/models` (gone 2026-11-02): the Swift runner's per-tool models, deleted with it; obid never served it (`405`), and a model and efforts are an account's.
- `GET /v1/connections` (gone 2026-10-27): connected apps went with the strip to a dispatcher (Connection, above).
- `POST /v1/connections` (gone 2026-10-27): connecting an app, gone with them.
- `DELETE /v1/connections/{name}` (gone 2026-10-27): disconnecting an app, gone with them.
- `GET /v1/checks/next` (gone 2026-10-27): the Mac app's separate Mac checks went with the strip to a dispatcher (Mac check, above).
- `GET /v1/checks/{id}/bundle` (gone 2026-10-27): a Mac check's bundle, gone with them.
- `POST /v1/checks/{id}/result` (gone 2026-10-27): a Mac check's result, gone with them.
- `POST /v1/hooks/{id}/deliver` (gone 2026-10-25): a hook's deliveries, cut with webhooks in trim batch 2b (#1446, docs/trim.md).

The Watch API, the Swift runner's second listener for the Apple Watch, went with the Swift runner's routes on 2026-11-02 (#454, closed not planned): the Watch reaches obid through its iPhone.

