import { existsSync, mkdtempSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// Settings rows obid has no setting for yet, kept by main in settings.json's `local`
// (main/settings.ts, docs/contracts/live-state.md, LIVE-25 and 26): main's half, run for real over
// an app folder of its own, with Electron, obid's calls and the updater stood in for.
type Handler = (e: unknown, ...args: unknown[]) => unknown;
const the = vi.hoisted(() => ({ handlers: new Map<string, Handler>(), userData: '' }));
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
  default: { autoUpdater: { setFeedURL() {}, on() {}, isUpdateSupported: () => true, checkForUpdates: async () => null } },
}));
// obid answers nothing, and its event stream never does
vi.mock('../../../main/obid', async (original) => ({
  ...await original<typeof import('../../../main/obid')>(),
  obid: async () => ({ status: 200, text: '[]' }),
  send: () => new Promise(() => {}),
  takesKeys: async () => false,
}));

// the window's own page, and a frame that isn't it
const own = { own: true };
const stranger = {};
const trusted = (e: unknown) => (e as { own?: boolean }).own === true;
const FILE = () => join(the.userData, 'settings.json');
const file = () => JSON.parse(readFileSync(FILE(), 'utf8'));
const call = async (name: string, e: object, ...args: unknown[]) => the.handlers.get(name)!(e, ...args);

// One launch of main's settings: settings.ts fresh, its channels registered
async function launch() {
  vi.resetModules();
  the.handlers.clear();
  const settings = await import('../../../main/settings');
  settings.localSettings(trusted);
  return settings;
}

beforeEach(() => {
  the.userData = mkdtempSync(join(tmpdir(), 'oby-local-'));
});

describe('LIVE-25', () => {
  it('live_25_a_local_row_set_through_main_is_there_after_a_restart', async () => {
    await launch();
    expect(await call('local-setting', own, 'login', false)).toBe(true);
    // a restart: main's modules loaded again over the same app folder
    await launch();
    expect(await call('local-settings', own)).toEqual({ login: false });
    expect(readFileSync(FILE(), 'utf8')).toBe('{"local":{"login":false}}');
    expect(statSync(FILE()).mode & 0o777).toBe(0o600);
  });

  it('live_25: a sender that isn\'t the window\'s own page gets null, and nothing is written', async () => {
    await launch();
    expect(await call('local-setting', stranger, 'login', false)).toBeNull();
    expect(await call('local-settings', stranger)).toBeNull();
    expect(existsSync(FILE())).toBe(false);
  });

  it('live_25: a write that fails answers false, and main goes on', async () => {
    await launch();
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    the.userData = join(the.userData, 'gone');
    expect(await call('local-setting', own, 'login', false)).toBe(false);
    expect(error).toHaveBeenCalledOnce();
    error.mockRestore();
  });
});

describe('LIVE-26', () => {
  it('live_26_a_kept_value_of_the_wrong_type_shows_the_rows_default: main answers {} for a file it can\'t read', async () => {
    await launch();
    writeFileSync(FILE(), '{"local":');
    expect(await call('local-settings', own)).toEqual({});
    for (const local of ['login', ['login', false]]) {
      writeFileSync(FILE(), JSON.stringify({ local }));
      expect(await call('local-settings', own), JSON.stringify(local)).toEqual({});
    }
    // a key no row may have isn't answered
    writeFileSync(FILE(), '{"local":{"login":false,"__proto__":1,"":2}}');
    expect(await call('local-settings', own)).toEqual({ login: false });
  });
});

describe('main\'s own keys', () => {
  it('live_25: the page reaches only local, never a key of main\'s own', async () => {
    await launch();
    writeFileSync(FILE(), JSON.stringify({ notifications: true, updatesAuto: false }));
    expect(await call('local-setting', own, 'notifications', false)).toBe(true);
    expect(file()).toEqual({ notifications: true, updatesAuto: false, local: { notifications: false } });
    const before = readFileSync(FILE(), 'utf8');
    const refused: unknown[][] = [['__proto__', 1], ['', 1], [7, 1], ['a'.repeat(65), 1], ['login', 'x'.repeat(65_535)],
      ['login', undefined], ['login', 1n], ['login', () => 1]];
    for (const args of refused) expect(await call('local-setting', own, ...args), String(args[0]).slice(0, 10)).toBeNull();
    expect(readFileSync(FILE(), 'utf8')).toBe(before);
    // at the limit, kept exactly as it reads back
    expect(await call('local-setting', own, 'a'.repeat(64), 'x'.repeat(65_534))).toBe(true);
    const answer = await call('local-settings', own) as Record<string, unknown>;
    expect(Object.keys(answer).sort()).toEqual(['a'.repeat(64), 'notifications']);
    expect(answer).not.toHaveProperty('updatesAuto');
    expect(answer.notifications).toBe(false);
  });

  it('live_25: alerts, Update automatically and the local rows share settings.json', async () => {
    await launch();
    const { notifying } = await import('../../../main/notify');
    const { updating } = await import('../../../main/updates');
    vi.useFakeTimers({ toFake: ['setInterval'] });
    try {
      notifying(trusted, () => null, () => null as never);
      updating(trusted, () => null);
    } finally {
      vi.useRealTimers();
    }
    expect(await call('local-setting', own, 'login', false)).toBe(true);
    // alerts turned off, then Update automatically: the row stays
    expect(await call('notifications', own, false)).toBe(false);
    expect(await call('updates', own, false)).toMatchObject({ auto: false });
    expect(file()).toEqual({ local: { login: false }, notifications: false, updatesAuto: false });
    // a row kept after them: they stay
    expect(await call('local-setting', own, 'voice.speaks', false)).toBe(true);
    expect(file()).toEqual({ local: { login: false, 'voice.speaks': false }, notifications: false, updatesAuto: false });
  });
});
