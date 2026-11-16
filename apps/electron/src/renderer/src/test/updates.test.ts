import { spawnSync } from 'node:child_process';
import { generateKeyPairSync, sign, type KeyObject } from 'node:crypto';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { load } from 'js-yaml';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DEADLINES, EVERY, FEED, FEED_PUBLIC_KEY, failure, next, onFeed, settings, verifyFeed, type Event, type Update,
} from '../../../shared/updates';

// docs/contracts/updates.md: each test is named after its scenario. UPDATES-11, 14 and 15 to 17 run
// main's updater (main/updates.ts) for real over an app folder of its own, with Electron (its net
// too: the feed main reads again) and electron-updater stood in for, on fake timers. Every key a
// feed is signed with here is made while the test runs.
type Handler = (e: unknown, ...args: unknown[]) => unknown;
const the = vi.hoisted(() => ({
  handlers: new Map<string, Handler>(), userData: '', fetch: (async () => new Response(null, { status: 404 })) as (...a: unknown[]) => Promise<Response>,
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
    net: { fetch: (...a: unknown[]) => the.fetch(...a) },
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

// One launch of main's updater, Update automatically on unless userData's settings.json says
// otherwise: checkForUpdates answers with `check`
async function launch(check: Promise<unknown>, userData = mkdtempSync(join(tmpdir(), 'oby-updates-'))) {
  vi.resetModules();
  the.handlers.clear();
  the.events.clear();
  the.userData = userData;
  // electron-updater's own check, which main's wrapper calls last
  const original = vi.fn(() => true);
  the.updater = {
    setFeedURL() {}, isUpdateSupported: original,
    on: (name: string, f: (...args: unknown[]) => void) => { the.events.set(name, f); },
    checkForUpdates: vi.fn(() => check), downloadUpdate: vi.fn(async () => []), quitAndInstall: vi.fn(),
  };
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  const { updating } = await import('../../../main/updates');
  vi.useFakeTimers();
  updating((e) => (e as { own?: boolean }).own === true, () => null);
  return {
    warn, quitAndInstall: the.updater.quitAndInstall as ReturnType<typeof vi.fn>, original,
    // main's wrapper, as electron-updater calls it before an update counts as found
    supported: (info: unknown) => (the.updater.isUpdateSupported as (i: unknown) => Promise<boolean>)(info),
  };
}

// latest-mac.yml as package.sh writes it before it's signed, and signed by key as sign-feed.mjs signs it
const unsigned = (f: { version?: string; issued?: string; minimumVersion?: string } = {}) => {
  const { version = '0.2.24', issued = '2026-11-15T10:00:00Z', minimumVersion = '0.2.23' } = f;
  return `version: ${version}\nfiles:\n  - url: Oby-${version}-arm64-mac.zip\n    sha512: c2hh\n    size: 1\n`
    + `path: Oby-${version}-arm64-mac.zip\nsha512: c2hh\nreleaseDate: '2026-11-15T10:00:00.000Z'\n`
    + `issued: '${issued}'\nminimumVersion: '${minimumVersion}'\n`;
};
const signatureOf = (text: string, key: KeyObject) => `signature: '${sign(null, Buffer.from(text), key).toString('base64')}'\n`;
const signed = (text: string, key: KeyObject) => Buffer.from(text + signatureOf(text, key));
// what electron-updater parsed from the same bytes
const parsed = (raw: Buffer | string) => load(raw.toString());
const pair = generateKeyPairSync('ed25519');
const other = generateKeyPairSync('ed25519');
const check = (raw: Buffer, lastIssued?: unknown, info: unknown = parsed(raw)) => verifyFeed(raw, info, lastIssued, pair.publicKey);
// main reads this as latest-mac.yml
const serve = (raw: Buffer | null, status = 200) => {
  the.fetch = vi.fn(async () => new Response(raw && new Uint8Array(raw), { status }));
  return the.fetch;
};

// scripts/sign-feed.mjs on its own in a folder of its own, whose src/shared/updates.ts carries
// carried as FEED_KEY; keyFile, when set, as OBY_FEED_KEY_FILE
function signer(carried: KeyObject, keyFile: ((app: string) => string) | null, ...args: string[]) {
  const app = mkdtempSync(join(tmpdir(), 'oby-signer-'));
  try {
    mkdirSync(join(app, 'scripts'));
    mkdirSync(join(app, 'src/shared'), { recursive: true });
    cpSync(resolve(__dirname, '../../../../scripts/sign-feed.mjs'), join(app, 'scripts/sign-feed.mjs'));
    const der = carried.export({ type: 'spki', format: 'der' }).toString('base64');
    writeFileSync(join(app, 'src/shared/updates.ts'), `// the app's key\nexport const FEED_KEY = '${der}';\n`);
    const env: Record<string, string> = { PATH: process.env.PATH ?? '/usr/bin:/bin' };
    if (keyFile) env.OBY_FEED_KEY_FILE = keyFile(app);
    const { status, stdout, stderr } = spawnSync(process.execPath, [join(app, 'scripts/sign-feed.mjs'), ...args], { encoding: 'utf8', env });
    return { status, words: stdout + stderr };
  } finally {
    rmSync(app, { recursive: true, force: true });
  }
}
// a private key's file in a folder of its own outside any app folder, removed after each test
const keyFolders: string[] = [];
const keyFileOf = (key: KeyObject, folder = mkdtempSync(join(tmpdir(), 'oby-feed-key-'))) => {
  keyFolders.push(folder);
  const file = join(folder, 'feed.pem');
  writeFileSync(file, key.export({ type: 'pkcs8', format: 'pem' }), { mode: 0o600 });
  return file;
};

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.doUnmock('../../../shared/updates');
  for (const folder of keyFolders.splice(0)) rmSync(folder, { recursive: true, force: true });
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

  it('updates_11_an_unsigned_or_badly_signed_feed_is_refused', async () => {
    // the owner's key, as the issue's comment gives it
    const owner = '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA2PAZCZ8StseJm63/g0XJWsvq0r0iprM7HHTd3SRkihI=\n-----END PUBLIC KEY-----\n';
    expect(FEED_PUBLIC_KEY.export({ type: 'spki', format: 'pem' })).toBe(owner);
    expect(FEED_PUBLIC_KEY.asymmetricKeyType).toBe('ed25519');

    const text = unsigned();
    expect(check(signed(text, pair.privateKey))).toEqual({ issued: '2026-11-15T10:00:00Z' });
    const refused = (raw: Buffer, why: string, info: unknown = parsed(raw)) => expect(() => check(raw, undefined, info), why).toThrow(`refused the feed: ${why}`);
    refused(Buffer.from(text), 'no signature');
    refused(Buffer.from(text.slice(0, -1)), 'no signature');
    refused(signed(text, other.privateKey), 'bad signature');
    // one byte changed after it was signed
    refused(Buffer.from(signed(text, pair.privateKey).toString().replace('size: 1', 'size: 2')), 'bad signature');
    // a signature line that isn't the last, or a second one above it
    refused(Buffer.from(`${text}${signatureOf(text, pair.privateKey)}size: 2\n`), 'no signature');
    const twice = text + signatureOf(text, pair.privateKey);
    refused(signed(twice, pair.privateKey), 'no signature', parsed(text));
    // electron-updater acting on other bytes than the ones checked
    const raw = signed(text, pair.privateKey);
    refused(raw, 'not the feed that was checked', parsed(signed(unsigned({ version: '0.2.25' }), pair.privateKey)));
    refused(raw, 'not the feed that was checked', { ...(parsed(raw) as object), path: 'Oby-0.2.0-arm64-mac.zip' });
    // what it says names no key, signature or feed text
    try { check(signed(text, other.privateKey)); } catch (e) { expect((e as Error).message).toBe('refused the feed: bad signature'); }

    // main: the wrapper reads latest-mac.yml again and refuses it before electron-updater's own check
    const { supported, original } = await launch(new Promise(() => {}));
    const fetched = serve(Buffer.from(text));
    await expect(supported(parsed(text))).rejects.toThrow('refused the feed: no signature');
    expect(fetched).toHaveBeenCalledWith(`${FEED}/latest-mac.yml`, { cache: 'no-store' });
    serve(null, 404);
    await expect(supported(parsed(text))).rejects.toThrow('refused the feed: couldn\'t read it (404)');
    // the real key isn't the test's: a feed only the test's key signed is refused too
    serve(raw);
    await expect(supported(parsed(raw))).rejects.toThrow('refused the feed: bad signature');
    expect(original).not.toHaveBeenCalled();

    // the signer: it stops without the key the app carries, and signs what verifyFeed accepts
    expect(signer(pair.publicKey, null).words).toMatch(/error: .*OBY_FEED_KEY_FILE/);
    expect(signer(pair.publicKey, null).status).not.toBe(0);
    const missing = signer(pair.publicKey, () => join(tmpdir(), 'oby-no-such-feed-key.pem'));
    expect(missing.status).not.toBe(0);
    expect(missing.words).toMatch(/^error: .*isn't there/m);
    const mismatched = signer(pair.publicKey, () => keyFileOf(other.privateKey));
    expect(mismatched.status).not.toBe(0);
    expect(mismatched.words).toContain('error: the feed key doesn\'t match the key the app carries');
    const inside = signer(pair.publicKey, (app) => keyFileOf(pair.privateKey, app));
    expect(inside.status).not.toBe(0);
    expect(inside.words).toContain('error: the feed key is inside the repository');
    const notKey = signer(pair.publicKey, () => { const f = keyFileOf(pair.privateKey); writeFileSync(f, 'not a key'); return f; });
    expect(notKey.words).toContain('error: the file OBY_FEED_KEY_FILE names isn\'t a readable private key');
    expect(signer(pair.publicKey, () => keyFileOf(pair.privateKey))).toEqual({ status: 0, words: '==> The feed key matches the app\'s\n' });
    const work = mkdtempSync(join(tmpdir(), 'oby-feed-'));
    keyFolders.push(work);
    writeFileSync(join(work, 'unsigned.yml'), text);
    const key = keyFileOf(pair.privateKey);
    const made = signer(pair.publicKey, () => key, join(work, 'unsigned.yml'), join(work, 'latest-mac.yml'));
    expect(made.status, made.words).toBe(0);
    const out = readFileSync(join(work, 'latest-mac.yml'));
    expect(out.toString().startsWith(text)).toBe(true);
    expect(check(out)).toEqual({ issued: '2026-11-15T10:00:00Z' });
    // never what the key holds
    expect(made.words).not.toContain(readFileSync(key, 'utf8').split('\n')[1]);
    // an unsigned file signed already isn't signed again
    const again = signer(pair.publicKey, () => key, join(work, 'latest-mac.yml'), join(work, 'twice.yml'));
    expect(again.status).not.toBe(0);
    expect(again.words).toContain('is signed already');
  });

  it('updates_12_a_replayed_feed_is_refused', () => {
    const raw = signed(unsigned({ issued: '2026-11-15T10:00:00Z' }), pair.privateKey);
    expect(() => check(raw, '2026-11-15T10:00:01Z')).toThrow('refused the feed: a replay');
    expect(() => check(raw, '2026-12-01T00:00:00Z')).toThrow('refused the feed: a replay');
    // the same issued again, a later one, or none kept
    expect(check(raw, '2026-11-15T10:00:00Z')).toEqual({ issued: '2026-11-15T10:00:00Z' });
    expect(check(raw, '2026-11-15T09:59:59Z')).toEqual({ issued: '2026-11-15T10:00:00Z' });
    for (const none of [undefined, null, 42, '', 'yesterday', '2026-13-45T99:99:99Z']) expect(check(raw, none), String(none)).toEqual({ issued: '2026-11-15T10:00:00Z' });
  });

  it('updates_13_a_version_under_the_floor_is_refused', () => {
    const feed = (version: string, minimumVersion: string) => signed(unsigned({ version, minimumVersion }), pair.privateKey);
    expect(() => check(feed('0.2.9', '0.2.10'))).toThrow('refused the feed: under its minimum version');
    expect(() => check(feed('0.2.23', '0.3.0'))).toThrow('refused the feed: under its minimum version');
    expect(() => check(feed('1.9.9', '2.0.0'))).toThrow('refused the feed: under its minimum version');
    // in numeric order, not the text's
    expect(check(feed('0.2.10', '0.2.9')).issued).toBe('2026-11-15T10:00:00Z');
    expect(check(feed('0.2.23', '0.2.23')).issued).toBe('2026-11-15T10:00:00Z');
    for (const [version, floor] of [['0.2.24', 'latest'], ['0.2.24', '0.2'], ['0.2', '0.2.0'], ['0.2.24-beta', '0.2.23']]) {
      expect(() => check(feed(version, floor)), `${version} ${floor}`).toThrow('refused the feed: unreadable version');
    }
    for (const issued of ['yesterday', '2026-11-15T10:00:00.000Z', '2026-11-15 10:00:00Z', '2026-19-15T10:00:00Z']) {
      expect(() => check(signed(unsigned({ issued }), pair.privateKey)), issued).toThrow('refused the feed: unreadable issued');
    }
  });

  it('updates_14_the_last_issued_is_kept', async () => {
    vi.doMock('../../../shared/updates', async (original) => ({ ...await original<typeof import('../../../shared/updates')>(), FEED_PUBLIC_KEY: pair.publicKey }));
    const userData = mkdtempSync(join(tmpdir(), 'oby-updates-'));
    const file = join(userData, 'settings.json');
    writeFileSync(file, JSON.stringify({ updatesAuto: false, notifications: { jobs: true } }));
    const newer = signed(unsigned({ issued: '2026-11-15T10:00:00Z' }), pair.privateKey);
    const older = signed(unsigned({ issued: '2026-11-14T10:00:00Z' }), pair.privateKey);

    let { supported } = await launch(new Promise(() => {}), userData);
    serve(newer);
    await expect(supported(parsed(newer))).resolves.toBe(true);
    expect(JSON.parse(readFileSync(file, 'utf8'))).toEqual({ updatesAuto: false, notifications: { jobs: true }, updatesIssued: '2026-11-15T10:00:00Z' });

    // relaunched over the same folder: an older feed is a replay, the same one isn't
    ({ supported } = await launch(new Promise(() => {}), userData));
    serve(older);
    await expect(supported(parsed(older))).rejects.toThrow('refused the feed: a replay');
    serve(newer);
    await expect(supported(parsed(newer))).resolves.toBe(true);
    expect(JSON.parse(readFileSync(file, 'utf8')).updatesIssued).toBe('2026-11-15T10:00:00Z');
    rmSync(userData, { recursive: true, force: true });
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
