# Profile storage boundaries

PROF-2 is an internal refactor serving the existing Electron and paired-device experience;
it adds no screen, setting, API field or profile selection. obid owns all storage decisions.

## Implemented: Mac-wide storage (PROF-2)

`Config.mac_home` identifies the root for the API token, publishing devices and pairing
file, gateway devices, phone relay key, push registrations and startup cleanup measurement.
Configuration loading sets it to `home`: every file stays where it was, with the same
permissions, failure handling and recovery. It is not a JSON configuration option.
Sandbox detection and the in-memory egress proxy retain their existing startup behavior.
Per-profile stores and `Jobs::open` are unchanged; splitting them belongs to PROF-3.

| Scenario | Acceptance |
| --- | --- |
| PROFILES-12 | With distinct internal `mac_home` and `home` roots, startup prepares the Mac root, uses its API token, devices/pairing, gateway, relay and push stores, and measures its worktrees. Profile stores remain under `home`. Normal configuration still makes both roots equal. Test: `profiles_12_mac_stores_use_the_mac_home`. |

## Test it when it's done

On the Mac, restart Oby with the existing configuration. Confirm existing paired devices
remain listed, the app connects with its existing API token, and a paired phone still connects
and receives notifications. No files should move or require pairing again.
