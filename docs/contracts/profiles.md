# Chrome-style profiles

The business behavior contract for local profiles on this Mac (Chrome-style), from [the template](../business-contract-template.md). Scenarios are numbered `PROFILES-n`; the tests that prove them are named after them (`profiles_1_…` in `core/crates/obid/src/profiles.rs` and `core/crates/obid/tests/profiles_api.rs`, `profiles_12_…` in `core/crates/obid/src/lib.rs`, `profiles_19_…` to `profiles_25_…` in `apps/electron/src/renderer/src/test/profileMenu.test.tsx`).

## Feature and status

- **Outcome:** The owner can have more than one local working context on this Mac (a profile: its own name and colour), the way Chrome keeps people on one browser. A profile is a local id and name on this Mac, never an identity, never a sign-in, never something that leaves the Mac.
- **Status:**
  - **PROF-1 is implemented**: `home/profiles.json`, `GET /v1/profiles` returning `[{id, name, color, default, waiting}]`, and the upgrade that gives an install with no file one first profile holding everything it already has. Nothing else in obid reads the registry yet.
  - **PROF-2 is implemented**: an internal refactor. `Config.mac_home` names the Mac-wide stores; it adds no screen, setting, API field or profile selection.
  - **PROF-3a is implemented**: creating a profile (`POST /v1/profiles`) and renaming or recolouring one (`PATCH /v1/profiles/{id}`), and the storage-root rule that gives each profile its folder, with its Kani proof (PROFILES-17).
  - **The Electron app's panel title and menu are implemented (A00-76, screen 07)**: the chat-first side panel's title is the open profile's name, and its menu lists the profiles, chooses one, and creates one; the app keeps which one is open across restarts. On a real obid, choosing a profile changes only the title and the check: the panel lists the same conversations whichever is open. On the mock, each profile shows its own.
  - **Not built yet:** deleting a profile, or changing which one is the default; scoping jobs, conversations, projects, accounts, settings, permissions or the vault to a profile; a way for a request to say which profile it is for; a `waiting` flag that is ever `true`.
- **Plain statement:** a second profile does not yet hold anything of its own. Its folder is made and stays empty, and every job, conversation, account, setting, permission and vault secret is still the install's, whichever profiles exist.
- **Scope for PROF-1:** the registry file, the list route, the upgrade. **Scope for PROF-2:** Mac-wide vs profile storage roots. **Scope for PROF-3a:** the create and rename routes and the storage-root rule. **Scope for A00-76:** the Electron app's panel title, its menu and the New profile form. **Out of scope:** profile switching in obid, moving data between profiles, linking a profile to a person or an Obysphere member.

## Behavior

### Actors and ownership

- **obid** owns the registry (`home/profiles.json`, 0600), writes the first profile on upgrade, answers `GET /v1/profiles`, creates and changes profiles, and owns all storage decisions (Mac-wide vs profile roots).
- **Clients** show the list and create profiles: the Electron app's side panel title is the open profile, its menu lists them from `GET /v1/profiles` and makes one with `POST /v1/profiles`. Which one is open is the app's own choice, kept in the window's local storage (`oby.profile`): the kept one while obid lists it, else the default, else the first. No request names a profile. PROF-2 adds no client surface.

### Triggers and prerequisites

- obid starts, or `GET /v1/profiles` is asked: with no `profiles.json` (or one whose `profiles` list is empty), obid writes one first profile and leaves every other file and folder in its home as they were.
- A profile's `id` is a lower-case UUID obid makes. On a new install, the first profile's `name` is `Default` (a label, never who the person is); an install whose registry already exists keeps its names, so one upgraded before PROF-3a keeps `Personal`. Its first `color` is `#3a5a8c` (Sangam's first avatar colour). It is `default: true`. `waiting` is always `false` until later work has something in a profile that waits on the owner.

### States and transitions

- **No registry:** upgrade writes the first profile; existing jobs, conversations, accounts, usage, permissions, logins and every other home path stay where they are, with the same names and contents. Nothing signs out.
- **Registry present:** obid answers its profiles as kept; it does not rewrite them on start.
- **Unreadable registry:** obid logs why, answers one first profile for the request, and does not write over the file: creating and renaming are refused until obid starts again with a file that reads.

### Creating and renaming (PROF-3a)

- `POST /v1/profiles` takes `{"name", "color"}`, both required, and no other key. The name is trimmed and must be 1 to 40 characters; the colour must be `#` and six hex digits, kept as sent. obid makes a lower-case UUID id, makes the profile's folder (0700, and `home/profiles` if missing), adds `{id, name, color, default: false}` to `profiles.json` and answers it as `GET` shows it.
- `PATCH /v1/profiles/{id}` takes `name`, `color` or both, checked the same way, and changes only those. `id` and `default` can't be changed: a body with them, with no key, or with any other key is refused. The default profile can be renamed or recoloured; it stays the one default.
- A refused request writes nothing. A failed write leaves the registry as it was.

### Storage root

A profile's own things go under its storage root: the default profile's is obid's `home` itself, whatever its id, so an existing install has nothing moved; any other profile's is `home/profiles/<id>`, only when its id is a lower-case UUID (hex digits with `-` at 8, 13, 18 and 23). Any other id (empty, the wrong length, holding `/`, `\`, `.`, a NUL or an upper-case letter) has no root. Only creating a profile uses the rule yet, to make the folder.

### Failures and recovery

| What happens | What obid does | What the owner sees |
|---|---|---|
| `profiles.json` can't be written on upgrade | Logged; `GET` may still answer the in-memory first profile | Nothing |
| `profiles.json` doesn't parse | Logged; answered as one first profile; file left as it is | Nothing |
| A bad name or colour, a body that isn't a profile, an unknown key, or a change with no key | `400` with the reason; nothing written | obid's reason in the New profile form |
| `PATCH` of an unknown id | `404`; nothing written | Nothing (the app doesn't rename) |
| Create or rename while `profiles.json` didn't read at start | `409`: the file couldn't be read and is left as it is; nothing written | obid's reason in the New profile form |
| The folder or `profiles.json` can't be written | `500` with the reason; the registry is left as it was | obid's reason in the New profile form |

### Invariants and boundaries

- A profile is a local id and name on this Mac, never an identity, never sent to another Oby, an agent, GitHub or the Obysphere.
- Upgrade never moves, renames, deletes or signs out a job, conversation, account, login, setting or any other home path.
- API add-only: `GET /v1/profiles`, `POST /v1/profiles` and `PATCH /v1/profiles/{id}` are new; no existing key or closed enum the Swift or Kotlin apps decode gains a new value.
- Nothing else in obid reads the registry or a storage root yet: jobs, conversations, accounts and settings stay global to the install as they were, and nothing reads or writes inside `home/profiles/<id>`.

## Implemented: Mac-wide storage (PROF-2)

`Config.mac_home` identifies the root for the API token, publishing devices and pairing
file, gateway devices, phone relay key, push registrations and startup cleanup measurement.
Configuration loading sets it to `home`: every file stays where it was, with the same
permissions, failure handling and recovery. It is not a JSON configuration option.
Sandbox detection and the in-memory egress proxy retain their existing startup behavior.
Per-profile stores (including `home/profiles.json`) and `Jobs::open` are unchanged; splitting them belongs to PROF-3.

## Interfaces and dependencies

- Persistence: `home/profiles.json` (0600), `{"profiles":[{"id","name","color","default"}]}` under the profile root (`home`). Mac-wide stores use `Config.mac_home` (today equal to `home`).
- `GET /v1/profiles` (obid only): `[{id, name, color, default, waiting}]`. Older runners answer `404`. Documented in [runner-api.md](../runner-api.md), "obid only: profiles".
- `POST /v1/profiles` (`{name, color}` → `201` and the profile) and `PATCH /v1/profiles/{id}` (`{name?, color?}` → `200` and the profile), obid only, on the same token as the list; phones can't reach them through the gateway. Documented in [api/profiles.md](../api/profiles.md).
- A profile's folder: `home/profiles/<id>` (0700), empty.

## Acceptance scenarios

- **PROFILES-1** (`profiles_1_upgrade_keeps_everything_in_the_first_profile`): an install with no `profiles.json` (and with accounts, jobs, usage and permissions already there) → after start, `GET /v1/profiles` is one profile `{id, name: "Default", color: "#3a5a8c", default: true, waiting: false}`, `profiles.json` holds that profile, and every other home file is byte-identical; the account is still signed in. A home that already has a registry keeps it and answers it with `waiting: false`.
- **PROFILES-12** (`profiles_12_mac_stores_use_the_mac_home`): With distinct internal `mac_home` and `home` roots, startup prepares the Mac root, uses its API token, devices/pairing, gateway, relay and push stores, and measures its worktrees. Profile stores remain under `home`. Normal configuration still makes both roots equal.
- **PROFILES-13** (`profiles_13_a_new_profile_is_added_with_its_own_folder`): on an install with accounts, usage and permissions, `POST /v1/profiles {"name":"  Acme  ","color":"#7a4a8c"}` answers `201` with exactly `id` (a lower-case UUID), `name: "Acme"`, `color`, `default: false` and `waiting: false`; `GET` then lists both, the first still the default; `profiles.json` (0600) holds both; `home/profiles/<id>` is an empty 0700 folder; every other file is byte-identical and the account is still listed.
- **PROFILES-14** (`profiles_14_a_bad_name_or_colour_writes_nothing`): an empty, blank or 41-character name, a colour `red` or `#12345`, a missing colour, an extra key or a body that isn't JSON each answer `400` with an `error`; `profiles.json` is byte-identical, `home/profiles` doesn't exist, and `GET` lists one profile.
- **PROFILES-15** (`profiles_15_a_profile_changes_only_its_name_and_colour`): renaming the default profile keeps its id, colour and `default: true`; recolouring a created one changes only its colour; `{"default":true}`, `{"id":"x"}` and `{}` answer `400` and an unknown id `404`, each leaving `profiles.json` byte-identical; exactly one profile stays the default.
- **PROFILES-16** (`profiles_16_an_unreadable_registry_is_never_written_over`): with `profiles.json` holding `not json`, `GET` answers one first profile, and creating and renaming answer `409`; the file still reads `not json` and `home/profiles` doesn't exist.
- **PROFILES-17** (`profiles_17_a_storage_root_is_inside_home_under_its_id`, and the Kani harness `profiles_17_only_a_lower_case_uuid_is_a_profile_id` in `core/crates/obid/src/profiles.rs`): the default profile's root is `home`; another's is `home/profiles/<id>`, inside `home`, for a lower-case UUID id; an empty, `.`, `..`, `../x`, `a/b`, upper-case, dotted, slashed, 35- or 37-character id has none. Kani proves that of every 36 bytes, exactly the lower-case UUIDs are accepted, so an accepted id never holds a path separator, a dot, a NUL or an upper-case letter.
- **PROFILES-18** (`profiles_18_an_install_with_one_profile_behaves_as_before`): an install with accounts, usage, permissions and one default profile named `Personal` starts as before: `GET` answers that profile with `waiting: false`, every file is byte-identical, `home/profiles` doesn't exist, and the account is listed.
- **PROFILES-19** (`profiles_19_one_profile`): obid lists one default profile `Default` `#3a5a8c` → the panel's title is its name with a chevron, and its menu holds one checked row (its mark in `#3a5a8c`), then New profile.
- **PROFILES-20** (`profiles_20_three_profiles_and_a_choice`): obid lists Default (the default), Deskree and Acme → the menu shows the three in obid's order, only Default checked. Choosing Acme makes the title Acme and moves the check, keeps Acme's id as `oby.profile`, leaves the panel's conversation rows the same ones in the same order, and asks obid nothing but its list again.
- **PROFILES-21** (`profiles_21_one_waiting`): with Deskree `waiting: true`, only Deskree's row has the orange dot; with every profile `waiting: false`, no row has one.
- **PROFILES-22** (`profiles_22_creating`): New profile, a name and a colour, then Create → `POST /v1/profiles` with exactly `{name, color}`; on `201` the form closes, the profile is in the menu and open, and the title is its name. A `400` with `{"error": …}` leaves the form open with obid's words, and the list, the title and `oby.profile` as they were.
- **PROFILES-23** (`profiles_23_kept_across_a_restart`): Acme chosen, the app starts again with the same answer from obid → the title is Acme. A kept id obid no longer lists → the default profile's name, the kept id left as it is.
- **PROFILES-24** (`profiles_24_an_obid_without_profiles`): `GET /v1/profiles` answers `404`, or obid can't be reached → the title is Default with no menu, no error shows, and the conversation rows are as they are without it.
- **PROFILES-25** (`profiles_25_the_mock_shows_each_profiles_own`): on the mock (`?everything&chat`) the menu lists Default, Deskree with its dot, and Acme; choosing Deskree shows Deskree's own conversations and none of Default's.

**Test it when it's done (real Mac):**

1. On a Mac that has used Oby before this build (jobs and an account already there), upgrade and open nothing new: `curl -s -H "Authorization: Bearer $(cat \"$HOME/Library/Application Support/Oby/Runner/home/api-token\")" http://127.0.0.1:7777/v1/profiles` shows the one profile it had (`Personal` on a Mac upgraded before PROF-3a; `Default` on a new install); Settings › Agents still shows the same accounts; no conversation or task is gone.
2. Restart Oby with the existing configuration. Confirm existing paired devices remain listed, the app connects with its existing API token, and a paired phone still connects and receives notifications. No files should move or require pairing again.
3. With `TOKEN` set to that API token, create and rename a profile: `curl -s -X POST -H "Authorization: Bearer $TOKEN" -d '{"name":"Acme","color":"#7a4a8c"}' http://127.0.0.1:7777/v1/profiles` answers the new profile; `curl -s -X PATCH -H "Authorization: Bearer $TOKEN" -d '{"name":"Acme Corp"}' http://127.0.0.1:7777/v1/profiles/<its id>` answers it renamed; `GET /v1/profiles` lists both; `home/profiles/<its id>` is an empty folder; Settings › Agents, the conversations and the tasks are as before.
4. In the app, click the side panel's title (Default, with its arrow): the menu lists the profiles with Default checked, then New profile…. Make one with New profile…: the title is its name. Quit and open Oby again: the title is still that profile. The conversations listed are the same whichever profile is open.
