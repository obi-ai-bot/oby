### obid only: profiles

Chrome-style profiles on this Mac (docs/contracts/profiles.md): a local id and name, never an identity. PROF-1 keeps the registry and the list route, PROF-3a creates and renames profiles, and A00-79's first slice lets a request name its profile with `Oby-Profile` (below), for its projects only. The Electron app lists and creates profiles for its panel's title and sends no `Oby-Profile` yet; a second profile holds only its own projects. The Swift runner has none: `404`.

**Where.** `home/profiles.json` (0600): `{"profiles":[{"id","name","color","default"}]}`. An install with no file, or one whose `profiles` list is empty, gets one first profile on start (`id` a lower-case UUID, `name` `Default`, `color` `#3a5a8c`, `default` true) that holds everything the install already has: nothing moves, renames or signs out. An existing registry keeps its names (an install upgraded before PROF-3a keeps `Personal`). A file that doesn't read is left as it is, logged, and answered as that same first profile without writing over it; creating and renaming then answer `409` until obid starts with a file that reads. A profile that isn't the default has its own folder, `home/profiles/<id>` (0700), made when it is created and empty; its projects are kept there in `projects.json` once one is made in it ([projects.md](projects.md)).

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

**The `Oby-Profile` header** (obid only, A00-79): the profile a request is for, by its id (`Oby-Profile: 9b2e…`; the header's name in any case). It is read on `GET` and `POST /v1/projects`, on `PATCH` and `DELETE /v1/projects/{id}`, and on a non-null `project` of `PATCH /v1/conversations/{id}` and `POST /v1/plans` ([projects.md](projects.md)); every other route, and those requests with no `project` or `"project": null`, ignore it. With no header a request is for the default profile, as before. It is refused with `400` and `{"error": …}`, nothing read or written and never answered from the default profile, when its value is no profile's id exactly (an unknown id, `../x`, another case), empty or not plain text (`There's no such profile on this Mac.`), or when it is sent more than once (`Send one Oby-Profile.`); a profile whose id in `profiles.json` isn't a lower-case UUID has no folder, and is refused the same way. `400`, not `404`, so an app never takes it for a route an older obid lacks. `profile-projects-v1` in `GET /v1/status`'s `capabilities` says obid reads it; an app sends the header only to an obid that lists it, since an older one ignores it and answers the default profile's projects. The gateway doesn't forward it, so a phone always reaches the default profile. The refusal texts are placeholders.
