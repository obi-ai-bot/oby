# The app's keys stay in the Keychain, and Oby is signed the same way build after build

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** `npm run dev` opening the installed Oby's own app folder and Keychain item ("oby-app Safe Storage"); `scripts/run-dev.sh` signing ad hoc by itself when this Mac had no signing identity
- **Changed by:** Nothing
- **Contracts and scenarios:** [keychain.md](../contracts/keychain.md) (KEYCHAIN-11, KEYCHAIN-12), [signed-words.md](../contracts/signed-words.md) (the device key it signs with)

## The owner's words

> macOS keeps asking for my login password for "oby-app Safe Storage". Make it stop, the way T3 Code does.

The owner's words of 2026-11-17, as the session that started this work relayed them. Not an exact quote.

## Where it is stated

- [keychain.md](../contracts/keychain.md), "The app's own item" and KEYCHAIN-11, KEYCHAIN-12
- [build-and-run.md](../build-and-run.md), "Today's dev on this Mac"
- AGENTS.md, "Non-negotiable" (approvals come only from the owner's own apps, signed by the device's key)

## Notes

### What was happening

Think of the Keychain item as a locked drawer with a guest list on the door. The Oby app keeps its device key, the key that signs the owner's approvals, in a box in its folder, and the box's key sits in that drawer. A program on the guest list opens the drawer without asking; anyone else makes macOS ask the owner for his password. A signed app is on the list by name and signature (its bundle ID and its certificate), and that stays the same from one build to the next. An app signed ad hoc is on the list only by the fingerprint of its exact code, which changes with every build.

So the prompt doesn't come from rebuilding. It comes from a **new signer**. Measured on 2026-11-17, never reading the owner's items:

- securityd's own log (`log show`, `process == "securityd"`) had one Keychain prompt for Oby since the log's start that day: `/Applications/Oby.app` asking for "oby-app Safe Storage", answered "always allow". The item's access list at that moment held two programs. One was an `npm run dev` Electron from a worktree's `node_modules`, listed by code hash; it made the item, since `npm run dev` runs under package.json's name `oby-app` too. The other was the old Developer ID "Oby Electron.app". The new Oby was signed by this Mac's Apple Development certificate, another team, so it was a new guest.
- A scratch keychain and a small probe signed like Oby, with user interaction off (`SecKeychainSetUserInteractionAllowed(false)`, so a miss returns an error instead of a dialog):

  | Who reads the item | Result |
  |---|---|
  | the build that made it | read, no prompt |
  | a rebuild with other code, signed by the same Apple Development identity, at the same path or any other | read, no prompt |
  | an ad hoc build (as `npm run dev`'s Electron, or `run-dev.sh` with no identity) | refused (`-25293`): a prompt |
  | the Apple Development build, on an item an ad hoc build made | refused: a prompt, until Always Allow |
  | after Always Allow for the Apple Development build, a rebuild of it | read, no prompt |
  | a copy of the allowed ad hoc binary, run from anywhere | read, no prompt |

  `security dump-keychain -a` on the scratch keychain shows why: the access list stores a signed app's designated requirement (`identifier … and anchor apple generic and certificate leaf[subject.CN] = …`), and an ad hoc one's `cdhash`.

What still made macOS ask, then:

- switching between a release (Developer ID) and a build of this Mac (Apple Development): once for each new signer, and the earlier ones stay allowed;
- `run-dev.sh` falling back to ad hoc on a Mac with no identity: a new app at every build;
- `npm run dev` on each new Electron version, because it shared the installed Oby's item;
- choosing Allow rather than Always Allow, which asks again at every start. Chromium's cookie encryption (the `enableCookieEncryption` fuse) reads the key at every start, before anything is signed.

### What protects the device key today, and what each option keeps

On a Mac with only a release of Oby, the drawer is a real lock. Only code signed with Deskree's Developer ID and the bundle ID `com.deskree.oby.electron` gets in without asking. The owner's agents run as his own user and, since #2433, outside obid's sandbox, but they can't open it:

- `security find-generic-password -s "oby-app Safe Storage" -w` makes macOS ask the owner;
- they can't make Oby run their code: the fuses refuse `ELECTRON_RUN_AS_NODE`, Node options and inspector flags, and any app but its own checked asar; the hardened runtime, with only `allow-jit`, refuses injected libraries and a debugger.

Ranked:

1. **(a) Sign the same way every time. Chosen and built.** It keeps the lock and ends the prompts for rebuilds:
   - `run-dev.sh` never signs ad hoc by itself (KEYCHAIN-12);
   - `npm run dev` keeps an item and folder of its own, `oby-app-dev`, so Electron's own binary, which runs any script and is listed by its hash, is never again added to the installed Oby's list (KEYCHAIN-11).

   One prompt remains for each new signer, and Always Allow ends it.
2. **(c) A hybrid**, such as a 0600 file for builds from source and the Keychain for releases. A dev build's device key is registered with the owner's real obid and signs real approvals, so a file only the agents could read would hand them those approvals. Not built.
3. **(b) T3 Code's way: the keys in a 0600 file in the app's folder, no Keychain.** It ends every prompt, but any of the owner's own-lane agents could `cat` the device key and sign Confirms, Publishes and words as the owner. That breaks a non-negotiable: approvals come only from the owner's own apps. T3 Code's 0600 files hold server secrets, not a key whose signature means "the owner said yes". Not built. It's the owner's to decide, and choosing it would change AGENTS.md's non-negotiable first.

A later option that keeps the lock and never asks at all: the data-protection keychain, with an item bound to the app's `keychain-access-groups` entitlement, which macOS grants only with a provisioning profile, through a small native module in main. Not built here: it needs the team's profile in the release, and its own design for builds from source.

### What this Mac still lets an agent do (for the owner)

On a Mac that builds Oby itself, the lock is weaker than on a release-only Mac, and no code here can change that:

- **The access list already held an `npm run dev` Electron by its code hash.** Any copy of that exact Electron, run by any program with a script of its own under the name `oby-app`, opens the item without asking (the last row above, shown on a scratch keychain). To close it, the owner can do one of two things: remove that Electron from the item's Access Control in Keychain Access, or delete the item. Deleting it means Oby makes a new device key and pairs it again on his click, as KEYCHAIN-11 describes for a new Mac.
- **This Mac's Apple Development signing key signs without asking.** `run-dev.sh` uses it unattended, and so could an agent: it could sign its own `com.deskree.oby.electron` and pass the list. To close it, set that key's access in Keychain Access to ask before `codesign` uses it. The cost is one prompt for each build `run-dev.sh` signs.
