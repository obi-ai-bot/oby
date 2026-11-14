import { type BrowserWindow } from 'electron';
import updater from 'electron-updater';
import type { CancellationToken } from 'electron-updater';
import {
  DEADLINES, EVERY, FEED, failure, next, onFeed, settings as updaterSettings, type BusyState, type Event, type Update,
} from '../shared/updates';
import { settings, writeSettings } from './settings';
import type { Trusted } from './obid';
import { ipc } from './ipc';
import { oneOf, orUndefined } from '../shared/decode';

// Oby updates itself from FEED (docs/contracts/updates.md) with electron-updater: it downloads the
// zip for this Mac's architecture and refuses it unless its sha512 is latest-mac.yml's, and
// Squirrel.Mac installs it only if its code signature meets this app's own designated requirement
// (Deskree's Developer ID, this bundle ID); a file anywhere but FEED's host isn't fetched (onFeed).
// What to do when is shared/updates.ts's; this carries it out, and tells About (Settings) what it
// shows.
const { autoUpdater } = updater;

let update: Update = { state: 'idle' };
let auto = true;
let window: () => BrowserWindow | null = () => null;
// Update and Restart ran this session: set until the state leaves ready (an install that failed)
let installing = false;
// the one deadline main holds, for the busy phase armed
let armed: BusyState | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
// the current download's, so a stalled one is abandoned
let download: CancellationToken | null = null;

function arm(phase: BusyState | null) {
  clearTimeout(timer);
  armed = phase;
  timer = phase ? setTimeout(() => expire(phase), DEADLINES[phase]) : undefined;
}

function expire(phase: BusyState) {
  const code = failure(phase);
  console.warn('update:', code, `no answer in ${DEADLINES[phase] / 1000} s`);
  if (phase === 'downloading') download?.cancel();
  on({ kind: 'failed', code });
}

function on(e: Event) {
  const [u, action, phase] = next(update, e, auto, installing);
  update = u;
  if (u.state !== 'ready') installing = false;
  if (phase !== armed) arm(phase);
  const w = window();
  if (w && !w.isDestroyed()) w.webContents.send('update', update);
  // a failure comes as the updater's 'error'; null: it isn't running (not packaged), nothing to look for.
  // With Update automatically on, the download starts inside the look, under the result's token
  // (cancelled, its promise rejects: caught).
  if (action === 'check') {
    autoUpdater.checkForUpdates().then((r) => {
      if (!r) return on({ kind: 'failed', code: failure(update.state) });
      if (r.cancellationToken) download = r.cancellationToken;
      r.downloadPromise?.catch(() => {});
    }, () => {});
  } else if (action === 'download') {
    download = new updater.CancellationToken();
    autoUpdater.downloadUpdate(download).catch(() => {});
  } else if (action === 'restart') {
    installing = true;
    autoUpdater.quitAndInstall();
  }
}

export function updating(trusted: Trusted, current: () => BrowserWindow | null) {
  window = current;
  auto = settings().updatesAuto !== false;
  autoUpdater.setFeedURL({ provider: 'generic', url: FEED });
  Object.assign(autoUpdater, updaterSettings(auto));
  // before an update counts as found: its files on FEED's host, then electron-updater's own check
  const supported = autoUpdater.isUpdateSupported;
  autoUpdater.isUpdateSupported = (info) => { onFeed(info); return supported(info); };
  const date = (d: unknown) => (typeof d === 'string' ? { date: d } : {});
  autoUpdater.on('update-not-available', () => on({ kind: 'none' }));
  autoUpdater.on('update-available', (i) => on({ kind: 'found', version: i.version, ...date(i.releaseDate) }));
  autoUpdater.on('update-downloaded', (i) => on({ kind: 'downloaded', version: i.version, ...date(i.releaseDate) }));
  // a download that moves gets its full deadline again
  autoUpdater.on('download-progress', () => { if (armed === 'downloading') arm('downloading'); });
  autoUpdater.on('error', (err) => {
    const code = failure(update.state);
    console.warn('update:', code, err.message);
    on({ kind: 'failed', code });
  });
  // About: what it shows and Update automatically; with 'check' or 'install', the owner's click,
  // with true or false, Update automatically turned
  ipc(trusted).handle('updates', [orUndefined(oneOf('check', 'install', true, false))], (_e, ask) => {
    if (ask === 'check' || ask === 'install') on({ kind: ask });
    else if (typeof ask === 'boolean' && ask !== auto) {
      auto = ask;
      writeSettings({ updatesAuto: auto });
      Object.assign(autoUpdater, updaterSettings(auto));
      on({ kind: 'auto', on: auto });
    }
    return { auto, update };
  }, null);
  on({ kind: 'start' });
  setInterval(() => on({ kind: 'tick' }), EVERY);
}
