import { existsSync, mkdtempSync, readdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { anything, boolean, flag, https, method, oneOf, optional, orUndefined, profileId, safeInteger, string, v1Path } from '../../../shared/decode';
import { input, takeInput } from '../../../main/words';

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
  refuseOpen: false,
  checks: 0,
  policies: 0,
  joins: 0,
  invites: 0,
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
    isPackaged: false, commandLine: { hasSwitch: () => false, appendSwitch() {} }, getPath: () => the.userData, getVersion: () => '1.0.0',
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
    clipboard: { readText: () => '' },
    // the artifact preview's view and its partition (artifactPreview.ts): made only when an artifact first shows
    WebContentsView: class {}, session: { fromPartition: () => ({}) },
    shell: { openExternal: async (url: string) => { if (the.refuseOpen) throw new Error('refused'); the.opened.push(url); } },
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
  chooseProfile: () => false, openedComputer: () => null, onComputer: () => {}, heardComputers: () => {},
  openedProfile: () => null,
}));
vi.mock('../../../shared/sphere', async (original) => ({
  // links read as main reads them; nothing joins
  parseInvite: (await original<typeof import('../../../shared/sphere')>()).parseInvite,
  NotThisKey: class extends Error {}, join: async () => { the.joins++; }, joinWarning() {}, leave() {}, keepMembership: async () => {},
  readPolicy: async () => { the.policies++; }, addStarterRule: async () => { the.policies++; },
  listInvitations: async () => ({ invitations: [], link: null, refused: null, why: null }),
  makeInvitation: async () => { the.invites++; return { invitations: [], link: 'oby://join/x#…', refused: null, why: null }; },
  withdrawInvitation: async () => { the.invites++; return { invitations: [], link: null, refused: null, why: null }; },
  setAdmin: async () => { the.invites++; return { admins: ['dana', 'wellington'], refused: null, why: null }; },
  rebindOffer: async () => [{ id: 'google', name: 'Google' }],
  getBackIn: async () => { the.invites++; return { kind: 'back', owner: 'wellington', admin: false }; },
  signinStatus: async () => ({ providers: [{ id: 'google', name: 'Google' }], linked: false }),
  addSignin: async () => { the.invites++; return { linked: true }; },
}));

const FORBIDDEN = { status: 403, text: '' };
const LINK = `https://sphere.example.com/join#sphere=demo&name=Demo%20team&id=9187a87a-3126-424c-8861-8811bba1a5ec&secret=${'A'.repeat(43)}`;
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
  'obid-snapshot': { args: [], refusal: null },
  'obid-load': { args: [{ conversation: 'c1' }], refusal: null },
  'obid-profile': { args: ['bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb'], refusal: null },
  'obid-confirm': { args: ['confirm-1', true], refusal: FORBIDDEN },
  'obid-pair-invite': { args: [], refusal: FORBIDDEN },
  'obid-publish': { args: ['j1', 'Publish this', {}], refusal: FORBIDDEN },
  'obid-limit-answer': { args: ['d1', 1, 'stop'], refusal: FORBIDDEN },
  'obid-undo': { args: ['j1', 'Undo this'], refusal: FORBIDDEN },
  'obid-finish': { args: ['c1', '/Users/me/api', 'commit'], refusal: FORBIDDEN },
  'open-worktree': { args: ['j1', 'terminal'], refusal: false },
  'worktree-editor': { args: [], refusal: null },
  'open-task-server': { args: ['j1', 3000], refusal: false },
  'reply-files': { args: ['task', 'j1', ['src/x.ts']], refusal: [] },
  'open-reply-file': { args: ['task', 'j1', 'src/x.ts:42'], refusal: false },
  showing: { args: [{ job: 'j1' }], refusal: undefined },
  opened: { args: [], refusal: null },
  notifications: { args: [false], refusal: null },
  'notification-kinds': { args: [{ merged: false }], refusal: null },
  'open-notification-settings': { args: [], refusal: undefined },
  updates: { args: ['check'], refusal: null },
  'local-settings': { args: [], refusal: null },
  'local-setting': { args: ['login', false], refusal: null },
  'login-item': { args: [false], refusal: null },
  'sphere-leave': { args: [], refusal: 'Not this app’s page.' },
  'sphere-policy': { args: [], refusal: null },
  'sphere-set-policy': { args: [0], refusal: null },
  'sphere-invitations': { args: [], refusal: null },
  'sphere-invite': { args: [], refusal: null },
  'sphere-withdraw': { args: ['9187a87a-3126-424c-8861-8811bba1a5ec'], refusal: null },
  'sphere-admin': { args: ['dana', true], refusal: null },
  'sphere-rebind-offer': { args: ['sphere.example.com/demo'], refusal: null },
  'sphere-rebind': { args: ['sphere.example.com/demo', 'google'], refusal: 'Not this app’s page.' },
  'sphere-signin-status': { args: [], refusal: null },
  'sphere-add-signin': { args: ['google'], refusal: 'Not this app’s page.' },
  'sphere-join': { args: [LINK], refusal: { kind: 'failed', name: '', why: 'Not this app’s page.' } },
  'sphere-handoff': { args: [{ to: 'dana', goal: 'Add a name option', repo: 'acme/app', base: 'main' }], refusal: 'Not this app’s page.' },
  'render-error': { args: ['TypeError', 'm', ''], refusal: undefined },
  help: { args: ['reveal-log'], refusal: 'Not this app’s page.' },
  'browser-follow': { args: [], refusal: null },
  'browser-watch': { args: ['t1'], refusal: null },
  'browser-unwatch': { args: ['t1'], refusal: null },
  'browser-register': { args: ['j1', 't1', 5], refusal: 'Not this app’s page.' },
  'browser-shown': { args: ['j1', 't1', true], refusal: null },
  'browser-fill': { args: ['t1', '#pw', 'x', '', true], refusal: 'Not this app’s page.' },
  'browser-reveal': { args: ['statement-oct.pdf'], refusal: false },
  'artifact-preview': { args: [null], refusal: null },
  'mac-pill': { args: [null], refusal: null },
  'mac-pill-stop': { args: [], refusal: undefined },
  'mac-pill-now': { args: [], refusal: null },
  'terminal-open': { args: ['task', 'j1', 80, 24], refusal: { error: 'Not this app’s page.' } },
  'terminal-write': { args: ['t1', 'ls'], refusal: undefined },
  'terminal-resize': { args: ['t1', 100, 30], refusal: undefined },
  'terminal-close': { args: ['t1'], refusal: undefined },
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
    const before = { calls: the.calls.length, checks: the.checks, policies: the.policies };
    // full screen: what full-screen's own answer would be
    the.windows[0].fullScreen = true;
    for (const [name, { args, refusal, sync }] of Object.entries(CHANNELS)) {
      const answer = await call(name, stranger(), ...args);
      expect(answer, name).toEqual(sync || the.handlers.get(name)!.kind === 'handle' ? refusal : UNSET);
    }
    expect(the.calls.length).toBe(before.calls);
    expect(the.checks).toBe(before.checks);
    expect(the.policies).toBe(before.policies);
    expect(the.dialogs).toBe(0);
    expect(the.opened).toEqual([]);
    expect(existsSync(join(the.userData, 'settings.json'))).toBe(false);
    expect(existsSync(join(the.userData, 'crashes.jsonl'))).toBe(false);
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
    // what main reads for the page: exactly one of a conversation, a job or the limit decisions
    for (const what of [{}, { job: '' }, { limits: 1 }, { conversation: 'c1', job: 'j1' }, { conversation: 7 }, null, 'c1', [{ job: 'j1' }]]) {
      expect(await call('obid-load', own(), what), JSON.stringify(what)).toBeNull();
    }
    // the Obysphere's rules: only a version the page read
    for (const version of ['0', 1.5, null, undefined]) expect(await call('sphere-set-policy', own(), version), String(version)).toBeNull();
    expect(the.policies).toBe(0);
    expect(the.calls.length).toBe(before);
    // the same calls, readable, reach obid
    expect(await call('obid-limit-answer', own(), 'd1', 1, 'stop')).toMatchObject({ status: 409 });
    expect(the.calls.length).toBe(before + 1);
    await call('open-external', own(), 'http://example.com');
    await call('open-external', own(), 42);
    expect(the.opened).toEqual([]);
    await call('open-external', own(), 'https://example.com');
    expect(the.opened).toEqual(['https://example.com']);
    // a render error's report: strings only, else no crash line
    await call('render-error', own(), 'TypeError', { secret: 1 }, '');
    expect(existsSync(join(the.userData, 'crashes.jsonl'))).toBe(false);
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
    // a profile's id as obid lists it, or none (B00-40)
    const id = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
    expect(profileId(id)).toEqual({ value: id });
    expect(profileId(null)).toEqual({ value: null });
    for (const v of [id.toUpperCase(), id.slice(1), '../x', '', 1, undefined]) expect(profileId(v)).toBeUndefined();
    // the lenient ones never fail
    for (const v of [undefined, null, 0, 'x', true, {}, []]) {
      expect(flag(v)).toEqual({ value: v === true });
      expect(orUndefined(string)(v)).toEqual({ value: typeof v === 'string' ? v : undefined });
      expect(anything(v)).toEqual({ value: v });
    }
  });
});

describe('POLICY-4', () => {
  it('policy_4_the_members_rule_is_written_only_right_after_a_click_main_saw', async () => {
    while (takeInput()) { /* other tests' */ }
    const before = the.policies;
    // the page alone, with no click or key main saw: refused, and nothing written
    expect(await call('sphere-set-policy', own(), 0)).toEqual({ sphere: null, policy: null, admin: false, refused: 'failed', why: 'Adding the rule takes your own click.' });
    expect(the.policies).toBe(before);
    // a click: one write, and only one
    input();
    await call('sphere-set-policy', own(), 0);
    expect(the.policies).toBe(before + 1);
    expect(await call('sphere-set-policy', own(), 0)).toMatchObject({ refused: 'failed' });
    expect(the.policies).toBe(before + 1);
    // reading takes none
    await call('sphere-policy', own());
    expect(the.policies).toBe(before + 2);
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

describe('ALERTS-19', () => {
  const PANE = 'x-apple.systempreferences:com.apple.Notifications-Settings.extension';
  it('alerts_19_main_opens_only_the_notifications_pane_whatever_the_page_sends', async () => {
    the.opened.length = 0;
    // the page's click opens the pane; anything it sends along is ignored
    await call('open-notification-settings', own());
    await call('open-notification-settings', own(), 'x-apple.systempreferences:com.apple.Passwords-Settings.extension', 'https://example.com');
    expect(the.opened).toEqual([PANE, PANE]);
    // a sender that isn't the window's own page opens nothing
    await call('open-notification-settings', stranger());
    expect(the.opened).toHaveLength(2);
    // the page's open-external still takes https only: no System Settings URL, no other scheme
    for (const url of [PANE, `${PANE}?id=com.deskree.oby.electron`, 'x-apple.systempreferences:', 'file:///etc/passwd', 'http://example.com']) {
      await call('open-external', own(), url);
    }
    expect(the.opened).toHaveLength(2);
    the.opened.length = 0;
  });

  it('alerts_19_a_refused_open_is_logged_never_silent', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});
    the.refuseOpen = true;
    try {
      await call('open-notification-settings', own());
      await vi.waitFor(() => expect(logged).toHaveBeenCalledWith(expect.stringContaining(`Oby: couldn’t open System Settings at ${PANE}`)));
    } finally {
      the.refuseOpen = false;
      logged.mockRestore();
    }
  });
});

// docs/contracts/sphere-app.md: a link pasted in the app, and a task handed over, go only right after
// a click or key main saw itself
describe('SPHEREAPP-4 and 6', () => {
  it('sphereapp_4_a_pasted_link_joins_through_main_s_own_dialog_only_after_a_click_main_saw', async () => {
    while (takeInput()) { /* other tests' */ }
    const before = { dialogs: the.dialogs, joins: the.joins };
    // the page alone: refused, no dialog, nothing joined
    expect(await call('sphere-join', own(), LINK)).toEqual({ kind: 'failed', name: '', why: 'Joining takes your own click.' });
    expect(the.dialogs).toBe(before.dialogs);
    // a click: main's own Join dialog, the oby:// link's; its Cancel joins nothing
    input();
    expect(await call('sphere-join', own(), LINK)).toEqual({ kind: 'cancelled' });
    expect(the.dialogs).toBe(before.dialogs + 1);
    expect(the.joins).toBe(before.joins);
    // a text that isn't a link: said, no dialog
    input();
    expect(await call('sphere-join', own(), 'https://example.com/')).toEqual({ kind: 'not_a_link' });
    expect(the.dialogs).toBe(before.dialogs + 1);
  });

  it('sphereapp_6_a_task_is_handed_over_only_right_after_a_click_main_saw', async () => {
    while (takeInput()) { /* other tests' */ }
    const ask = { to: 'dana', goal: 'Add a name option', repo: 'acme/app', base: 'main' };
    const before = the.calls.length;
    expect(await call('sphere-handoff', own(), ask)).toBe('Handing a task over takes your own click.');
    expect(the.calls.length).toBe(before);
    // a click: main asks obid who's in (no one here), and says so
    input();
    expect(await call('sphere-handoff', own(), ask)).toBe('dana isn’t in your Obysphere.');
    expect(the.calls.slice(before).map((c) => `${c[0]} ${c[1]}`)).toEqual(['GET /v1/obid/sphere/members']);
    // the page's object is read as four strings, nothing more
    expect(await call('sphere-handoff', own(), { ...ask, goal: 7 })).toBe('Not this app’s page.');
  });

  it('sphereapp_6_main_sends_obid_one_handoff_to_their_oby_that_takes_tasks', async () => {
    const { handOff } = await import('../../../main/sphere');
    const sent: unknown[][] = [];
    const members = [
      { obi: 'obi-dana-old', owner: 'dana', accepts: ['question', 'review_request'], sealed: true },
      { obi: 'obi-dana', owner: 'dana', accepts: ['question', 'review_request', 'handoff'], sealed: true },
      { obi: 'obi-ben', owner: 'ben', accepts: ['question'], sealed: true },
    ];
    const fake = (answer: { status: number; text: string }) => async (method: string, path: string, body?: unknown) => {
      sent.push([method, path, body]);
      return method === 'GET' ? { status: 200, text: JSON.stringify(members) } : answer;
    };
    const ask = { to: 'dana', goal: '  Add a name option to the greeter ', repo: 'acme/app', base: 'main' };
    expect(await handOff(ask, fake({ status: 202, text: '{}' }))).toBeNull();
    expect(sent[1]).toEqual(['POST', '/v1/obid/sphere/requests', {
      to: 'obi-dana', type: 'handoff', expiresIn: 604800,
      body: { goal: 'Add a name option to the greeter', repo: 'acme/app', base_branch: 'main', acceptance: [{ kind: 'tests_pass' }] },
    }]);
    // a member whose Oby doesn't take tasks, the Sphere's rules, and a form that can't go: words, and nothing sent
    sent.length = 0;
    expect(await handOff({ ...ask, to: 'ben' }, fake({ status: 202, text: '{}' }))).toBe('ben’s Oby doesn’t take tasks yet.');
    expect(await handOff(ask, fake({ status: 502, text: JSON.stringify({ error: 'The Sphere refused it: policy_denied: no rule' }) })))
      .toBe('This Obysphere’s rules don’t allow handing over tasks yet. An admin can change that.');
    expect(sent.filter((c) => c[0] === 'POST')).toHaveLength(1);
    sent.length = 0;
    expect(await handOff({ ...ask, repo: 'not a repo' }, fake({ status: 202, text: '{}' }))).toBe('That task can’t go as it is.');
    expect(sent).toEqual([]);
  });
});

describe('SPHEREAPP-10', () => {
  it('sphereapp_10_a_link_is_made_or_withdrawn_only_right_after_a_click_main_saw', async () => {
    while (takeInput()) { /* other tests' */ }
    const before = the.invites;
    expect(await call('sphere-invite', own())).toMatchObject({ refused: 'failed', why: 'Making a link takes your own click.', link: null });
    expect(await call('sphere-withdraw', own(), 'inv-1')).toMatchObject({ refused: 'failed', why: 'Withdrawing a link takes your own click.' });
    expect(the.invites).toBe(before);
    // a click: one link, and only one
    input();
    await call('sphere-invite', own());
    expect(the.invites).toBe(before + 1);
    await call('sphere-invite', own());
    expect(the.invites).toBe(before + 1);
    input();
    await call('sphere-withdraw', own(), 'inv-1');
    expect(the.invites).toBe(before + 2);
    // the list takes none
    expect(await call('sphere-invitations', own())).toMatchObject({ refused: null });
    expect(the.invites).toBe(before + 2);
  });
});

describe('SPHEREAPP-11', () => {
  it('sphereapp_11_who_is_admin_changes_only_right_after_a_click_main_saw', async () => {
    while (takeInput()) { /* other tests' */ }
    const before = the.invites;
    expect(await call('sphere-admin', own(), 'dana', true)).toEqual({ admins: null, refused: 'failed', why: 'Changing who’s admin takes your own click.' });
    expect(the.invites).toBe(before);
    input();
    expect(await call('sphere-admin', own(), 'dana', true)).toMatchObject({ admins: ['dana', 'wellington'], refused: null });
    expect(the.invites).toBe(before + 1);
    // the page's owner and choice are read as a string and a boolean, nothing else
    expect(await call('sphere-admin', own(), 'dana', 'yes')).toBeNull();
  });
});

describe('SPHEREAPP-12', () => {
  it('sphereapp_12_getting_back_in_starts_only_right_after_a_click_main_saw_and_main_s_own_dialog', async () => {
    while (takeInput()) { /* other tests' */ }
    const before = { dialogs: the.dialogs, invites: the.invites };
    // what an address offers takes no click
    expect(await call('sphere-rebind-offer', own(), 'sphere.example.com/demo')).toEqual({ sphere: 'demo', host: 'sphere.example.com', providers: [{ id: 'google', name: 'Google' }] });
    expect(await call('sphere-rebind-offer', own(), 'not an address')).toBeNull();
    // the sign-in: refused without a click, no dialog
    expect(await call('sphere-rebind', own(), 'sphere.example.com/demo', 'google')).toBe('Getting back in takes your own click.');
    expect(the.dialogs).toBe(before.dialogs);
    // a click: main's own dialog; its Cancel starts nothing
    input();
    expect(await call('sphere-rebind', own(), 'sphere.example.com/demo', 'google')).toBeNull();
    expect(the.dialogs).toBe(before.dialogs + 1);
    expect(the.invites).toBe(before.invites);
  });
});

describe('SPHEREAPP-13', () => {
  it('sphereapp_13_a_sign_in_is_added_only_right_after_a_click_main_saw', async () => {
    while (takeInput()) { /* other tests' */ }
    const before = the.invites;
    expect(await call('sphere-signin-status', own())).toEqual({ providers: [{ id: 'google', name: 'Google' }], linked: false });
    expect(await call('sphere-add-signin', own(), 'google')).toBe('Adding sign-in takes your own click.');
    expect(the.invites).toBe(before);
    input();
    expect(await call('sphere-add-signin', own(), 'google')).toBeNull();
    expect(the.invites).toBe(before + 1);
  });
});
