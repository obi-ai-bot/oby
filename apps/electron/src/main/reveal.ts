import { basename, join } from 'node:path';

// Show in Finder's file (docs/contracts/browser-app.md, BROWSERAPP-16): a download of that name in the
// owner's Downloads, its name's last part only, so no other folder is reached; null for a name with
// none (empty, `.` or `..`, which would be Downloads itself or the folder above it).
export function downloadsFile(downloads: string, name: string): string | null {
  const last = basename(name);
  if (!last || last === '.' || last === '..') return null;
  return join(downloads, last);
}
