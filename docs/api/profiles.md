### obid only: profiles

Chrome-style profiles on this Mac (docs/contracts/profiles.md): a local id and name, never an identity. PROF-1 keeps the registry and the list route; nothing else in obid reads them yet, and apps show nothing. The Swift runner has none: `404`.

**Where.** `home/profiles.json` (0600): `{"profiles":[{"id","name","color","default"}]}`. An install with no file, or one whose `profiles` list is empty, gets one first profile on start (`id` a lower-case UUID, `name` `Personal`, `color` `#3a5a8c`, `default` true) that holds everything the install already has: nothing moves, renames or signs out. A file that doesn't read is left as it is, logged, and answered as that same first profile without writing over it.

**`GET /v1/profiles`**: every profile, as kept:

```json
[{"id": "3f0c…", "name": "Personal", "color": "#3a5a8c", "default": true, "waiting": false}]
```

`waiting` is always `false` until later work has something in a profile that waits on the owner. Older runners answer `404`.

