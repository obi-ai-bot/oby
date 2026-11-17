import { dirname, join } from 'node:path';

// The name and app folder this copy of Oby keeps, set in main before anything reads them. The name
// names safeStorage's Keychain item ("<name> Safe Storage"), which seals this Mac's device and owner
// keys (main/device.ts), and the folder holds those sealed keys.
//
// Only the packaged Oby keeps the installed one's, `oby-app` (package.json's name). A scratch install
// keeps its own beside its obid's home (scripts/run-dev.sh). The app run from source (`npm run dev`)
// keeps `oby-app-dev`: it runs on Electron's own binary, signed ad hoc, which runs any script it's
// given, and the Keychain trusts an ad hoc binary by its hash wherever a copy of it is. Once it's on
// the installed item's access list, any program could open the installed Oby's keys with it, and
// each new Electron would be asked for again (docs/contracts/keychain.md, KEYCHAIN-11).
export const DEV_NAME = 'oby-app-dev';
export const SCRATCH_NAME = 'oby-app-scratch';

type Copy = { packaged: boolean; name: string; scratchHome: string | null; appData: string };

// null: this copy keeps what it has (the packaged Oby's `oby-app`, or a test's own launcher's name)
export function ownFolder({ packaged, name, scratchHome, appData }: Copy): { name: string; userData: string } | null {
  if (scratchHome) return { name: SCRATCH_NAME, userData: join(dirname(scratchHome), 'app-data') };
  if (!packaged && name === 'oby-app') return { name: DEV_NAME, userData: join(appData, DEV_NAME) };
  return null;
}
