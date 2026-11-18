### obid only: profiles

Chrome-style profiles on this Mac (docs/contracts/profiles.md): a local id and name, never an identity. PROF-1 keeps the registry and the list route, PROF-3a creates and renames profiles, A00-79's first slice lets a request name its profile with `Oby-Profile` (below), and B00-40 keeps each profile's conversations, jobs, helpers, accounts and vault secrets its own. The Electron app lists and creates profiles for its panel's title and names the open one on every call and on its event stream. The Swift runner has none: `404`.

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

**`DELETE /v1/profiles/{id}`** (PROFILES-44, 45): a profile that isn't the default goes, answering `204` with no body. Everything it kept becomes the default profile's, and nothing is deleted, stopped or signed out: its projects are added to the default's `projects.json` after its own (same ids), its id is kept in `profiles.json` under `"merged": [ids]` (left out of the file until a profile is deleted), which makes every conversation, job, helper, account and vault secret kept with it the default's, and its folder `home/profiles/<id>` goes. A request naming it then is refused `400` as an unknown profile. `409` for the default profile (`The default profile isn't deleted…`) or while `profiles.json` didn't read at start; `404` for an id no profile has; `500` when the default's `projects.json` doesn't read or can't be written, or `profiles.json` can't be written (the default's projects put back as they were). Each refusal writes nothing.

Each answers `{"error": "…"}` with:

- `400`: a body that isn't JSON or isn't a profile, a missing, blank or over-40-character name, a colour that isn't `#rrggbb`, an unknown key, or a change with no key. Nothing is written.
- `404` (`PATCH` only): no profile has that id. Nothing is written.
- `409`: `profiles.json` couldn't be read when obid started, and is left as it is. Nothing is written.
- `500`: the folder or `profiles.json` couldn't be written; the reason is in `error`, and the profiles stay as they were.

Older runners answer `404`.

**The `Oby-Profile` header** (obid only, A00-79, B00-40): the profile a request is for, by its id (`Oby-Profile: 9b2e…`; the header's name in any case). It is read on every `/v1` route and on `GET /v1/events` (B00-40, `profile-scope-v1`): the project routes and a conversation's `project` ([projects.md](projects.md)), and every list, read, change and use of a conversation, a job, a helper, an account or a vault secret answer only that profile's (below). With no header a request is for the default profile, as before. It is refused with `400` and `{"error": …}`, nothing read or written and never answered from the default profile, when its value is no profile's id exactly (an unknown id, `../x`, another case), empty or not plain text (`There's no such profile on this Mac.`), or when it is sent more than once (`Send one Oby-Profile.`); a profile whose id in `profiles.json` isn't a lower-case UUID has no folder, and is refused the same way. `400`, not `404`, so an app never takes it for a route an older obid lacks. `profile-projects-v1` in `GET /v1/status`'s `capabilities` says obid reads it; an app sends the header only to an obid that lists it, since an older one ignores it and answers the default profile's projects. The gateway doesn't forward it, so a phone always reaches the default profile. The refusal texts are placeholders.

**What each profile keeps** (obid only, B00-40, `profile-scope-v1`; docs/contracts/profiles.md, "Each profile keeps its own"):

- `GET /v1/conversations` (`q=` too) and `GET /v1/conversation` answer only the profile's conversations; `POST /v1/plans` `{"new": true}` makes one in it, and `conversation` naming another profile's is `404`.
- `GET /v1/jobs` lists only the profile's jobs; `POST /v1/jobs` makes one in it, on its accounts only (naming another profile's account is refused as one obid doesn't have).
- `GET /v1/status`'s `accounts` lists only the profile's; `POST /v1/accounts` adds one to it. Its `availableLogins` are the tools' own logins no profile holds; `otherProfileLogins` (add-only, PROFILES-43) are those another profile holds as an account, `[{"agent", "id", "profile", "profileName"}]`, by the holding profile's id and name; and `talkNext` is the account a new conversation of the profile would talk through.
- `GET /v1/helpers` lists only the profile's; `POST /v1/helpers` makes one in it (`profile`, when sent, must be the request's: `422` otherwise).
- `GET /v1/obid/vault` lists only the profile's names; `PUT` of a name another profile keeps is `409`, `DELETE` of one `404`.
- `/v1/jobs/{id}…`, `/v1/conversations/{id}`, `/v1/accounts/{id}…` and `/v1/helpers/{id}` of another profile's are `404`, as an id obid doesn't have.
- `GET /v1/events` sends a job's, a conversation's and their turns' events only when they're the profile's, and `accounts` with only its accounts.
- A conversation's summary, a job and their events carry `profile`, the profile's id, only for a profile that isn't the default (add-only).
