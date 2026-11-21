import { existsSync, mkdtempSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// Settings rows obid has no setting for yet, kept by main in settings.json's `local`
// (main/settings.ts, docs/contracts/live-state.md, LIVE-25 to 29): main's half, run for real over
// an app folder of its own, with Electron, obid's calls and the updater stood in for.
type Handler = (e: unknown, ...args: unknown[]) => unknown;
type LoginCall = { set?: { type?: string; serviceName?: string; openAtLogin?: boolean }; get?: { type?: string } };
const the = vi.hoisted(() => ({
  handlers: new Map<string, Handler>(), userData: '',
  // a packaged copy, in Applications (asked how often)
  packaged: false, inApplications: true, asked: 0,
  // macOS's login items: the app's own, and obid's `agentService` entry, kept apart; every call
  appItem: false, obidItem: 'enabled', calls: [] as LoginCall[], setThrows: false,
}));
vi.mock('electron', () => {
  const capture = (name: string, f: Handler) => { the.handlers.set(name, f); };
  return {
    app: {
      getPath: () => the.userData, get isPackaged() { return the.packaged; }, dock: { setBadge() {} },
      isInApplicationsFolder: () => { the.asked++; return the.inApplications; },
      getLoginItemSettings: (o?: { type?: string }) => {
        the.calls.push({ get: o });
        return o?.type === 'agentService'
          ? { openAtLogin: the.obidItem === 'enabled', status: the.obidItem }
          : { openAtLogin: the.appItem, status: the.appItem ? 'enabled' : 'not-registered' };
      },
      setLoginItemSettings: (s: { type?: string; openAtLogin?: boolean }) => {
        the.calls.push({ set: s });
        if (the.setThrows) throw new Error('macOS said no');
        if (s.type === 'agentService') the.obidItem = s.openAtLogin ? 'enabled' : 'not-registered';
        else the.appItem = !!s.openAtLogin;
      },
    },
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
  chooseProfile: () => false, openedComputer: () => null, onComputer: () => {}, heardComputers: () => {},
  openedProfile: () => null,
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
  Object.assign(the, { packaged: false, inApplications: true, asked: 0, appItem: false, obidItem: 'enabled', calls: [], setThrows: false });
});
// a packaged Oby in Applications
const installed = () => Object.assign(the, { packaged: true, inApplications: true });
const sets = () => the.calls.flatMap((c) => (c.set ? [c.set] : []));

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

describe('LIVE-27', () => {
  it('live_27_turning_open_at_login_off_removes_the_apps_login_item_and_leaves_obids_entry', async () => {
    installed();
    the.appItem = true;
    await launch();
    expect(await call('login-item', stranger, false)).toBeNull();
    expect(the.calls).toEqual([]);
    expect(await call('login-item', own, false)).toBe(false);
    expect(sets()).toEqual([{ openAtLogin: false }]);
    expect(the.calls.every((c) => !(c.set ?? c.get)?.type && !c.set?.serviceName)).toBe(true);
    expect(the.obidItem).toBe('enabled');
    expect(file()).toEqual({ loginApplied: true });
    expect(await call('login-item', own, true)).toBe(true);
    expect(sets()).toEqual([{ openAtLogin: false }, { openAtLogin: true }]);
    expect(the.calls.some((c) => (c.set ?? c.get)?.type === 'agentService')).toBe(false);
    expect(the.obidItem).toBe('enabled');
  });

  it('live_27: in development nothing is registered', async () => {
    for (const copy of [{ packaged: false, inApplications: true }, { packaged: true, inApplications: false }]) {
      Object.assign(the, copy, { asked: 0 });
      await launch();
      expect(await call('login-item', own), JSON.stringify(copy)).toBeNull();
      expect(await call('login-item', own, false), JSON.stringify(copy)).toBeNull();
      expect(the.calls).toEqual([]);
      expect(existsSync(FILE())).toBe(false);
      // an unpackaged run never asks where it is
      expect(the.asked).toBe(copy.packaged ? 2 : 0);
    }
  });
});

describe('LIVE-28', () => {
  it('live_28_a_fresh_install_registers_the_apps_login_item_once_at_start', async () => {
    installed();
    (await launch()).loginDefault();
    expect(sets()).toEqual([{ openAtLogin: true }]);
    expect(file()).toEqual({ loginApplied: true });
    expect(the.appItem).toBe(true);
    expect(the.obidItem).toBe('enabled');
    // removed in System Settings, then a restart: it stays removed
    the.appItem = false;
    (await launch()).loginDefault();
    expect(sets()).toEqual([{ openAtLogin: true }]);
    expect(the.appItem).toBe(false);
  });

  it('live_28: a saved off is never registered', async () => {
    installed();
    writeFileSync(FILE(), JSON.stringify({ local: { login: false } }));
    (await launch()).loginDefault();
    expect(sets()).toEqual([]);
    expect(file()).toEqual({ local: { login: false }, loginApplied: true });
  });

  it('live_28: a saved value of the wrong type counts as on', async () => {
    installed();
    writeFileSync(FILE(), JSON.stringify({ local: { login: 'no' } }));
    (await launch()).loginDefault();
    expect(sets()).toEqual([{ openAtLogin: true }]);
  });

  it('live_28: an owner\'s turn is never overridden', async () => {
    installed();
    await launch();
    expect(await call('login-item', own, false)).toBe(false);
    the.calls = [];
    (await launch()).loginDefault();
    expect(the.calls).toEqual([]);
    expect(the.appItem).toBe(false);
  });

  it('live_28: nothing at start in development', async () => {
    (await launch()).loginDefault();
    expect(the.calls).toEqual([]);
    expect(the.asked).toBe(0);
    expect(existsSync(FILE())).toBe(false);
    // nor from a copy outside Applications
    Object.assign(the, { packaged: true, inApplications: false });
    (await launch()).loginDefault();
    expect(the.calls).toEqual([]);
    expect(existsSync(FILE())).toBe(false);
  });

  it('live_28: a default macOS doesn\'t take is tried again at the next start', async () => {
    installed();
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    the.setThrows = true;
    (await launch()).loginDefault();
    expect(error).toHaveBeenCalledOnce();
    expect(existsSync(FILE())).toBe(false);
    the.setThrows = false;
    (await launch()).loginDefault();
    expect(sets()).toEqual([{ openAtLogin: true }, { openAtLogin: true }]);
    expect(file()).toEqual({ loginApplied: true });
    error.mockRestore();
  });
});

describe('LIVE-29', () => {
  it('live_29_the_switch_shows_macos_state_when_it_differs_from_the_saved_value', async () => {
    installed();
    writeFileSync(FILE(), JSON.stringify({ local: { login: true }, loginApplied: true }));
    await launch();
    expect(await call('login-item', own)).toBe(false);
    expect(sets()).toEqual([]);
    the.appItem = true;
    expect(await call('login-item', own)).toBe(true);
    expect(file()).toEqual({ local: { login: true }, loginApplied: true });
  });

  it('live_29: a change macOS doesn\'t take goes back to what macOS says', async () => {
    installed();
    the.appItem = true;
    the.setThrows = true;
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await launch();
    expect(await call('login-item', own, false)).toBe(true);
    expect(error).toHaveBeenCalledOnce();
    // a write that fails too: both logged, and macOS's state still answered
    the.userData = join(the.userData, 'gone');
    expect(await call('login-item', own, false)).toBe(true);
    expect(error).toHaveBeenCalledTimes(3);
    error.mockRestore();
  });
});
