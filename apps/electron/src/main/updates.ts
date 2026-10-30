import { ipcMain, type BrowserWindow } from 'electron';
import { constants } from 'node:fs';
import updater from 'electron-updater';
import { EVERY, FEED, next, settings as updaterSettings, type Event, type Update } from '../shared/updates';
import { writeOwn } from './device';
import { SETTINGS, settings } from './notify';
import type { Trusted } from './obid';

// Oby updates itself from FEED (docs/contracts/updates.md) with electron-updater: it downloads the
// zip for this Mac's architecture and refuses it unless its sha512 is latest-mac.yml's, and
// Squirrel.Mac installs it only if its code signature meets this app's own designated requirement
// (Deskree's Developer ID, this bundle ID). What to do when is shared/updates.ts's; this carries it
// out, and tells About (Settings) what it shows.
const { autoUpdater } = updater;

let update: Update = { state: 'idle' };
let auto = true;
let window: () => BrowserWindow | null = () => null;

function on(e: Event) {
  const [u, action] = next(update, e, auto);
  update = u;
  const w = window();
  if (w && !w.isDestroyed()) w.webContents.send('update', update);
  // a failure comes as the updater's 'error'; null: it isn't running (not packaged), nothing to look for
  if (action === 'check') autoUpdater.checkForUpdates().then((r) => { if (!r) on({ kind: 'failed' }); }, () => {});
  else if (action === 'download') autoUpdater.downloadUpdate().catch(() => {});
  else if (action === 'restart') autoUpdater.quitAndInstall();
}

export function updating(trusted: Trusted, current: () => BrowserWindow | null) {
  window = current;
  auto = settings().updatesAuto !== false;
  autoUpdater.setFeedURL({ provider: 'generic', url: FEED });
  Object.assign(autoUpdater, updaterSettings(auto));
  const date = (d: unknown) => (typeof d === 'string' ? { date: d } : {});
  autoUpdater.on('update-not-available', () => on({ kind: 'none' }));
  autoUpdater.on('update-available', (i) => on({ kind: 'found', version: i.version, ...date(i.releaseDate) }));
  autoUpdater.on('update-downloaded', (i) => on({ kind: 'downloaded', version: i.version, ...date(i.releaseDate) }));
  autoUpdater.on('error', (err) => { console.warn('update:', err.message); on({ kind: 'failed' }); });
  // About: what it shows and Update automatically; with 'check' or 'install', the owner's click,
  // with true or false, Update automatically turned
  ipcMain.handle('updates', (e, ask: unknown) => {
    if (!trusted(e)) return null;
    if (ask === 'check' || ask === 'install') on({ kind: ask });
    else if (typeof ask === 'boolean' && ask !== auto) {
      auto = ask;
      writeOwn(SETTINGS(), JSON.stringify({ ...settings(), updatesAuto: auto }), constants.O_TRUNC);
      Object.assign(autoUpdater, updaterSettings(auto));
      on({ kind: 'auto', on: auto });
    }
    return { auto, update };
  });
  on({ kind: 'start' });
  setInterval(() => on({ kind: 'tick' }), EVERY);
}
