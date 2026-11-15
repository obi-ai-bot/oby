### obid only: profiles

Chrome-style profiles on this Mac (docs/contracts/profiles.md): a local id and name, never an identity. PROF-1 keeps the registry and the list route, PROF-3a creates and renames profiles; nothing else in obid reads them yet, a second profile holds nothing of its own, and apps show nothing. The Swift runner has none: `404`.

**Where.** `home/profiles.json` (0600): `{"profiles":[{"id","name","color","default"}]}`. An install with no file, or one whose `profiles` list is empty, gets one first profile on start (`id` a lower-case UUID, `name` `Default`, `color` `#3a5a8c`, `default` true) that holds everything the install already has: nothing moves, renames or signs out. An existing registry keeps its names (an install upgraded before PROF-3a keeps `Personal`). A file that doesn't read is left as it is, logged, and answered as that same first profile without writing over it; creating and renaming then answer `409` until obid starts with a file that reads. A profile that isn't the default has its own folder, `home/profiles/<id>` (0700), made when it is created and empty: nothing reads or writes in it yet.

**`GET /v1/profiles`**: every profile, as kept:

```json
[{"id": "3f0c…", "name": "Default", "color": "#3a5a8c", "default": true, "waiting": false}]
```

`waiting` is always `false` until later work has something in a profile that waits on the owner. Older runners answer `404`.

**`POST /v1/profiles`**: a new profile. The body is `{"name": string, "color": string}`, both required, and no other key. `name` is trimmed and must be 1 to 40 characters; `color` is `#` and six hex digits (any case), kept as sent. obid makes its id (a lower-case UUID) and its folder, keeps it in `profiles.json` with `default: false`, and answers `201` with it as `GET` shows it:

```json
{"id": "9b2e…", "name": "Acme", "color": "#7a4a8c", "default": false, "waiting": false}
```

**`PATCH /v1/profiles/{id}`**: a new name, colour or both, checked as above; nothing else of the profile changes. The body is `{"name"?: string, "color"?: string}` with at least one of them; `id`, `default` or any other key is refused. The default profile can be renamed and recoloured, and stays the default. Answers `200` with the profile:

```json
{"id": "3f0c…", "name": "Home", "color": "#3a5a8c", "default": true, "waiting": false}
```

Both answer `{"error": "…"}` with:

- `400`: a body that isn't JSON or isn't a profile, a missing, blank or over-40-character name, a colour that isn't `#rrggbb`, an unknown key, or a change with no key. Nothing is written.
- `404` (`PATCH` only): no profile has that id. Nothing is written.
- `409`: `profiles.json` couldn't be read when obid started, and is left as it is. Nothing is written.
- `500`: the folder or `profiles.json` couldn't be written; the reason is in `error`, and the profiles stay as they were.

Older runners answer `404`.
