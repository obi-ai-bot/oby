### obid only: projects

A project is a named set of folders and GitHub repositories that conversations belong to (A00-70, [docs/contracts/projects.md](../contracts/projects.md)). A conversation in a project works in the project's folders. Only the owner's Mac app reaches these routes. `projects-v1` in `capabilities` says obid serves them; the Swift runner has none (`404`), and an app sends `project` only to an obid that lists it.

**Where.** Each profile keeps its own projects in `projects.json` (0600) under its root ([profiles.md](profiles.md)): the default profile's is `home/projects.json`, as before, and another profile's `home/profiles/<id>/projects.json`. `{"projects": [{"id", "name", "folders", "repos"}]}`, in the order made. No file is no projects; a profile's file is written, never its folder made, only when a project is made, changed or deleted there. A file that doesn't read or parse is logged and left as it is: nothing writes over it, every route below answers `500` with a reason naming the file for that profile, and the start's reconcile (below) changes no conversation.

**Which profile.** Every route below answers for the profile the request names with its `Oby-Profile` header, by id, or with none for the default profile (obid only: profiles, "The `Oby-Profile` header"). A profile this Mac doesn't have, an empty value or two headers is `400`, nothing read or written. Another profile's project isn't listed, and is `404` on `PATCH` and `DELETE`, as an id no profile has. `profile-projects-v1` in `capabilities` says obid reads the header and keeps projects per profile.

A project, as every route answers it:

```json
{"id": "6f1c2b9e-…", "name": "Oby", "folders": ["/Users/alex/Projects/oby"], "repos": ["obi-ai-bot/oby"]}
```

- `id`: a lower-case UUID obid makes.
- `name`: trimmed, one line of 1 to 100 characters, with no line break (`\n`, `\r`, U+2028, U+2029) or other control character, as a conversation's title. Two projects may share one.
- `folders`: canonical paths, each checked as `folders` on `POST /v1/plans` is (obid only: working folders): absolute, there, a folder, none of the places Oby never works in; the same folder kept once, in the order sent, at most the owner's `limits.folders` (`A conversation works in at most 20 folders.`). One refused folder refuses the whole request with its own reason.
- `repos`: `owner/name` strings, each kept once (compared in any case). obid never calls GitHub for them, and they decide nothing yet.

**`GET /v1/projects`**: `200`, every project as kept.

**`POST /v1/projects`** with `{"name": "Oby", "folders": ["/Users/alex/Projects/oby"], "repos": ["obi-ai-bot/oby"]}`: `201` with the new project. `folders` and `repos` left out are `[]`.

**`PATCH /v1/projects/{id}`** with any of `name`, `folders` and `repos`: `200` with the project. New `folders` are written to `projects.json` first, then reach every conversation in the project: each answers them, its `revision` one higher, with its `conversation` event. A body with none of the three is `422`.

**`DELETE /v1/projects/{id}`**: `204`. Every conversation in it leaves it first: its `project` becomes `null`, and the folders it was working in (the project's) become its own, so the folders it answers don't change; each is written, its `revision` one higher, with its `conversation` event. Then the project goes. A conversation that can't be written is put back, the answer is `500`, and the project stays: `DELETE` again finishes it.

On every route: an unknown id is `404`; a body that doesn't parse (another key, a value of another type) is `400`; a refused value (a name, a folder, a repository that isn't `owner/name`, a missing `name` on `POST`) is `422` with a plain reason, and nothing is kept; `projects.json` that can't be written is `500`.

**A conversation's `project`** ([conversations.md](conversations.md)): the id of its project, or `null`, always sent in `GET /v1/conversations`, a search's answer, `PATCH`'s answer and the `conversation` event. While it's set, the conversation's `folders` are its project's (obid keeps a copy, `projectFolders` in `home/conversations/<id>.json`, never answered by itself); its own list waits underneath, and comes back when it's taken out.

- `PATCH /v1/conversations/{id}` `{"project": "<id>"}` puts it in the project of the profile `Oby-Profile` names (the default with none); an id that names no project of that profile is `422`, nothing changed. `{"project": null}` takes it out. `folders` in the same `PATCH` is `422` (`This conversation works in its project's folders: change the project, or take the conversation out of it.`) when it's in a project once the `PATCH`'s `project` applies, so `{"project": null, "folders": [...]}` is taken.
- `POST /v1/plans` `{"said": "…", "new": true, "project": "<id>"}` starts the conversation in the project. `project` is taken only with `"new": true` (`422` without); it must be a project of the profile `Oby-Profile` names (the default with none), and any other id is `422`, with no conversation made. `folders` sent with it become its own list underneath. Words with `folders` to a conversation in a project are `422`, nothing added. Words sent again with their `id` are `repeated`, whatever their `project`.

**At start**, before obid serves, obid reads every profile's projects (the default's and each other profile's in the registry, making no file or folder): each conversation whose `project` names a project kept in any of them gets that project's folders as they are now (written only when they differ), and each whose `project` names none leaves it as a delete takes it out. When any profile's `projects.json` doesn't read, no conversation changes. So a delete or a change cut short by a stop leaves no conversation pointing at a project that's gone; a project a cut-short delete left is still listed, with no conversation in it.

**Through the gateway and the phone relay** the four routes are off the gateway's list: `404` `That route isn't available through the gateway.` A conversation's `project` may only be `null` there: `{"project": "<id>"}` on `PATCH /v1/conversations/{id}`, and `project` on `POST /v1/plans`, are `422` (`Projects are chosen only on the Mac: from here you can take a conversation out of one.`).

The refusal texts are placeholders until Sangam's screens (A00-71) say otherwise.
