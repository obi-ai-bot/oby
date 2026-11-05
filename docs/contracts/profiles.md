# Chrome-style profiles

The business behavior contract for local profiles on this Mac (Chrome-style), from [the template](../business-contract-template.md). Scenarios are numbered `PROFILES-n`; the tests that prove them are named after them (`profiles_1_…` in `core/crates/obid/src/profiles.rs` and `core/crates/obid/tests/profiles_api.rs`, `profiles_12_…` in `core/crates/obid/src/lib.rs`).

## Feature and status

- **Outcome:** The owner can have more than one local working context on this Mac (a profile: its own name and colour), the way Chrome keeps people on one browser. A profile is a local id and name on this Mac, never an identity, never a sign-in, never something that leaves the Mac.
- **Status:**
  - **PROF-1 is implemented**: `home/profiles.json`, `GET /v1/profiles` returning `[{id, name, color, default, waiting}]`, and the upgrade that gives an install with no file one first profile holding everything it already has. Nothing else in obid reads the registry yet; apps show nothing (no circle, no name under Chats yet: docs/contracts/chat-first.md).
  - **PROF-2 is implemented**: an internal refactor. `Config.mac_home` names the Mac-wide stores; it adds no screen, setting, API field or profile selection.
  - **Not built yet:** switching, creating or renaming a profile; scoping jobs, conversations, accounts or settings to a profile (PROF-3); anything the apps show.
- **Scope for PROF-1:** the registry file, the list route, the upgrade. **Scope for PROF-2:** Mac-wide vs profile storage roots. **Out of scope:** UI, profile switching, moving data between profiles, linking a profile to a person or an Obysphere member.

## Behavior

### Actors and ownership

- **obid** owns the registry (`home/profiles.json`, 0600), writes the first profile on upgrade, answers `GET /v1/profiles`, and owns all storage decisions (Mac-wide vs profile roots).
- **Clients** may call the list route; for PROF-1 they show nothing from it. PROF-2 adds no client surface.

### Triggers and prerequisites

- obid starts, or `GET /v1/profiles` is asked: with no `profiles.json` (or one whose `profiles` list is empty), obid writes one first profile and leaves every other file and folder in its home as they were.
- A profile's `id` is a lower-case UUID obid makes. Its first `name` is `Personal` (a label, never who the person is). Its first `color` is `#3a5a8c` (Sangam's first avatar colour). It is `default: true`. `waiting` is always `false` until later work has something in a profile that waits on the owner.

### States and transitions

- **No registry:** upgrade writes the first profile; existing jobs, conversations, accounts, usage, permissions, logins and every other home path stay where they are, with the same names and contents. Nothing signs out.
- **Registry present:** obid answers its profiles as kept; it does not rewrite them on start.
- **Unreadable registry:** obid logs why, answers one first profile for the request, and does not write over the file.

### Failures and recovery

| What happens | What obid does | What the owner sees |
|---|---|---|
| `profiles.json` can't be written on upgrade | Logged; `GET` may still answer the in-memory first profile | Nothing yet (apps show nothing) |
| `profiles.json` doesn't parse | Logged; answered as one first profile; file left as it is | Nothing yet |

### Invariants and boundaries

- A profile is a local id and name on this Mac, never an identity, never sent to another Oby, an agent, GitHub or the Obysphere.
- Upgrade never moves, renames, deletes or signs out a job, conversation, account, login, setting or any other home path.
- API add-only: `GET /v1/profiles` is new; no existing key or closed enum the Swift or Kotlin apps decode gains a new value.
- Nothing else in obid reads the registry in PROF-1: jobs, conversations, accounts and settings stay global to the install as they were.

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

## Acceptance scenarios

- **PROFILES-1** (`profiles_1_upgrade_keeps_everything_in_the_first_profile`): an install with no `profiles.json` (and with accounts, jobs, usage and permissions already there) → after start, `GET /v1/profiles` is one profile `{id, name: "Personal", color: "#3a5a8c", default: true, waiting: false}`, `profiles.json` holds that profile, and every other home file is byte-identical; the account is still signed in. A home that already has a registry keeps it and answers it with `waiting: false`.
- **PROFILES-12** (`profiles_12_mac_stores_use_the_mac_home`): With distinct internal `mac_home` and `home` roots, startup prepares the Mac root, uses its API token, devices/pairing, gateway, relay and push stores, and measures its worktrees. Profile stores remain under `home`. Normal configuration still makes both roots equal.

**Test it when it's done (real Mac):**

1. On a Mac that has used Oby before this build (jobs and an account already there), upgrade and open nothing new: `curl -s -H "Authorization: Bearer $(cat \"$HOME/Library/Application Support/Oby/Runner/home/api-token\")" http://127.0.0.1:7777/v1/profiles` shows one Personal profile; Settings › Agents still shows the same accounts; no conversation or task is gone.
2. Restart Oby with the existing configuration. Confirm existing paired devices remain listed, the app connects with its existing API token, and a paired phone still connects and receives notifications. No files should move or require pairing again.
