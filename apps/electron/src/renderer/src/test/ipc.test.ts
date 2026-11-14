import { existsSync, mkdtempSync, readdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { anything, boolean, flag, https, method, oneOf, optional, orUndefined, safeInteger, string, v1Path } from '../../../shared/decode';

// docs/contracts/ipc.md: every channel the page reaches main on, through main's one helper
// (main/ipc.ts). Main's own index.ts and the modules it registers channels from, run for real,
// with Electron, obid's calls and the updater stood in for.
type Handler = (e: unknown, ...args: unknown[]) => unknown;
const the = vi.hoisted(() => ({
  handlers: new Map<string, { kind: 'on' | 'handle'; f: Handler }>(),
  app: null as unknown as import('node:events').EventEmitter,
  windows: [] as ({ webContents: { mainFrame: object }; fullScreen: boolean })[],
  userData: '',
  calls: [] as unknown[][],
  dialogs: 0,
  opened: [] as string[],
  checks: 0,
}));
vi.mock('electron', async () => {
  const { EventEmitter } = await import('node:events');
  class BrowserWindow extends EventEmitter {
    fullScreen = false;
    webContents = Object.assign(new EventEmitter(), { setWindowOpenHandler() {}, send() {}, getURL: () => '', isDestroyed: () => false, mainFrame: {} });
    constructor() { super(); the.windows.push(this); }
    static getAllWindows = () => the.windows;
    static fromWebContents = () => null;
    getNativeWindowHandle() { return Buffer.alloc(8); }
    loadFile() {}
    loadURL() {}
    isDestroyed() { return false; }
    isMinimized() { return false; }
    isVisible() { return true; }
    isFullScreen() { return this.fullScreen; }
    getBounds() { return { x: 0, y: 0, width: 994, height: 670 }; }
    setBounds() {}
    setResizable() {}
    setMaximizable() {}
    setFullScreenable() {}
    setWindowButtonPosition() {}
    setSize() {}
    restore() {}
    show() {}
    focus() {}
  }
  the.app = Object.assign(new EventEmitter(), {
    isPackaged: false, commandLine: { hasSwitch: () => false, appendSwitch() {} }, getPath: () => the.userData,
    enableSandbox() {}, whenReady: () => Promise.resolve(), setAsDefaultProtocolClient() {}, quit() {},
  });
  const capture = (kind: 'on' | 'handle') => (name: string, f: Handler) => { the.handlers.set(name, { kind, f }); };
  return {
    app: the.app, BrowserWindow, Notification: class { static isSupported = () => true; },
    dialog: {
      showOpenDialog: async () => { the.dialogs++; return { canceled: true, filePaths: [] }; },
      showMessageBox: async () => { the.dialogs++; return { response: 1 }; },
    },
    ipcMain: { on: capture('on'), handle: capture('handle') },
    Menu: { setApplicationMenu() {}, buildFromTemplate: () => ({}) },
    powerMonitor: { on() {} },
    safeStorage: { isEncryptionAvailable: () => true, encryptString: (s: string) => Buffer.from(s), decryptString: (b: Buffer) => b.toString() },
    screen: { getCursorScreenPoint: () => ({ x: 0, y: 0 }) },
    shell: { openExternal: async (url: string) => { the.opened.push(url); } },
  };
});
// the native window style (index.ts's windowStyle): where it put the traffic lights
const CHROME = { lightsLeft: 12, lightsTop: 14, lightsCenterY: 20, lightsRight: 70 };
vi.mock('node:module', async (original) => ({
  ...await original<typeof import('node:module')>(), createRequire: () => () => ({ useToolbarWindow: () => CHROME }),
}));
vi.mock('electron-updater', () => ({
  default: { autoUpdater: { setFeedURL() {}, on() {}, isUpdateSupported: () => true, checkForUpdates: async () => { the.checks++; return null; } } },
}));
// obid's calls, counted; the event stream never answers
vi.mock('../../../main/obid', async (original) => ({
  ...await original<typeof import('../../../main/obid')>(),
  obid: async (...a: unknown[]) => { the.calls.push(a); return { status: 200, text: '[]' }; },
  send: () => new Promise(() => {}),
  takesKeys: async () => false,
}));
vi.mock('../../../shared/sphere', () => ({
  NotThisKey: class extends Error {}, join() {}, joinWarning() {}, leave() {}, parseInvite() {}, keepMembership: async () => {},
}));

const FORBIDDEN = { status: 403, text: '' };
const NOT_OBID = { status: 400, text: '{"error":"Not an obid call."}' };
// every channel, with arguments its handler would act on, and what a sender that isn't the
// window's own page gets (sync: its returnValue; undefined: nothing, as send gets)
const CHANNELS: Record<string, { args: unknown[]; refusal: unknown; sync?: true }> = {
  'window-chrome': { args: [], refusal: null, sync: true },
  'full-screen': { args: [], refusal: false, sync: true },
  'setup-window': { args: [true], refusal: null, sync: true },
  'open-external': { args: ['https://example.com/sign-in'], refusal: undefined },
  'pick-folders': { args: ['/tmp'], refusal: [] },
  obid: { args: ['GET', '/v1/status'], refusal: FORBIDDEN },
  'open-login-items': { args: [], refusal: undefined },
  'obid-follow': { args: [], refusal: undefined },
  'obid-confirm': { args: ['confirm-1', true], refusal: FORBIDDEN },
  'obid-pair-invite': { args: [], refusal: FORBIDDEN },
  'obid-publish': { args: ['j1', 'Publish this', {}], refusal: FORBIDDEN },
  'obid-limit-answer': { args: ['d1', 1, 'stop'], refusal: FORBIDDEN },
  showing: { args: [{ job: 'j1' }], refusal: undefined },
  opened: { args: [], refusal: null },
  notifications: { args: [false], refusal: null },
  'open-notification-settings': { args: [], refusal: undefined },
  updates: { args: ['check'], refusal: null },
  'sphere-leave': { args: [], refusal: 'Not this app’s page.' },
};

const UNSET = Symbol('unset');
// a message from a frame that isn't the window's own page (another page, or a frame in it)
const stranger = () => ({ sender: {}, senderFrame: {}, returnValue: UNSET as unknown });
// one from the window's own page
const own = () => { const w = the.windows[0]; return { sender: w.webContents, senderFrame: w.webContents.mainFrame, returnValue: UNSET as unknown }; };
async function call(name: string, e: { returnValue: unknown }, ...args: unknown[]) {
  const answer = await the.handlers.get(name)!.f(e, ...args);
  return the.handlers.get(name)!.kind === 'on' ? e.returnValue : answer;
}

beforeAll(async () => {
  the.userData = mkdtempSync(join(tmpdir(), 'oby-ipc-'));
  vi.useFakeTimers({ toFake: ['setInterval'] });
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  // main as on a Mac, wherever the test runs: index.ts loads the window style only on macOS
  const platform = Object.getOwnPropertyDescriptor(process, 'platform')!;
  Object.defineProperty(process, 'platform', { ...platform, value: 'darwin' });
  try {
    await import('../../../main/index');
    await new Promise((r) => setImmediate(r));
  } finally {
    Object.defineProperty(process, 'platform', platform);
  }
});

describe('IPC-1', () => {
  it('ipc_1_a_sender_that_isnt_the_windows_page_gets_the_channels_refusal_and_nothing_runs', async () => {
    expect([...the.handlers.keys()].sort()).toEqual(Object.keys(CHANNELS).sort());
    const before = { calls: the.calls.length, checks: the.checks };
    // full screen: what full-screen's own answer would be
    the.windows[0].fullScreen = true;
    for (const [name, { args, refusal, sync }] of Object.entries(CHANNELS)) {
      const answer = await call(name, stranger(), ...args);
      expect(answer, name).toEqual(sync || the.handlers.get(name)!.kind === 'handle' ? refusal : UNSET);
    }
    expect(the.calls.length).toBe(before.calls);
    expect(the.checks).toBe(before.checks);
    expect(the.dialogs).toBe(0);
    expect(the.opened).toEqual([]);
    expect(existsSync(join(the.userData, 'settings.json'))).toBe(false);
    the.windows[0].fullScreen = false;
  });

  it('ipc_1: the window\'s own page still gets window-chrome and full-screen', async () => {
    the.windows[0].fullScreen = true;
    expect(await call('full-screen', own())).toBe(true);
    the.windows[0].fullScreen = false;
    expect(await call('full-screen', own())).toBe(false);
    expect(await call('window-chrome', own())).toEqual(CHROME);
  });
});

describe('IPC-2', () => {
  it('ipc_2_obid_takes_only_its_methods_and_v1_paths_before_anything_is_signed', async () => {
    const { bridge } = await import('../../../main/obid');
    const ownersWords = vi.fn(async () => ({ status: 200, text: '{}' }));
    const kept = the.handlers.get('obid');
    bridge(() => true, ownersWords);
    const obid = (...args: unknown[]) => the.handlers.get('obid')!.f({}, ...args);
    try {
      for (const m of ['HEAD', 'get', 'TRACE', 'OPTIONS', '', 1, undefined]) expect(await obid(m, '/v1/status'), String(m)).toEqual(NOT_OBID);
      for (const p of ['/v2/status', 'v1/status', 'https://example.com/v1/x', '', 7, null, undefined]) expect(await obid('POST', p, {}), String(p)).toEqual(NOT_OBID);
      expect(ownersWords).not.toHaveBeenCalled();
      expect(await obid('GET', '/v1/status')).toEqual({ status: 200, text: '{}' });
      expect(ownersWords).toHaveBeenLastCalledWith('GET', '/v1/status', undefined, false);
      await obid('POST', '/v1/messages', { text: 'hi' }, 'yes');
      expect(ownersWords).toHaveBeenLastCalledWith('POST', '/v1/messages', { text: 'hi' }, false);
      await obid('POST', '/v1/messages', { text: 'hi' }, true);
      expect(ownersWords).toHaveBeenLastCalledWith('POST', '/v1/messages', { text: 'hi' }, true);
      // an untrusted sender is refused before its method is even read
      bridge(() => false, ownersWords);
      expect(await obid('HEAD', '/v2/status')).toEqual(FORBIDDEN);
    } finally {
      the.handlers.set('obid', kept!);
    }
  });
});

describe('IPC-3', () => {
  it('ipc_3_an_argument_a_decoder_cant_read_never_reaches_the_handler', async () => {
    const { ipc } = await import('../../../main/ipc');
    const fn = vi.fn((..._: unknown[]) => 'ran');
    const kept = new Map(the.handlers);
    try {
      ipc(() => true).handle('x', [string, optional(safeInteger)], fn, 'refused', 'invalid');
      expect(await the.handlers.get('x')!.f({}, 1)).toBe('invalid');
      expect(await the.handlers.get('x')!.f({}, 's', 1.5)).toBe('invalid');
      expect(fn).not.toHaveBeenCalled();
      // arguments past the decoders are ignored
      expect(await the.handlers.get('x')!.f({}, 's', undefined, 'more')).toBe('ran');
      expect(fn).toHaveBeenLastCalledWith({}, 's', undefined);
      ipc(() => false).handle('x', [string], fn, 'refused', 'invalid');
      expect(await the.handlers.get('x')!.f({}, 's')).toBe('refused');
      // sendSync: always an answer
      ipc(() => true).on('y', [boolean], () => 'ran', null);
      const e = { returnValue: UNSET as unknown };
      the.handlers.get('y')!.f(e, 'no');
      expect(e.returnValue).toBeNull();
      the.handlers.get('y')!.f(e, true);
      expect(e.returnValue).toBe('ran');
    } finally {
      the.handlers.clear();
      for (const [k, v] of kept) the.handlers.set(k, v);
    }
  });

  it('ipc_3: real channels refuse what they can\'t read, with no obid call', async () => {
    const before = the.calls.length;
    expect(await call('obid-confirm', own(), 7, true)).toEqual(FORBIDDEN);
    expect(await call('obid-limit-answer', own(), 'd1', 1.5, 'stop')).toEqual(FORBIDDEN);
    expect(await call('obid-limit-answer', own(), 'd1', '1', 'stop')).toEqual(FORBIDDEN);
    expect(await call('obid-limit-answer', own(), 'd1', 1, 'later')).toEqual(FORBIDDEN);
    expect(await call('obid-limit-answer', own(), 'd1', 1, 'move', 5)).toEqual(FORBIDDEN);
    for (const choices of [null, [], { force: false }, { to: '' }, { other: true }]) {
      expect(await call('obid-publish', own(), 'j1', 'Publish this', choices), JSON.stringify(choices)).toEqual(FORBIDDEN);
    }
    expect(the.calls.length).toBe(before);
    // the same calls, readable, reach obid
    expect(await call('obid-limit-answer', own(), 'd1', 1, 'stop')).toMatchObject({ status: 409 });
    expect(the.calls.length).toBe(before + 1);
    await call('open-external', own(), 'http://example.com');
    await call('open-external', own(), 42);
    expect(the.opened).toEqual([]);
    await call('open-external', own(), 'https://example.com');
    expect(the.opened).toEqual(['https://example.com']);
  });

  it('ipc_3: the decoders', () => {
    expect(string('a')).toEqual({ value: 'a' });
    expect(string(1)).toBeUndefined();
    expect(boolean(false)).toEqual({ value: false });
    expect(boolean('true')).toBeUndefined();
    expect(safeInteger(3)).toEqual({ value: 3 });
    expect(safeInteger(2 ** 53)).toBeUndefined();
    expect(https('https://a.b')).toEqual({ value: 'https://a.b' });
    expect(https('http://a.b')).toBeUndefined();
    expect(optional(string)(undefined)).toEqual({ value: undefined });
    expect(optional(string)(null)).toBeUndefined();
    expect(oneOf('a', true)(true)).toEqual({ value: true });
    expect(oneOf('a', true)('b')).toBeUndefined();
    for (const m of ['GET', 'POST', 'PUT', 'PATCH', 'DELETE']) expect(method(m)).toEqual({ value: m });
    expect(method('get')).toBeUndefined();
    expect(v1Path('/v1/x')).toEqual({ value: '/v1/x' });
    expect(v1Path('/v1')).toBeUndefined();
    // the lenient ones never fail
    for (const v of [undefined, null, 0, 'x', true, {}, []]) {
      expect(flag(v)).toEqual({ value: v === true });
      expect(orUndefined(string)(v)).toEqual({ value: typeof v === 'string' ? v : undefined });
      expect(anything(v)).toEqual({ value: v });
    }
  });
});

describe('IPC-4', () => {
  it('ipc_4_only_main_ipc_ts_registers_a_channel', () => {
    const main = join(__dirname, '../../../main');
    const files = (readdirSync(main, { recursive: true }) as string[]).filter((f) => f.endsWith('.ts') && f !== 'ipc.ts');
    expect(files).toContain('index.ts');
    const raw = files.filter((f) => /ipcMain\s*\.\s*(handle|on)\s*\(/.test(readFileSync(join(main, f), 'utf8')));
    expect(raw).toEqual([]);
  });
});
