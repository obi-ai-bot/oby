import { dirname, join } from 'node:path';

// The name and app folder this copy of Oby keeps, set in main before anything reads them. The folder
// holds this Mac's device and owner keys (main/device.ts), and its settings.
//
// Only the packaged Oby keeps the installed one's, `oby-app` (package.json's name). A scratch install
// keeps its own beside its obid's home (scripts/run-dev.sh). The app run from source (`npm run dev`)
// keeps `oby-app-dev`, so a build being worked on never reads or moves the installed Oby's keys
// (docs/contracts/keychain.md, KEYCHAIN-11).
export const DEV_NAME = 'oby-app-dev';
export const SCRATCH_NAME = 'oby-app-scratch';

type Copy = { packaged: boolean; name: string | undefined; scratchHome: string | null; appData: () => string };

// null: this copy keeps what it has (the packaged Oby's `oby-app`, or a test's own launcher's name)
export function ownFolder({ packaged, name, scratchHome, appData }: Copy): { name: string; userData: string } | null {
  if (scratchHome) return { name: SCRATCH_NAME, userData: join(dirname(scratchHome), 'app-data') };
  if (!packaged && name === 'oby-app') return { name: DEV_NAME, userData: join(appData(), DEV_NAME) };
  return null;
}
