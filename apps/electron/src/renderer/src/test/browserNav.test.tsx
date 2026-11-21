// @vitest-environment happy-dom
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BrowserWindow } from 'electron';
import type { RelayHost } from '../../../main/browserRelay';
import { BrowserProvider } from '../browser/store';
import { BrowserGlobe, BrowserPane } from '../browser/BrowserPane';

// The browser panel's Back, Forward, Reload and Stop loading (docs/contracts/browser-app.md,
// BROWSERAPP-30 to 35), as T3 Code has them: main moves the tab's own webContents
// (main/browserGuest.ts `nav`), with fake webviews; the panel asks main by tab, under happy-dom.

const the = vi.hoisted(() => ({
  guests: new Map<number, unknown>(),
  session: { setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, on() {} },
  invoked: [] as unknown[][],
  exposed: {} as Record<string, { browser: { nav: (threadId: string, tabId: string, op: string) => Promise<unknown> } }>,
}));
vi.mock('electron', () => ({
  session: { fromPartition: () => the.session },
  webContents: { fromId: (id: number) => the.guests.get(id) ?? null },
  contextBridge: { exposeInMainWorld: (name: string, api: never) => { the.exposed[name] = api; } },
  ipcRenderer: { sendSync: () => null, send: () => {}, on: () => {}, off: () => {}, invoke: async (...args: unknown[]) => { the.invoked.push(args); return null; } },
}));
const { BUSY_PAGE_PROBE_MS, hosting } = await import('../../../main/browserGuest');

// --- main: a fake window and its webviews

type Answers = 'yes' | 'reject' | 'never';
let ids = 100;
function fakeGuest(embedder: object, o: { attached?: boolean; answers?: Answers } = {}) {
  const sent: string[] = [];
  const wc = Object.assign(new EventEmitter(), {
    id: ids++, session: the.session, hostWebContents: embedder, getType: () => 'webview',
    destroyed: false, back: false, forward: false, loading: false,
    isDestroyed: () => wc.destroyed, isDevToolsOpened: () => false, isLoading: () => false, isLoadingMainFrame: () => wc.loading,
    setWindowOpenHandler() {},
    navigationHistory: { canGoBack: () => wc.back, canGoForward: () => wc.forward, goBack: vi.fn(), goForward: vi.fn() },
    reload: vi.fn(), stop: vi.fn(),
    sent,
    debugger: Object.assign(new EventEmitter(), {
      attached: o.attached ?? true,
      isAttached: () => wc.debugger.attached, attach() {}, detach() {},
      sendCommand: vi.fn((method: string) => {
        sent.push(method);
        if (method !== 'Runtime.evaluate') return Promise.resolve({});
        const a = o.answers ?? 'yes';
        return a === 'yes' ? Promise.resolve({ result: { value: 0 } }) : a === 'reject' ? Promise.reject(new Error('busy')) : new Promise(() => {});
      }),
    }),
  });
  the.guests.set(wc.id, wc);
  return wc;
}
type Guest = ReturnType<typeof fakeGuest>;

function aWindow() {
  const win = { webContents: new EventEmitter() };
  const report = vi.fn();
  const host = { attach: vi.fn(), detach: vi.fn(), forget: vi.fn(), event: vi.fn(), shown: vi.fn(), loadFailed: vi.fn() };
  const h = hosting(win as unknown as BrowserWindow, host as unknown as RelayHost, () => 'persist:oby-agent-codex', report);
  // a webview the page mounted, attached; keyed to a tab once registered
  const guest = (tabId?: string, o: Parameters<typeof fakeGuest>[1] = {}) => {
    const g = fakeGuest(win.webContents, o);
    win.webContents.emit('did-attach-webview', {}, g);
    if (tabId) expect(h.register({ threadId: 'j1', tabId }, g.id)).toBeNull();
    return g;
  };
  return { h, report, guest, win };
}
const moved = (g: Guest) => ({ back: g.navigationHistory.goBack.mock.calls.length, forward: g.navigationHistory.goForward.mock.calls.length, reload: g.reload.mock.calls.length, stop: g.stop.mock.calls.length });
const STILL = { back: 0, forward: 0, reload: 0, stop: 0 };
const NO_PAGE = 'This tab has no page in this window.';

// --- the page: the panel on obid, main stood in for

type NavState = { threadId: string; tabId: string; canGoBack: boolean; canGoForward: boolean; loading: boolean };
const tabOf = (id: string, state: string, extra: object = {}) => ({ id, owner: { kind: 'agent', job: 'j1', agent: 'codex' }, title: id, url: 'http://localhost:3000/dashboard', state, heldBy: null, holdGeneration: 0, computer: { id: 'local', name: 'This Mac' }, ask: null, dialog: null, ...extra });

let root: Root, host: HTMLDivElement;
let say: ((s: NavState) => void) | null;
let nav: ReturnType<typeof vi.fn>;
let calls: { method: string; path: string }[];
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
  say = null; calls = [];
  nav = vi.fn(async () => null);
});
afterEach(() => { act(() => root.unmount()); host.remove(); delete (window as unknown as { app?: unknown }).app; vi.useRealTimers(); });

async function onObid(tabs: object[]) {
  const call = async (method: string, path: string) => {
    calls.push({ method, path });
    return { status: 200, text: JSON.stringify(path === '/v1/browser' ? { tabs, settings: { agentsUseBrowser: true, agentsUseMac: true }, seq: 1 } : { downloads: [], seq: 1 }) };
  };
  (window as unknown as { app: unknown }).app = {
    obid: { call },
    browser: { follow: () => () => {}, fill: async () => null, reveal: async () => true, nav, navState: (fn: (s: NavState) => void) => { say = fn; return () => { say = null; }; } },
  };
  await act(async () => { root.render(<BrowserProvider board={null} live><BrowserGlobe /><BrowserPane width={640} /></BrowserProvider>); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  act(() => (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click());
}
const tell = (s: Partial<NavState>) => act(() => say!({ threadId: 'j1', tabId: 't1', canGoBack: false, canGoForward: false, loading: false, ...s }));
const button = (label: string) => host.querySelector(`.br-nav [aria-label="${label}"]`) as HTMLButtonElement | null;
const named = (words: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === words) as HTMLButtonElement | undefined;
const click = (b: HTMLButtonElement | null | undefined) => act(() => b!.click());
const pick = (i: number) => act(() => (host.querySelectorAll('.br-tab')[i] as HTMLButtonElement).click());

describe('BROWSERAPP-30: Back and Forward go back and forward on the tab\'s own page', () => {
  it('browserapp_30: the panel\'s Back and Forward ask main for that tab\'s job and tab', async () => {
    await onObid([tabOf('t1', 'ready')]);
    tell({ canGoBack: true, canGoForward: true });
    click(button('Back'));
    click(button('Forward'));
    expect(nav.mock.calls).toEqual([['j1', 't1', 'back'], ['j1', 't1', 'forward']]);
  });

  it('browserapp_30: main goes back or forward on that tab\'s webview only, and only when it has a page there', async () => {
    const w = aWindow();
    const one = w.guest('t30a'), other = w.guest('t30b');
    const k = { threadId: 'j1', tabId: 't30a' };
    expect(await w.h.nav(k, 'back')).toBeNull();
    expect(await w.h.nav(k, 'forward')).toBeNull();
    expect(moved(one)).toEqual(STILL);
    one.back = true; one.forward = true;
    expect(await w.h.nav(k, 'back')).toBeNull();
    expect(await w.h.nav(k, 'forward')).toBeNull();
    expect(moved(one)).toEqual({ ...STILL, back: 1, forward: 1 });
    expect(moved(other)).toEqual(STILL);
    expect(other.debugger.sendCommand).not.toHaveBeenCalled();
  });

  it('browserapp_30: a tab with no webview of this window\'s gets main\'s answer, and nothing moves', async () => {
    const w = aWindow();
    const unkeyed = w.guest();
    unkeyed.back = true;
    expect(await w.h.nav({ threadId: 'j1', tabId: 't30c' }, 'back')).toBe(NO_PAGE);
    const gone = w.guest('t30d');
    gone.back = true; gone.destroyed = true;
    expect(await w.h.nav({ threadId: 'j1', tabId: 't30d' }, 'back')).toBe(NO_PAGE);
    // another window's webview for that tab
    const elsewhere = aWindow().guest('t30e');
    elsewhere.back = true;
    expect(await w.h.nav({ threadId: 'j1', tabId: 't30e' }, 'reload')).toBe(NO_PAGE);
    for (const g of [unkeyed, gone, elsewhere]) { expect(moved(g)).toEqual(STILL); expect(g.debugger.sendCommand).not.toHaveBeenCalled(); }
  });

  it('browserapp_30: what Electron throws comes back as main\'s answer, never into main', async () => {
    const w = aWindow();
    const g = w.guest('t30f');
    g.reload.mockImplementation(() => { throw new Error('Object has been destroyed'); });
    expect(await w.h.nav({ threadId: 'j1', tabId: 't30f' }, 'reload')).toBe('Object has been destroyed');
  });
});

describe('BROWSERAPP-31: Back is disabled on a tab\'s first page, and Forward with nothing ahead', () => {
  it('browserapp_31: main reports the page state once registered and on each of T3\'s five events, never for an unkeyed webview', () => {
    const w = aWindow();
    const loose = w.guest();
    for (const e of ['did-navigate', 'did-navigate-in-page', 'did-start-loading', 'did-stop-loading', 'did-fail-load']) loose.emit(e, {});
    expect(w.report).not.toHaveBeenCalled();
    const g = w.guest('t31');
    expect(w.report.mock.calls).toEqual([[{ threadId: 'j1', tabId: 't31', canGoBack: false, canGoForward: false, loading: false }]]);
    g.back = true; g.loading = true;
    for (const e of ['did-navigate', 'did-navigate-in-page', 'did-start-loading', 'did-stop-loading', 'did-fail-load']) g.emit(e, {});
    expect(w.report).toHaveBeenCalledTimes(6);
    expect(w.report.mock.calls.slice(1).every(([s]) => s.canGoBack && !s.canGoForward && s.loading && s.tabId === 't31')).toBe(true);
    g.destroyed = true;
    g.emit('did-navigate', {});
    expect(w.report).toHaveBeenCalledTimes(6);
  });

  it('browserapp_31: on obid Back and Forward are disabled until main says there is a page behind or ahead', async () => {
    await onObid([tabOf('t1', 'ready')]);
    expect(button('Back')!.disabled).toBe(true);
    expect(button('Forward')!.disabled).toBe(true);
    tell({ canGoBack: true });
    expect(button('Back')!.disabled).toBe(false);
    expect(button('Forward')!.disabled).toBe(true);
    tell({ canGoForward: true });
    expect(button('Back')!.disabled).toBe(true);
    expect(button('Forward')!.disabled).toBe(false);
    // another tab's state is that tab's only
    tell({ tabId: 't2', canGoBack: true });
    expect(button('Back')!.disabled).toBe(true);
  });

  it('browserapp_31: on the demo data at board 2, Back is enabled and Forward disabled, as the board draws them', () => {
    act(() => root.render(<BrowserProvider board={2}><BrowserPane width={640} /></BrowserProvider>));
    expect(button('Back')!.disabled).toBe(false);
    expect(button('Forward')!.disabled).toBe(true);
    expect(button('Reload')!.disabled).toBe(false);
  });
});

describe('BROWSERAPP-32: Reload reloads, and while the page loads the row shows Back and Stop loading', () => {
  it('browserapp_32: on obid the row follows main\'s loading: Back then Stop loading, whose click stops it; then Reload', async () => {
    await onObid([tabOf('t1', 'ready')]);
    tell({ loading: true });
    expect([...host.querySelectorAll('.br-nav button')].map((b) => b.getAttribute('aria-label'))).toEqual(['Back', 'Stop loading']);
    expect(host.querySelector('.br-loading')).not.toBeNull();
    click(button('Stop loading'));
    tell({ loading: false });
    expect(button('Stop loading')).toBeNull();
    click(button('Reload'));
    expect(nav.mock.calls).toEqual([['j1', 't1', 'stop'], ['j1', 't1', 'reload']]);
  });

  it('browserapp_32: main reloads and stops the webview itself, never through its debugger', async () => {
    const w = aWindow();
    const g = w.guest('t32');
    expect(await w.h.nav({ threadId: 'j1', tabId: 't32' }, 'reload')).toBeNull();
    expect(await w.h.nav({ threadId: 'j1', tabId: 't32' }, 'stop')).toBeNull();
    expect(moved(g)).toEqual({ ...STILL, reload: 1, stop: 1 });
    expect(g.sent.filter((m) => m.startsWith('Page.'))).toEqual([]);
  });
});

describe('BROWSERAPP-33: a page stuck in a script is left anyway', () => {
  it('browserapp_33: with no answer in 1 s, main ends the page\'s script, then goes back, forward or reloads', async () => {
    vi.useFakeTimers();
    for (const [op, did] of [['back', 'back'], ['forward', 'forward'], ['reload', 'reload']] as const) {
      const w = aWindow();
      const g = w.guest(`t33-${op}`, { answers: 'never' });
      g.back = true; g.forward = true;
      const done = w.h.nav({ threadId: 'j1', tabId: `t33-${op}` }, op);
      await vi.advanceTimersByTimeAsync(BUSY_PAGE_PROBE_MS - 1);
      expect(g.sent).toEqual(['Runtime.evaluate']);
      expect(moved(g)).toEqual(STILL);
      await vi.advanceTimersByTimeAsync(1);
      expect(await done).toBeNull();
      expect(g.sent).toEqual(['Runtime.evaluate', 'Runtime.terminateExecution']);
      expect(g.debugger.sendCommand).toHaveBeenNthCalledWith(1, 'Runtime.evaluate', { expression: '0' });
      expect(moved(g)).toEqual({ ...STILL, [did]: 1 });
    }
  });

  it('browserapp_33: a page that answers, or refuses, isn\'t ended, and is left at once', async () => {
    vi.useFakeTimers();
    for (const answers of ['yes', 'reject'] as const) {
      const w = aWindow();
      const g = w.guest(`t33-${answers}`, { answers });
      g.back = true;
      expect(await w.h.nav({ threadId: 'j1', tabId: `t33-${answers}` }, 'back')).toBeNull();
      expect(g.sent).toEqual(['Runtime.evaluate']);
      expect(moved(g)).toEqual({ ...STILL, back: 1 });
    }
  });

  it('browserapp_33: Stop asks nothing first, nor does anything with no debugger attached (DevTools have it)', async () => {
    const w = aWindow();
    const g = w.guest('t33-stop', { answers: 'never' });
    expect(await w.h.nav({ threadId: 'j1', tabId: 't33-stop' }, 'stop')).toBeNull();
    expect(g.sent).toEqual([]);
    const off = w.guest('t33-off', { attached: false, answers: 'never' });
    off.back = true;
    expect(await w.h.nav({ threadId: 'j1', tabId: 't33-off' }, 'back')).toBeNull();
    expect(await w.h.nav({ threadId: 'j1', tabId: 't33-off' }, 'reload')).toBeNull();
    expect(off.debugger.sendCommand).not.toHaveBeenCalled();
    expect(moved(off)).toEqual({ ...STILL, back: 1, reload: 1 });
  });
});

describe('BROWSERAPP-34: board 10\'s ways out that are these calls work on obid', () => {
  it('browserapp_34: Try again, the crashed page\'s Reload and the certificate page\'s Go back move the tab\'s page; Go on anyway, Leave and Stay don\'t show', async () => {
    await onObid([tabOf('t1', 'failed'), tabOf('t2', 'timedOut'), tabOf('t3', 'crashed'), tabOf('t4', 'certificate'), tabOf('t5', 'dialog', { dialog: 'Leave this page?' })]);
    click(named('Try again'));
    pick(1);
    click(named('Try again'));
    pick(2);
    expect(host.querySelector('[data-browser="page-crashed"] button')?.textContent).toBe('Reload');
    click(host.querySelector('[data-browser="page-crashed"] button') as HTMLButtonElement);
    pick(3);
    expect(named('Go back')!.disabled).toBe(true);
    expect(named('Go on anyway')).toBeUndefined();
    tell({ tabId: 't4', canGoBack: true });
    expect(named('Go back')!.disabled).toBe(false);
    click(named('Go back'));
    pick(4);
    for (const w of ['Leave', 'Stay', 'Take control to answer']) expect(named(w), w).toBeUndefined();
    expect(nav.mock.calls).toEqual([['j1', 't1', 'reload'], ['j1', 't2', 'reload'], ['j1', 't3', 'reload'], ['j1', 't4', 'back']]);
  });

  it('browserapp_34: on the demo data at board 10, each state\'s way out shows as the board draws it', () => {
    act(() => root.render(<BrowserProvider board={10}><BrowserPane width={640} /></BrowserProvider>));
    const ways: Record<number, string[]> = { 1: ['Try again'], 2: ['Try again'], 3: ['Leave', 'Stay', 'Take control to answer'], 4: ['Go back', 'Go on anyway'], 5: ['Reload'] };
    for (const [i, words] of Object.entries(ways)) {
      pick(Number(i));
      for (const w of words) expect(named(w), w).toBeDefined();
    }
    pick(0);
    expect([...host.querySelectorAll('.br-nav button')].map((b) => b.getAttribute('aria-label'))).toEqual(['Back', 'Stop loading']);
  });
});

describe('BROWSERAPP-35: none of these acts calls obid or is signed', () => {
  it('browserapp_35: on obid, every one of these clicks reaches main\'s nav only; obid hears no call but the panel\'s reads', async () => {
    await onObid([tabOf('t1', 'ready'), tabOf('t2', 'failed'), tabOf('t3', 'crashed'), tabOf('t4', 'certificate')]);
    tell({ canGoBack: true, canGoForward: true });
    tell({ tabId: 't4', canGoBack: true });
    click(button('Back')); click(button('Forward')); click(button('Reload'));
    tell({ loading: true, canGoBack: true });
    click(button('Stop loading'));
    pick(1); click(named('Try again'));
    pick(2); click(host.querySelector('[data-browser="page-crashed"] button') as HTMLButtonElement);
    pick(3); click(named('Go back'));
    expect(nav).toHaveBeenCalledTimes(7);
    expect(calls.every((c) => c.method === 'GET')).toBe(true);
  });

  it('browserapp_35: New tab and Page options do nothing: no handler, no nav and no obid call', async () => {
    await onObid([tabOf('t1', 'ready')]);
    const before = calls.length;
    click(host.querySelector('[aria-label="New tab"]') as HTMLButtonElement);
    click(button('Page options'));
    expect(nav).not.toHaveBeenCalled();
    expect(calls.length).toBe(before);
    const panel = readFileSync(join(__dirname, '../browser/Panel.tsx'), 'utf8');
    for (const label of ['aria-label={newTabLabel}', 'aria-label="Page options"']) expect(panel.split('\n').find((l) => l.includes(label)), label).not.toMatch(/onClick/);
  });

  it('browserapp_35: the preload\'s nav invokes browser-nav with exactly the job, the tab and the act, nothing signed', async () => {
    await import('../../../preload/index');
    await the.exposed.app.browser.nav('j1', 't1', 'back');
    expect(the.invoked.at(-1)).toEqual(['browser-nav', 'j1', 't1', 'back']);
  });

  it('browserapp_35: main\'s browserGuest reaches no signer and no obid call', () => {
    const src = readFileSync(join(__dirname, '../../../main/browserGuest.ts'), 'utf8');
    expect(src).not.toMatch(/words|device|from '\.\/obid'/);
  });
});
