import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { next, type Update } from '../../../shared/updates';

// T3 Code's desktop-shell fixes, on Oby's main process (index.ts, sphere.ts, shared/updates.ts) with
// Electron and obid stood in for, as wake.test.ts does:
// - auto-update: a waiting update isn't announced again on every look, and a failed look is tried again
//   at the next (T3 2cc4b4888e "a waiting update is no longer re-announced on every poll", b0e469ceb9,
//   731291ee1f; docs/contracts/updates.md);
// - the window: its page never leaves for another page nor opens a window, whatever a reply links to
//   (T3 20e5a33968 "restrict editor deep links"; docs/contracts/profiles.md PROFILES-42, CHATFIRST-40);
// - deep links: an oby:// link that opens Oby before its window exists still asks to join once it's
//   there, and one that isn't an invite says so (T3 1dd8c676e4; docs/contracts/obysphere.md, SPHEREAPP-3);
// - theme: the window opens on the page's own dark, never a white flash before it paints (T3 411f0f7d87).

describe('updates (T3: auto-update)', () => {
  const ready: Update = { state: 'ready', version: '0.3.0' } as Update;
  it('t3_update_a_waiting_update_is_not_looked_for_or_announced_again_on_each_tick', () => {
    let u = ready;
    for (let i = 0; i < 12; i++) {
      const [v, action] = next(u, { kind: 'tick' }, true);
      expect(action).toBeNull();
      // the same state, so About isn't told anything new
      expect(v).toBe(u);
      u = v;
    }
    // nor while one downloads
    const downloading = { state: 'downloading', restart: false, version: '0.3.0' } as Update;
    expect(next(downloading, { kind: 'tick' }, true).slice(0, 2)).toEqual([downloading, null]);
  });

  it('t3_update_a_failed_look_or_download_is_tried_again_at_the_next_tick', () => {
    const [failed] = next({ state: 'downloading', restart: false, version: '0.3.0' } as Update, { kind: 'failed', code: 'download_failed' } as never, true);
    expect(failed.state).toBe('idle');
    const [again, action] = next(failed, { kind: 'tick' }, true);
    expect(action).toBe('check');
    expect(again.state).toBe('checking');
    // with Update automatically off, only the owner's click looks
    expect(next(failed, { kind: 'tick' }, false)[1]).toBeNull();
  });
});

const the = vi.hoisted(() => ({
  app: null as unknown as import('node:events').EventEmitter,
  windows: [] as (import('node:events').EventEmitter & { visible: boolean; closed: boolean; webContents: import('node:events').EventEmitter & { opens?: (d: { url: string }) => { action: string } } })[],
  dialogs: [] as { message: string; detail?: string }[],
  options: [] as Record<string, unknown>[],
  ready: null as unknown as { promise: Promise<void>; go: () => void },
}));
vi.mock('electron', async () => {
  const { EventEmitter } = await import('node:events');
  class BrowserWindow extends EventEmitter {
    visible = false; closed = false;
    webContents = Object.assign(new EventEmitter(), {
      setWindowOpenHandler(f: (d: { url: string }) => { action: string }) { (this as { opens?: unknown }).opens = f; },
      send() {}, getURL: () => 'file:///Applications/Oby.app/Contents/Resources/app/out/renderer/index.html', mainFrame: {},
    });
    constructor(options: Record<string, unknown>) { super(); the.options.push(options); the.windows.push(this as never); the.app.emit('browser-window-created', {}, this); }
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
  let go!: () => void;
  the.ready = { promise: new Promise<void>((r) => { go = r; }), go: () => go() };
  the.app = Object.assign(new EventEmitter(), {
    isPackaged: true, isInApplicationsFolder: () => false, commandLine: { hasSwitch: () => false, appendSwitch() {} },
    enableSandbox() {}, whenReady: () => the.ready.promise, setAsDefaultProtocolClient() {}, quit() {},
  });
  return {
    app: the.app, BrowserWindow,
    dialog: { showMessageBox: async (_w: unknown, o: { message: string; detail?: string }) => { the.dialogs.push({ message: o.message, detail: o.detail }); return { response: 1 }; } },
    ipcMain: { on() {}, handle() {} }, Menu: { setApplicationMenu() {}, buildFromTemplate: () => ({}) },
    screen: { getCursorScreenPoint: () => ({ x: 0, y: 0 }) }, shell: { openExternal() {} },
  };
});
vi.mock('../../../main/obid', () => ({ bridge() {}, startObid() {}, openedComputer: () => null, onComputer() {}, send: async () => ({ status: 0, text: '' }), obid: async () => ({ status: 200, text: '{"owner":"me"}' }) }));
vi.mock('../../../main/live', () => ({ living() {} }));
vi.mock('../../../main/device', () => ({ confirming() {}, ownersWords() {}, keptKey() {}, typeInPages() {} }));
vi.mock('../../../main/words', () => ({ input() {}, space() {}, picked() {}, takeInput: () => false }));
vi.mock('../../../main/notify', () => ({ notifying() {}, windowLeft() {}, windowSeen() {} }));
vi.mock('../../../main/updates', () => ({ updating() {} }));
vi.mock('../../../main/runMarker', () => ({ started() {}, windowLoaded() {}, rendererGone() {}, childGone() {}, quitting() {}, quit() {} }));
vi.mock('../../../shared/sphere', () => {
  class NotThisKey extends Error {}
  return {
    NotThisKey, join() {}, joinWarning: () => '', leave() {}, keepMembership: async () => {},
    parseInvite: (text: string) => (text.startsWith('oby://join/') ? { name: 'Deskree', sphere: 'deskree', by: 'Dmytro' } : null),
  };
});

const tick = () => new Promise((r) => setImmediate(r));
beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers({ toFake: ['setInterval'] });
  the.windows.length = 0;
  the.dialogs.length = 0;
  the.options.length = 0;
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => {
  the.app?.removeAllListeners();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('the window (T3: desktop shell)', () => {
  it('t3_shell_the_window_never_leaves_its_page_nor_opens_another_window', async () => {
    await import('../../../main/index');
    the.ready.go();
    await tick();
    const w = the.windows[0];
    expect(w).toBeDefined();
    const at = 'file:///Applications/Oby.app/Contents/Resources/app/out/renderer/index.html';
    for (const url of ['https://github.com/o/r/pull/1', 'oby://join/x', 'file:///etc/passwd', 'javascript:alert(1)', `${at}#elsewhere`]) {
      const e = { prevented: false, preventDefault() { this.prevented = true; } };
      w.webContents.emit('will-navigate', e, url);
      expect(e.prevented, url).toBe(true);
    }
    // its own reload (PROFILES-42)
    const reload = { prevented: false, preventDefault() { this.prevented = true; } };
    w.webContents.emit('will-navigate', reload, at);
    expect(reload.prevented).toBe(false);
    // a link with target=_blank, window.open: no window
    expect(w.webContents.opens?.({ url: 'https://github.com' })).toEqual({ action: 'deny' });
  });
});

describe('dark (T3: theme)', () => {
  it('t3_theme_the_window_opens_on_the_pages_own_dark_never_a_white_flash', async () => {
    await import('../../../main/index');
    the.ready.go();
    await tick();
    const tokens = readFileSync(join(__dirname, '..', 'tokens.css'), 'utf8');
    const bg = /--bg: (#[0-9a-f]{6});/i.exec(tokens)?.[1];
    expect(bg).toBeDefined();
    // drawn before the page paints, and the page's own background: the same colour
    expect(String(the.options[0].backgroundColor).toLowerCase()).toBe(bg!.toLowerCase());
    // the page is dark only: Chromium's own controls and scrollbars are drawn dark too (T3 411f0f7d87)
    expect(tokens).toMatch(/color-scheme: dark;/);
  });
});

describe('oby:// links (T3: deep links)', () => {
  it('t3_deeplink_a_link_that_opens_oby_before_its_window_asks_to_join_once_it_is_there', async () => {
    await import('../../../main/index');
    // macOS hands the link over before the app is ready
    const e = { prevented: false, preventDefault() { this.prevented = true; } };
    the.app.emit('open-url', e, 'oby://join/sphere.example#invite');
    expect(e.prevented).toBe(true);
    expect(the.dialogs).toEqual([]);
    the.ready.go();
    await tick();
    await tick();
    expect(the.windows.filter((w) => !w.closed)).toHaveLength(1);
    expect(the.dialogs.map((d) => d.message)).toEqual(['Join Deskree?']);
    // once running, the next link asks again; one that isn't an invite says so
    the.app.emit('open-url', { preventDefault() {} }, 'oby://join/sphere.example#again');
    the.app.emit('open-url', { preventDefault() {} }, 'oby://settings');
    await tick();
    await tick();
    // (the invite's dialog waits on obid's owner id, so the refusal can come first)
    expect(the.dialogs.map((d) => d.message).sort()).toEqual(['Join Deskree?', 'Join Deskree?', 'Oby can’t open this link.']);
  });
});
