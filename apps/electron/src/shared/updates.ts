// Oby's own updates (docs/contracts/updates.md): what Settings › About shows, and what main's
// updater (main/updates.ts) does next. No Electron here, so its tests run anywhere.

// Where the builds are (scripts/package.sh writes latest-mac.yml and the zips for it): https only
export const FEED = 'https://downloads.oby.dev/electron';
// with Update automatically on, how often Oby looks (Sangam's design/app: every 6 hours)
export const EVERY = 6 * 60 * 60 * 1000;

// The busy phases main holds a deadline for: installing is `ready` after Update and Restart
export type BusyState = 'checking' | 'downloading' | 'installing';
// How long each may last before it fails (ms), so About never stays busy
export const DEADLINES: Record<BusyState, number> = {
  // a look with no answer
  checking: 90_000,
  // no progress event since the last one, or since the download started
  downloading: 120_000,
  // Update and Restart clicked, and Oby is still running
  installing: 300_000,
};

// why a look, a download or an install ended with nothing (main's log, idle's error)
export type Failure = 'check_failed' | 'download_failed' | 'install_failed';

type Found = { version: string; date?: string };
// idle: not looked since launch, or the last look failed, error saying how (About: Check for Updates)
// current: Oby is up to date
// available: found, not downloaded (Update automatically off): Update and Restart downloads it
// downloading: restart, the owner clicked Update and Restart and it restarts once it's checked
// ready: downloaded, its checksum checked; installs when Oby quits, or now with Update and Restart,
//   once Squirrel.Mac checked its signature
export type Update =
  | { state: 'idle'; error?: Failure } | { state: 'checking' } | { state: 'current' }
  | ({ state: 'available' } & Found)
  | ({ state: 'downloading'; restart: boolean } & Found)
  | ({ state: 'ready' } & Found);

export type Event =
  // Oby started, or EVERY passed
  | { kind: 'start' } | { kind: 'tick' }
  // the owner: Check for Updates, Update and Restart, Update automatically turned
  | { kind: 'check' } | { kind: 'install' } | { kind: 'auto'; on: boolean }
  // the updater: nothing newer, a newer one (found), it downloaded and checked it, it failed
  | { kind: 'none' } | ({ kind: 'found' } & Found) | ({ kind: 'downloaded' } & Found) | { kind: 'failed'; code: Failure };

// what main asks the updater for
export type Action = 'check' | 'download' | 'restart' | null;

const found = (e: Found): Found => ({ version: e.version, ...(e.date ? { date: e.date } : {}) });

// The failure for what main was doing (a state About shows, or a busy phase whose deadline fired):
// an error outside a look, a download or an install counts as a look's
export function failure(s: Update['state'] | BusyState): Failure {
  if (s === 'downloading') return 'download_failed';
  return s === 'ready' || s === 'installing' ? 'install_failed' : 'check_failed';
}

// The update decision: from what About shows, an event, Update automatically and whether Update
// and Restart already ran this session, what it shows next, what main does, and the busy phase
// whose deadline main holds after it. auto off means Oby never looks by itself: only the owner's
// click goes out. A downloaded update installs only after the updater checked its checksum and
// Apple's signature (main/updates.ts), never from here.
export function next(u: Update, e: Event, auto: boolean, installing = false): [Update, Action, BusyState | null] {
  const [v, action] = decide(u, e, auto, installing);
  const phase = v.state === 'checking' || v.state === 'downloading' ? v.state
    : v.state === 'ready' && (installing || action === 'restart') ? 'installing' : null;
  return [v, action, phase];
}

function decide(u: Update, e: Event, auto: boolean, installing: boolean): [Update, Action] {
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
      // once per session: a second click while it installs does nothing
      if (u.state === 'ready') return [u, installing ? null : 'restart'];
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
      return [{ state: 'idle', error: e.code }, null];
  }
}

// latest-mac.yml's files, resolved against the feed as electron-updater resolves them: each one https
// on the feed's own host, or the update is refused (thrown: the updater's 'error', About offers
// Check for Updates again). Whoever writes the feed can't send Macs to another host.
export function onFeed(info: { files?: { url: string }[]; path?: string }) {
  const feed = new URL(`${FEED}/`);
  for (const u of [...(info.files ?? []).map((f) => f.url), ...(info.path ? [info.path] : [])]) {
    const url = new URL(u, feed);
    if (url.protocol !== 'https:' || url.host !== feed.host) throw new Error(`refused an update file off ${feed.host}: ${url.href}`);
  }
}

// electron-updater's settings for Update automatically: never a lower version or a pre-release,
// downloads and installs at quit only when it's on; always the whole zip (package.sh makes no
// blockmaps for a part of one)
export const settings = (auto: boolean) => ({
  autoDownload: auto, autoInstallOnAppQuit: auto, allowDowngrade: false, allowPrerelease: false, disableDifferentialDownload: true,
});
