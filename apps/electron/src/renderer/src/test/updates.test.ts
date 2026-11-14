import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DEADLINES, EVERY, FEED, failure, next, onFeed, settings, type Event, type Update } from '../../../shared/updates';

// docs/contracts/updates.md: each test is named after its scenario. UPDATES-15 to 17 run main's
// updater (main/updates.ts) for real over an app folder of its own, with Electron and
// electron-updater stood in for, on fake timers.
type Handler = (e: unknown, ...args: unknown[]) => unknown;
const the = vi.hoisted(() => ({
  handlers: new Map<string, Handler>(), userData: '',
  // the updater's events main listens to, and the stand-in for electron-updater's autoUpdater
  events: new Map<string, (...args: unknown[]) => void>(), updater: {} as Record<string, unknown>,
}));
vi.mock('electron', () => {
  const capture = (name: string, f: Handler) => { the.handlers.set(name, f); };
  return {
    app: { getPath: () => the.userData, isPackaged: false, dock: { setBadge() {} } },
    ipcMain: { on: capture, handle: capture },
    BrowserWindow: class {}, dialog: {}, safeStorage: {}, shell: {},
    Notification: class { static isSupported = () => true; },
    powerMonitor: { on() {}, getSystemIdleState: () => 'active' },
  };
});
vi.mock('electron-updater', () => ({
  default: { get autoUpdater() { return the.updater; }, CancellationToken: class { cancel() {} } },
}));

const own = { own: true };
const emit = (name: string, ...args: unknown[]) => the.events.get(name)!(...args);
// what About shows now, from main's updates call
const shown = async () => ((await the.handlers.get('updates')!(own)) as { update: Update }).update;
const install = () => the.handlers.get('updates')!(own, 'install');

// One launch of main's updater, Update automatically on: checkForUpdates answers with `check`
async function launch(check: Promise<unknown>) {
  vi.resetModules();
  the.handlers.clear();
  the.events.clear();
  the.userData = mkdtempSync(join(tmpdir(), 'oby-updates-'));
  the.updater = {
    setFeedURL() {}, isUpdateSupported: () => true,
    on: (name: string, f: (...args: unknown[]) => void) => { the.events.set(name, f); },
    checkForUpdates: vi.fn(() => check), downloadUpdate: vi.fn(async () => []), quitAndInstall: vi.fn(),
  };
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  const { updating } = await import('../../../main/updates');
  vi.useFakeTimers();
  updating((e) => (e as { own?: boolean }).own === true, () => null);
  return { warn, quitAndInstall: the.updater.quitAndInstall as ReturnType<typeof vi.fn> };
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const idle: Update = { state: 'idle' };
const found = { version: '0.2.0', date: '2026-10-27T12:00:00.000Z' };
// the events in turn, from idle; what About shows after each, and what main did
function run(auto: boolean, events: Event[], from: Update = idle) {
  const actions: string[] = [];
  let u = from;
  for (const e of events) {
    const [v, a] = next(u, e, auto);
    u = v;
    if (a) actions.push(a);
  }
  return { u, actions };
}

describe('updates', () => {
  it('updates_1_automatic_looks_at_launch_and_every_6_hours_downloads_and_installs_at_quit', () => {
    expect(EVERY).toBe(6 * 60 * 60 * 1000);
    const { u, actions } = run(true, [{ kind: 'start' }, { kind: 'found', ...found }, { kind: 'downloaded', ...found }]);
    expect(actions).toEqual(['check']);
    expect(u).toEqual({ state: 'ready', ...found });
    expect(settings(true)).toMatchObject({ autoDownload: true, autoInstallOnAppQuit: true });
    expect(run(true, [{ kind: 'tick' }], { state: 'current' }).actions).toEqual(['check']);
  });

  it('updates_2_off_never_looks_by_itself_and_update_and_restart_downloads_then_restarts', () => {
    expect(run(false, [{ kind: 'start' }, { kind: 'tick' }]).actions).toEqual([]);
    expect(settings(false)).toMatchObject({ autoDownload: false, autoInstallOnAppQuit: false });
    const looked = run(false, [{ kind: 'check' }, { kind: 'found', ...found }]);
    expect(looked.u).toEqual({ state: 'available', ...found });
    const { u, actions } = run(false, [{ kind: 'install' }, { kind: 'downloaded', ...found }], looked.u);
    expect(actions).toEqual(['download', 'restart']);
    expect(u).toEqual({ state: 'ready', ...found });
  });

  it('updates_3_nothing_newer_is_up_to_date', () => {
    expect(run(false, [{ kind: 'check' }, { kind: 'none' }]).u).toEqual({ state: 'current' });
    expect(run(false, [{ kind: 'check' }], { state: 'current' }).actions).toEqual(['check']);
  });

  it('updates_4_a_failure_installs_nothing_and_offers_check_again', () => {
    for (const from of [{ state: 'checking' }, { state: 'downloading', restart: true, ...found }, { state: 'ready', ...found }] as Update[]) {
      const code = failure(from.state);
      expect(next(from, { kind: 'failed', code }, true)).toEqual([{ state: 'idle', error: code }, null, null]);
    }
    // the code names what failed: an error outside a look, a download or an install is a look's
    expect((['checking', 'downloading', 'ready', 'idle', 'current', 'available'] as const).map(failure))
      .toEqual(['check_failed', 'download_failed', 'install_failed', 'check_failed', 'check_failed', 'check_failed']);
    // with auto on, the next 6 hours look again
    expect(run(true, [{ kind: 'tick' }]).actions).toEqual(['check']);
  });

  it('updates_5_https_feed_on_downloads_oby_dev_never_a_lower_version_or_prerelease', () => {
    expect(FEED).toBe('https://downloads.oby.dev/electron');
    for (const auto of [true, false]) expect(settings(auto)).toMatchObject({ allowDowngrade: false, allowPrerelease: false });
    // the file electron-updater reads in the bundle names the same feed
    const yml = readFileSync(new URL('../../../../build/app-update.yml', import.meta.url), 'utf8');
    expect(yml).toMatch(/^provider: generic$/m);
    expect(yml).toMatch(new RegExp(`^url: ${FEED}$`, 'm'));
    expect(yml).toMatch(/^updaterCacheDirName: \S+$/m);
  });

  it('updates_10_an_update_never_moves_the_app', () => {
    // Squirrel.Mac moves an installed app to the update's executable name when its folder has the
    // running executable's name: the old installs' folder is this name, so they stay
    const builder = readFileSync(new URL('../../../../electron-builder.yml', import.meta.url), 'utf8');
    expect(builder).toMatch(/^ {2}extendInfo:\n {4}CFBundleExecutable: Oby Electron$/m);
  });

  it('updates_9_a_file_off_the_feeds_host_or_not_https_is_refused', () => {
    const zip = (url: string) => ({ files: [{ url }] });
    expect(() => onFeed(zip('Oby-0.2.0-arm64-mac.zip'))).not.toThrow();
    expect(() => onFeed(zip('https://downloads.oby.dev/electron/Oby-0.2.0-x64-mac.zip'))).not.toThrow();
    for (const url of [
      'https://evil.example/Oby.zip', 'http://downloads.oby.dev/electron/Oby.zip', '//evil.example/Oby.zip',
      'https://downloads.oby.dev.evil.example/Oby.zip', 'https://downloads.oby.dev:8443/Oby.zip', 'file:///tmp/Oby.zip',
    ]) expect(() => onFeed(zip(url)), url).toThrow(/refused/);
    // the legacy path too
    expect(() => onFeed({ files: [], path: 'https://evil.example/Oby.zip' })).toThrow(/refused/);
  });

  it('updates_6_update_and_restart_on_a_downloaded_update_restarts_now', () => {
    expect(run(true, [{ kind: 'install' }], { state: 'ready', ...found }).actions).toEqual(['restart']);
    // clicked while it downloads by itself: restarts once it's downloaded and checked
    const { u, actions } = run(true, [{ kind: 'install' }, { kind: 'downloaded', ...found }], { state: 'downloading', restart: false, ...found });
    expect(actions).toEqual(['restart']);
    expect(u.state).toBe('ready');
  });

  it('updates_7_turning_automatic_on_looks_now_or_downloads_what_was_found_off_stops', () => {
    expect(run(true, [{ kind: 'auto', on: true }]).actions).toEqual(['check']);
    const { u, actions } = run(true, [{ kind: 'auto', on: true }], { state: 'available', ...found });
    expect(actions).toEqual(['download']);
    expect(u).toEqual({ state: 'downloading', restart: false, ...found });
    expect(run(false, [{ kind: 'auto', on: false }, { kind: 'tick' }], { state: 'current' }).actions).toEqual([]);
  });

  it('updates_8_nothing_starts_twice_while_it_looks_or_downloads', () => {
    const busy: [Update, string | null][] = [
      [{ state: 'checking' }, 'checking'], [{ state: 'downloading', restart: false, ...found }, 'downloading'], [{ state: 'ready', ...found }, null],
    ];
    for (const [from, phase] of busy) {
      for (const e of [{ kind: 'check' }, { kind: 'tick' }, { kind: 'start' }, { kind: 'auto', on: true }] as Event[]) {
        expect(next(from, e, true)).toEqual([from, null, phase]);
      }
    }
    // a second Update and Restart while one downloads for it
    const downloading: Update = { state: 'downloading', restart: true, ...found };
    expect(next(downloading, { kind: 'install' }, false)).toEqual([downloading, null, 'downloading']);
  });

  it('updates_15_a_check_that_never_answers_ends', async () => {
    const { warn } = await launch(new Promise(() => {}));
    expect(DEADLINES.checking).toBe(90_000);
    expect(await shown()).toEqual({ state: 'checking' });
    vi.advanceTimersByTime(89_999);
    expect(await shown()).toEqual({ state: 'checking' });
    vi.advanceTimersByTime(1);
    expect(await shown()).toEqual({ state: 'idle', error: 'check_failed' });
    expect(warn).toHaveBeenCalledWith('update:', 'check_failed', 'no answer in 90 s');
    // a late answer still gets a deadline
    emit('update-available', { version: found.version, releaseDate: found.date });
    expect(await shown()).toEqual({ state: 'downloading', restart: false, ...found });
    vi.advanceTimersByTime(120_000);
    expect(await shown()).toEqual({ state: 'idle', error: 'download_failed' });
    // one timer, cleared: a look that answers leaves no deadline behind
    await the.handlers.get('updates')!(own, 'check');
    expect(await shown()).toEqual({ state: 'checking' });
    warn.mockClear();
    emit('update-not-available');
    vi.advanceTimersByTime(90_000);
    expect(await shown()).toEqual({ state: 'current' });
    expect(warn).not.toHaveBeenCalled();
  });

  it('updates_16_a_stalled_download_ends', async () => {
    const cancellationToken = { cancel: vi.fn() };
    const { warn } = await launch(Promise.resolve({ cancellationToken }));
    emit('update-available', { version: found.version, releaseDate: found.date });
    expect(await shown()).toMatchObject({ state: 'downloading' });
    // each progress event gives it its 120 s again
    vi.advanceTimersByTime(100_000);
    emit('download-progress', { percent: 10 });
    vi.advanceTimersByTime(100_000);
    emit('download-progress', { percent: 20 });
    vi.advanceTimersByTime(119_999);
    expect(await shown()).toMatchObject({ state: 'downloading' });
    expect(cancellationToken.cancel).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(await shown()).toEqual({ state: 'idle', error: 'download_failed' });
    expect(cancellationToken.cancel).toHaveBeenCalledOnce();
    expect(warn).toHaveBeenCalledWith('update:', 'download_failed', 'no answer in 120 s');
  });

  it('updates_17_a_second_install_does_nothing', async () => {
    const ready: Update = { state: 'ready', ...found };
    expect(next(ready, { kind: 'install' }, true, true)).toEqual([ready, null, 'installing']);
    const { warn, quitAndInstall } = await launch(new Promise(() => {}));
    const downloaded = async () => {
      emit('update-available', { version: found.version, releaseDate: found.date });
      emit('update-downloaded', { version: found.version, releaseDate: found.date });
      expect(await shown()).toEqual(ready);
    };
    await downloaded();
    await install();
    await install();
    expect(quitAndInstall).toHaveBeenCalledOnce();
    // Oby is still running 300 s after Update and Restart: the install failed
    vi.advanceTimersByTime(299_999);
    expect(await shown()).toEqual(ready);
    vi.advanceTimersByTime(1);
    expect(await shown()).toEqual({ state: 'idle', error: 'install_failed' });
    expect(warn).toHaveBeenCalledWith('update:', 'install_failed', 'no answer in 300 s');
    // the latch is freed: a new ready update installs on one click
    await downloaded();
    await install();
    expect(quitAndInstall).toHaveBeenCalledTimes(2);
  });
});
