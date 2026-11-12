import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// docs/contracts/alerts.md, ALERTS-3: obid opens Oby hidden (`open -g -j … --background`) and no
// window comes forward. Main's own index.ts and sphere.ts, with Electron and obid stood in for.
const the = vi.hoisted(() => ({
  app: null as unknown as import('node:events').EventEmitter,
  windows: [] as (import('node:events').EventEmitter & { visible: boolean; closed: boolean })[],
  dialogs: [] as { w: unknown; message: string }[],
  oldKey: false,
}));
vi.mock('electron', async () => {
  const { EventEmitter } = await import('node:events');
  class BrowserWindow extends EventEmitter {
    visible = false; closed = false;
    webContents = Object.assign(new EventEmitter(), { setWindowOpenHandler() {}, send() {}, getURL: () => '', mainFrame: {} });
    // as Electron's: each new window is told to the app
    constructor() { super(); the.windows.push(this); the.app.emit('browser-window-created', {}, this); }
    static getAllWindows = () => the.windows.filter((w) => !w.closed);
    static fromWebContents = () => null;
    getNativeWindowHandle() { return Buffer.alloc(8); }
    loadFile() {}
    loadURL() {}
    isDestroyed() { return this.closed; }
    isMinimized() { return false; }
    isVisible() { return this.visible; }
    isFullScreen() { return false; }
    restore() {}
    show() { this.visible = true; this.emit('show'); }
    focus() {}
    getContentBounds() { return { x: 0, y: 0, width: 1, height: 1 }; }
  }
  the.app = Object.assign(new EventEmitter(), {
    isPackaged: true, commandLine: { hasSwitch: () => false, appendSwitch() {} },
    enableSandbox() {}, whenReady: () => Promise.resolve(), setAsDefaultProtocolClient() {}, quit() {},
  });
  return {
    app: the.app, BrowserWindow,
    dialog: { showMessageBox: async (w: unknown, o: { message: string }) => { the.dialogs.push({ w, message: o.message }); return { response: 0 }; } },
    ipcMain: { on() {}, handle() {} }, Menu: { setApplicationMenu() {}, buildFromTemplate: () => ({}) },
    screen: { getCursorScreenPoint: () => ({ x: 0, y: 0 }) }, shell: {},
  };
});
vi.mock('../../../main/obid', () => ({ bridge() {}, startObid() {}, stopFollowing() {}, obid: async () => ({ status: 200, text: '{}' }) }));
vi.mock('../../../main/device', () => ({ confirming() {}, ownersWords() {}, keptKey() {} }));
vi.mock('../../../main/words', () => ({ input() {}, space() {}, picked() {} }));
vi.mock('../../../main/notify', () => ({ notifying() {}, windowLeft() {}, windowSeen() {} }));
vi.mock('../../../main/updates', () => ({ updating() {} }));
vi.mock('../../../shared/sphere', () => {
  class NotThisKey extends Error {}
  return {
    NotThisKey, join() {}, joinWarning() {}, leave() {}, parseInvite() {},
    // obid holds a membership an older owner key signed (SPHERE-6)
    keepMembership: async () => { if (the.oldKey) throw new NotThisKey('Signed by another key.'); },
  };
});

const tick = () => new Promise((r) => setImmediate(r));
// Oby launched as obid or the owner launches it: main's index.ts, ready
async function launch(...args: string[]) {
  process.argv.push(...args);
  try { await import('../../../main/index'); } finally { if (args.length) process.argv.splice(-args.length); }
  await tick();
}
const open = () => the.windows.filter((w) => !w.closed);
const close = (w: (typeof the.windows)[number]) => { w.closed = true; w.visible = false; w.emit('closed'); };

beforeEach(() => {
  vi.resetModules();
  // Oby's pointer tracking, 60 times a second while a window shows, never runs here
  vi.useFakeTimers({ toFake: ['setInterval'] });
  the.windows.length = 0;
  the.dialogs.length = 0;
  the.oldKey = false;
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => {
  the.app?.removeAllListeners();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('ALERTS-3', () => {
  it('alerts_3_a_running_oby_obid_reopens_shows_no_window', async () => {
    await launch();
    the.app.emit('did-become-active');
    expect(open()).toHaveLength(1);
    // the owner closes the window and goes to another app; Oby keeps running
    close(open()[0]);
    the.app.emit('did-resign-active');
    // obid's `open -g -j` reaches the running Oby: a reopen, never made active
    the.app.emit('activate');
    await tick();
    expect(open()).toHaveLength(0);
    // the owner's Dock click: Oby becomes active, with its reopen
    the.app.emit('did-become-active');
    the.app.emit('activate');
    expect(open()).toHaveLength(1);
    // closed again while Oby stays in front: a Dock click still opens it
    close(open()[0]);
    the.app.emit('activate');
    expect(open()).toHaveLength(1);
  });

  it('alerts_3_a_membership_error_waits_for_the_owner_to_open_oby', async () => {
    the.oldKey = true;
    await launch('--background');
    // obid's own reopen of the launch it made
    the.app.emit('activate');
    await tick();
    expect(the.windows).toHaveLength(0);
    expect(the.dialogs).toHaveLength(0);
    // the owner opens Oby: the window, and the error on it once it shows
    the.app.emit('did-become-active');
    expect(open()).toHaveLength(1);
    open()[0].emit('ready-to-show');
    await tick();
    expect(the.dialogs).toEqual([{ w: open()[0], message: 'Oby can’t keep your Obysphere working.' }]);
  });
});
