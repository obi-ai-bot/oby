# What obid has, as the app keeps it

## Settings rows obid has no setting for yet (R06-13)

The business behavior contract for Settings rows whose choice obid doesn't keep yet, from [the template](../business-contract-template.md). Roadmap task R06-13 (`factory/queue/done/R06-13.md`); it uses R06-04's channels ([ipc.md](ipc.md)). Scenarios LIVE-25 and LIVE-26; the tests that prove them are named after them (`live_25_…`, `live_26_…`, `apps/electron/src/renderer/src/test/localSettings.test.ts` and `.tsx`).

### Feature and status

- **Outcome:** A Settings choice that obid doesn't keep yet, such as Open at login, survives a restart of the app, in one file main owns, instead of looking saved and coming back on its default.
- **Status:** **Implemented** in the Electron app (`apps/electron`): the file in `src/main/settings.ts`, the page's half in `useSetting` (`src/renderer/src/SettingsView.tsx`), the bridge as `window.app.settings` (`src/preload/index.ts`).
- **Scope:** every row `useSetting` holds, the demo rows `?everything` shows included, but `accounts`. **Out of scope:** making a row do anything (nothing ties Open at login to the Mac's login item), moving a row into obid, any change to how a row looks, reads or defaults, the choices `useKeptSetting` keeps in the page's `localStorage` (`models.mine`, `agent.default`, `talk.default`, outside this channel), and obid and its API ([runner-api.md](../runner-api.md) is unchanged).

### Actors and ownership

- **The owner** turns a row in Settings.
- **The page** shows each row: the value set in this launch, else the kept one when it has the type of the row's default (the same `typeof`, an array only for an array default, `null` only for a `null` default), else the default. It reads what main kept once, as it starts, and sends each change to main. Until main's answer comes the row shows its default, and a change made before then isn't replaced by it.
- **Main** owns `settings.json` in the app's own folder (`src/main/settings.ts`): its `local` section holds the rows, an object of row key to JSON value, beside main's own keys (`notifications`, `updatesAuto`). Every write reads the file, sets its keys and writes it whole, owner-only (0600) and never through a link (`writeOwn`), so a row keeps main's keys and main's keys keep `local`.
- **obid** owns nothing here.

### The channels

- `local-settings` (no arguments): the `local` section, only the keys a row may have.
- `local-setting` (`rowKey`, `rowValue`): `local[key] = value`, answered `true`. A key is a string matching `^[A-Za-z][A-Za-z0-9._-]{0,63}$`; a value is plain JSON of at most 65,536 characters, kept as it reads back.
- Both are refused (`null`) to anything but the window's own page; `local-setting` with a key or value the decoders can't read gets `null` and writes nothing ([ipc.md](ipc.md)).
- In a browser (`npm run web`) there is no bridge: the rows stay in memory while the page is open, as before.

### Failures and recovery

- **The file can't be read** (missing, not JSON) or its `local` isn't a plain object: `local-settings` answers `{}` and every row shows its default.
- **A kept value of the wrong type:** the row shows its default, and nothing throws. Turning the row keeps the new value over it.
- **A write fails** (the folder is gone, the disk is full): `local-setting` answers `false` and main logs it; the row keeps its new value for this launch.
- **Main's answer is refused or fails:** nothing is read, and the rows show their defaults.

### Invariants and boundaries

- The page's channels never read or write a key of main's own: only `local`'s entries.
- `accounts` is never kept nor read from `local`: on obid it's obid's own list (`replaceAccounts`), and a kept list would show stale accounts.
- No row's look, words or default changes.

### The local rows

What each row holds, where it shows and what stands in its place on obid. None has a task that moves it into obid yet.

| Key | Page | Who sees it | On obid today | Task |
|---|---|---|---|---|
| `login` | General, Open at login | everyone | The same switch. Its value is now kept, but nothing yet ties it to the Mac's login item: it opens nothing at login. | no task yet |
| `voice.speaks` | General, Voice | `?everything`, a browser | Nothing: the Mac has no voice yet. | no task yet |
| `mac.mode` | General, Permissions on this Mac (the mock) | `?everything`, a browser | obid's `/v1/settings/permissions` (`MacPermissions`). | no task yet |
| `mac.rules` | General, Permissions on this Mac (the mock's rules) | `?everything`, a browser | obid's saved rules, on the same page. | no task yet |
| `remote.on`, `remote.forgot` | Remote access (the mock) | a browser | Pairing (Pair a device, Paired devices) through the owner's Obysphere. | no task yet |
| `share.mac`, `share.when`, `share.who`, `share.chosen`, `share.jobs`, `share.size`, `share.work`, `share.guests`, `share.grants` | Sharing | `?everything` | Nothing: Sharing is hidden at launch, and obid's side of it went with the strip to a dispatcher. | no task yet |
| `claude.model`, `claude.effort`, `claude.complexEffort`, `codex.model`, `codex.effort`, `codex.complexEffort`, `cursor.model` | Models, and the mock's models on Agents | `?everything`, a browser | Settings › Agents, each account's model and efforts kept by obid. | no task yet |
| `vault` | Vault | `?everything` | Nothing: Vault is hidden for launch. | no task yet |
| `updates.auto` | About (the mock) | a browser | Main's own `updatesAuto` in the same file ([updates.md](updates.md)). | no task yet |
| `connected` | Connectors (the mock) | a browser | obid's GitHub and Tetrix. | no task yet |

Kept outside this channel, in the page's `localStorage` (`useKeptSetting`): `models.mine`, `agent.default` and `talk.default`.

### Acceptance scenarios

- **LIVE-25** Given the owner changes a local Settings row, when the app restarts, then the row shows the changed value. Main: `local-setting('login', false)` from the window's own page answers `true`, and after main's modules load again `local-settings` answers `{ login: false }`, from a `settings.json` of `{"local":{"login":false}}` with mode 0600. The page: Open at login shows on until main's answer comes, then off; turned before the answer comes, it keeps the owner's turn and sends it to main. `accounts` is never sent nor read; a refused sender gets `null` and nothing is written; a key of main's own (`notifications`) reaches only `local.notifications`; `__proto__`, `''`, a number, a 65-character key and a value over 65,536 JSON characters are refused with the file unchanged; alerts, Update automatically and the rows share the file. `live_25_a_local_row_set_through_main_is_there_after_a_restart` (both files), and the other `live_25: …` cases.
- **LIVE-26** Given a value of the wrong type in `settings.json`, when the row reads it, then it shows its default. With `local` holding `{ login: "yes", "share.chosen": 3 }`, the rows show `true` and `[]`, and nothing throws; a file that isn't JSON, and a `local` that is a string or an array, answer `{}`. `live_26_a_kept_value_of_the_wrong_type_shows_the_rows_default` (both files).

#### Test it when it's done (on a real Mac)

1. `npm run dev`: turn Open at login off in Settings › General, quit Oby and open it again: it shows off. Notifications and Update automatically keep their own choices.
