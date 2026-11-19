# The app's keys are 0600 files, as T3 Code keeps its secrets

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** this Mac's device key and owner key sealed by Electron's safeStorage, whose key macOS keeps in the login Keychain ("oby-app Safe Storage"), and Chromium's cookie encryption, which read that item at every start
- **Changed by:** Nothing
- **Contracts and scenarios:** [keychain.md](../contracts/keychain.md) (KEYCHAIN-11 to KEYCHAIN-13), [signed-words.md](../contracts/signed-words.md), [obysphere.md](../contracts/obysphere.md)

## The owner's words

> IT'S JUST HAVE TO BE SIMPLE LIKE T3CODE JUST MAKE IT WORK AS DISPATCHER AS SIMPLE AND STRAIGHTFORWARD AS POSSIBLE!

The owner's words of 2026-11-17, as the session that started this work relayed them.

## Where it is stated

- [keychain.md](../contracts/keychain.md), "The app's own keys: 0600 files, no Keychain", KEYCHAIN-11 to KEYCHAIN-13
- [signed-words.md](../contracts/signed-words.md), "Main"
- AGENTS.md, "Non-negotiable" (approvals are still signed by the device's key; only where the key is kept changed)

## Notes

The device key, which signs the owner's approvals, used to sit in a box whose key was locked in the Keychain; now it sits in a drawer only the owner's user account can open, like T3 Code's secrets. macOS asked for the password whenever Oby's signature changed (a release after a build of this Mac's, an ad hoc build, a new `npm run dev` Electron), not at each rebuild: measured on a scratch keychain, the item trusts a signed app by its designated requirement and an ad hoc one by its code hash.

**The tradeoff, plainly:**

- On a Mac that only runs a Developer ID release of Oby, the Keychain kept the key from the owner's own-lane agents, which run as him outside obid's sandbox since #2433. Now any of their shell commands can read `device-key.pem` and sign approvals as the owner.
- On a Mac that builds Oby itself, the Keychain had already stopped protecting it. The item trusted an `npm run dev` Electron by its hash, and any copy of that binary runs any script. The Mac's Apple Development key also signed code without asking, so an agent could sign its own Oby.

The owner chose no prompts over that protection. Builds from source keep a folder of their own (`oby-app-dev`), so they never read or move the installed Oby's keys. A key already sealed in the Keychain is moved once, keeping the devices it paired and the Obyspheres it joined; that one read asks only if the item doesn't trust the new build's signature.
