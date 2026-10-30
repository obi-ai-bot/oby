// Oby's own updates (docs/contracts/updates.md): what Settings › About shows, and what main's
// updater (main/updates.ts) does next. No Electron here, so its tests run anywhere.

// Where the builds are (scripts/package.sh writes latest-mac.yml and the zips for it): https only
export const FEED = 'https://downloads.oby.dev/electron';
// with Update automatically on, how often Oby looks (Sangam's design/app: every 6 hours)
export const EVERY = 6 * 60 * 60 * 1000;

type Found = { version: string; date?: string };
// idle: not looked since launch, or the last look failed (About: Check for Updates)
// current: Oby is up to date
// available: found, not downloaded (Update automatically off): Update and Restart downloads it
// downloading: restart, the owner clicked Update and Restart and it restarts once it's checked
// ready: downloaded, its checksum checked; installs when Oby quits, or now with Update and Restart,
//   once Squirrel.Mac checked its signature
export type Update =
  | { state: 'idle' } | { state: 'checking' } | { state: 'current' }
  | ({ state: 'available' } & Found)
  | ({ state: 'downloading'; restart: boolean } & Found)
  | ({ state: 'ready' } & Found);

export type Event =
  // Oby started, or EVERY passed
  | { kind: 'start' } | { kind: 'tick' }
  // the owner: Check for Updates, Update and Restart, Update automatically turned
  | { kind: 'check' } | { kind: 'install' } | { kind: 'auto'; on: boolean }
  // the updater: nothing newer, a newer one (found), it downloaded and checked it, it failed
  | { kind: 'none' } | ({ kind: 'found' } & Found) | ({ kind: 'downloaded' } & Found) | { kind: 'failed' };

// what main asks the updater for
export type Action = 'check' | 'download' | 'restart' | null;

const found = (e: Found): Found => ({ version: e.version, ...(e.date ? { date: e.date } : {}) });

// The update decision: from what About shows, an event, and Update automatically, what it shows
// next and what main does. auto off means Oby never looks by itself: only the owner's click goes
// out. A downloaded update installs only after the updater checked its checksum and Apple's
// signature (main/updates.ts), never from here.
export function next(u: Update, e: Event, auto: boolean): [Update, Action] {
  const busy = u.state === 'checking' || u.state === 'downloading' || u.state === 'ready';
  switch (e.kind) {
    case 'start': case 'tick':
      return auto && !busy ? [{ state: 'checking' }, 'check'] : [u, null];
    case 'check':
      return busy ? [u, null] : [{ state: 'checking' }, 'check'];
    case 'auto':
      if (!e.on) return [u, null];
      if (u.state === 'available') return [{ state: 'downloading', restart: false, ...found(u) }, 'download'];
      return busy ? [u, null] : [{ state: 'checking' }, 'check'];
    case 'install':
      if (u.state === 'ready') return [u, 'restart'];
      if (u.state === 'available') return [{ state: 'downloading', restart: true, ...found(u) }, 'download'];
      if (u.state === 'downloading') return [{ ...u, restart: true }, null];
      return [u, null];
    case 'none':
      return [{ state: 'current' }, null];
    case 'found':
      // with auto on the updater downloads it by itself (autoDownload)
      return [auto ? { state: 'downloading', restart: false, ...found(e) } : { state: 'available', ...found(e) }, null];
    case 'downloaded':
      return [{ state: 'ready', ...found(e) }, u.state === 'downloading' && u.restart ? 'restart' : null];
    case 'failed':
      return [{ state: 'idle' }, null];
  }
}

// electron-updater's settings for Update automatically: never an older or a pre-release build,
// downloads and installs at quit only when it's on; always the whole zip (package.sh makes no
// blockmaps for a part of one)
export const settings = (auto: boolean) => ({
  autoDownload: auto, autoInstallOnAppQuit: auto, allowDowngrade: false, allowPrerelease: false, disableDifferentialDownload: true,
});
