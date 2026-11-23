// @vitest-environment happy-dom
import { EventEmitter } from 'node:events';
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BrowserWindow } from 'electron';
import type { RelayHost } from '../../../main/browserRelay';

// The owner's own tabs and the agents' local servers with no page (docs/contracts/browser-app.md,
// BROWSERAPP-45 to 48), as T3 Code has them: its address bar (packages/shared/src/preview.ts
// resolveAddressBarInput), its empty preview's local servers (PreviewEmptyState.tsx) and its own
// webview per tab. Main with fake webviews; the panel on obid under happy-dom, main stood in for.

const the = vi.hoisted(() => ({
  guests: new Map<number, unknown>(),
  sessions: new Map<string, object>(),
  jobs: {} as Record<string, unknown>,
}));
const sessionOf = (p: string) => {
  if (!the.sessions.has(p)) the.sessions.set(p, { setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, on() {}, partition: p, clearStorageData: vi.fn(async () => undefined), clearCache: vi.fn(async () => undefined), clearAuthCache: vi.fn(async () => undefined) });
  return the.sessions.get(p)!;
};
vi.mock('electron', () => ({
  session: { fromPartition: (p: string) => sessionOf(p) },
  webContents: { fromId: (id: number) => the.guests.get(id) ?? null },
}));
vi.mock('../live', async (actual) => {
  const real = await actual<typeof import('../live')>();
  return { ...real, useLive: () => ({ ...real.useLive(), jobs: the.jobs }) };
});
const { MOUNT_MS, OWN_PARTITION, OWN_THREAD, harden, hosting } = await import('../../../main/browserGuest');
const { NotAnAddress, resolveAddress } = await import('../browser/address');
const { BrowserProvider } = await import('../browser/store');
const { BrowserGlobe, BrowserPane } = await import('../browser/BrowserPane');
const { BrowserWebviews } = await import('../browser/Webviews');

// --- main: a fake window and its webviews

let ids = 500;
function fakeGuest(embedder: object, partition: string) {
  const wc = Object.assign(new EventEmitter(), {
    id: ids++, session: sessionOf(partition), hostWebContents: embedder, getType: () => 'webview',
    destroyed: false, back: false, url: 'https://example.com/',
    getURL: () => wc.url, isDestroyed: () => wc.destroyed, isDevToolsOpened: () => false, isLoading: () => false, isLoadingMainFrame: () => false,
    opener: null as null | ((d: { url: string }) => unknown),
    setWindowOpenHandler(f: (d: { url: string }) => unknown) { wc.opener = f; },
    navigationHistory: { canGoBack: () => wc.back, canGoForward: () => false, goBack: vi.fn(), goForward: vi.fn() },
    reload: vi.fn(), stop: vi.fn(), loadURL: vi.fn(async () => undefined),
    debugger: Object.assign(new EventEmitter(), { isAttached: () => false, attach: vi.fn(), detach() {}, sendCommand: vi.fn(async () => ({})) }),
  });
  the.guests.set(wc.id, wc);
  return wc;
}
function aWindow() {
  const win = { webContents: new EventEmitter() };
  const report = vi.fn();
  const host = { attach: vi.fn(), detach: vi.fn(), forget: vi.fn(), event: vi.fn(), shown: vi.fn(), loadFailed: vi.fn() };
  const h = hosting(win as unknown as BrowserWindow, host as unknown as RelayHost, () => 'persist:oby-agent-codex', report, { computer: () => null, send: vi.fn() });
  // a webview attaching: in an owner's partition, as one the page said it mounts (`mount`, its tab) or not
  const attachAsked = (partition: string, src = 'https://example.com/') => {
    const e = { preventDefault: vi.fn() };
    win.webContents.emit('will-attach-webview', e, {}, { partition, src });
    return !e.preventDefault.mock.calls.length;
  };
  const guest = (partition = OWN_PARTITION, embedder: object = win.webContents, mount?: string) => {
    if (mount) {
      expect(h.ownMount(mount, partition, 'https://example.com/')).toBeNull();
      expect(attachAsked(partition)).toBe(true);
    }
    const g = fakeGuest(embedder, partition);
    win.webContents.emit('did-attach-webview', {}, g);
    return g;
  };
  return { h, host, report, guest, attachAsked };
}

// --- the page: the panel on obid, main stood in for

type NavState = { threadId: string; tabId: string; url: string; canGoBack: boolean; canGoForward: boolean; loading: boolean; failed: string | null };
let root: Root, box: HTMLDivElement;
let say: ((s: NavState) => void) | null;
let calls: { method: string; path: string }[];
let main: { ownMount: ReturnType<typeof vi.fn>; own: ReturnType<typeof vi.fn>; go: ReturnType<typeof vi.fn>; nav: ReturnType<typeof vi.fn>; register: ReturnType<typeof vi.fn>; shown: ReturnType<typeof vi.fn> };
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  box = document.createElement('div'); document.body.append(box); root = createRoot(box);
  say = null; calls = []; the.jobs = {};
  main = { ownMount: vi.fn(async () => null), own: vi.fn(async () => null), go: vi.fn(async () => null), nav: vi.fn(async () => null), register: vi.fn(async () => null), shown: vi.fn() };
});
afterEach(() => { act(() => root.unmount()); box.remove(); delete (window as unknown as { app?: unknown }).app; });

const agentTab = { id: 't1', owner: { kind: 'agent', job: 'j1', agent: 'codex' }, title: 'Dashboard', url: 'http://localhost:3000/dashboard', state: 'ready', heldBy: null, holdGeneration: 0, computer: { id: 'local', name: 'This Mac' }, ask: null, dialog: null };
async function onObid(tabs: object[] = []) {
  const call = async (method: string, path: string) => {
    calls.push({ method, path });
    return { status: 200, text: JSON.stringify(path === '/v1/browser' ? { tabs, settings: { agentsUseBrowser: true, agentsUseMac: true }, seq: 1 } : { downloads: [], seq: 1 }) };
  };
  (window as unknown as { app: unknown }).app = {
    obid: { call },
    browser: { follow: () => () => {}, fill: async () => null, reveal: async () => true, pointer: () => () => {}, ...main, navState: (fn: (s: NavState) => void) => { say = fn; return () => { say = null; }; } },
  };
  await act(async () => { root.render(<BrowserProvider board={null} live><BrowserGlobe /><BrowserPane width={640} /><BrowserWebviews /></BrowserProvider>); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  act(() => (box.querySelector('[data-browser="globe"]') as HTMLButtonElement).click());
}
const address = () => box.querySelector('.br-address input') as HTMLInputElement | null;
const setValue = (el: HTMLInputElement, v: string) => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(el, v);
  el.dispatchEvent(new Event('input', { bubbles: true }));
};
async function type(words: string) {
  const el = address()!;
  act(() => el.focus());
  act(() => setValue(el, words));
  await act(async () => { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await new Promise((r) => setTimeout(r, 0)); });
}
const webviews = () => [...box.querySelectorAll('webview')] as HTMLElement[];
const tabs = () => [...box.querySelectorAll('.br-tab .br-tab-name')].map((t) => t.textContent);
const tell = (s: Partial<NavState>) => act(() => say!({ threadId: OWN_THREAD, tabId: '', url: '', canGoBack: false, canGoForward: false, loading: false, failed: null, ...s }));
const ownTabId = () => (box.querySelector('[data-browser="webview"]') as HTMLElement).dataset.tab!;

describe('BROWSERAPP-45: typing an address opens a tab of the owner\'s own', () => {
  it('browserapp_45: a bare host, localhost:5173 or a full URL becomes an address as T3 Code\'s bar makes it, other words a search', () => {
    expect(resolveAddress('localhost:5173')).toBe('http://localhost:5173/');
    expect(resolveAddress('127.0.0.1:8080/app')).toBe('http://127.0.0.1:8080/app');
    expect(resolveAddress('example.com')).toBe('https://example.com/');
    expect(resolveAddress('  https://example.com/a?b=1 ')).toBe('https://example.com/a?b=1');
    expect(resolveAddress('http://localhost:3000')).toBe('http://localhost:3000/');
    expect(resolveAddress('how do webviews work')).toBe('https://duckduckgo.com/?q=how%20do%20webviews%20work');
  });

  it('browserapp_45: with no tab, the address takes what the owner types; Enter opens it in a webview of their own partition, kept by main, with no obid call', async () => {
    await onObid();
    expect(box.querySelector('.br-address')?.textContent).toBe('Type an address');
    await type('localhost:5173');
    const [w] = webviews();
    expect(webviews()).toHaveLength(1);
    expect(w.getAttribute('partition')).toBe('persist:oby-owner');
    expect(w.getAttribute('src')).toBe('http://localhost:5173/');
    // the tab is the owner's: a Y, its strip says so, no Take control, and obid hears only the panel's reads
    expect(box.querySelector('.br-tab [aria-label="Yours"]')).not.toBeNull();
    expect(box.querySelector('[data-browser="control"] .br-control-text > span:last-child')?.textContent).toBe('Your tab. No agent uses it unless you ask.');
    expect([...box.querySelectorAll('button')].some((b) => b.textContent?.includes('Take control'))).toBe(false);
    expect(calls.every((c) => c.method === 'GET')).toBe(true);
    // attached, its webview goes to main as the owner's own (browser-own), never registered as obid's tab or shown to obid
    Object.assign(w, { getWebContentsId: () => 42 });
    await act(async () => { w.dispatchEvent(new Event('did-attach')); });
    expect(main.own).toHaveBeenCalledWith(ownTabId(), 42);
    expect(main.register).not.toHaveBeenCalled();
    expect(main.shown).not.toHaveBeenCalled();
  });

  it('browserapp_45: main keeps a webview of the owner\'s partition as theirs, never relays it, and refuses any other', () => {
    const w = aWindow();
    const own = w.guest(OWN_PARTITION, undefined, 'own-a-1');
    expect(w.h.own('own-a-1', own.id)).toBeNull();
    // told as the owner's own (threadId ''), never to the relay
    expect(w.report.mock.calls.at(-1)![0]).toMatchObject({ threadId: OWN_THREAD, tabId: 'own-a-1', url: 'https://example.com/' });
    own.emit('did-fail-load', {}, -102, 'ERR_CONNECTION_REFUSED', 'http://localhost:9/', true);
    own.debugger.emit('message', {}, 'Page.frameNavigated', {});
    own.emit('render-process-gone', {}, { reason: 'crashed' });
    expect(w.host.loadFailed).not.toHaveBeenCalled();
    expect(w.host.event).not.toHaveBeenCalled();
    expect(w.host.detach).not.toHaveBeenCalled();
    expect(w.host.attach).not.toHaveBeenCalled();
    expect(own.debugger.attach).not.toHaveBeenCalled();
    // obid can't take it as an agent tab's, and an agent's or another window's webview isn't the owner's
    expect(w.h.register({ threadId: 'j1', tabId: 't1' }, own.id)).not.toBeNull();
    expect(w.h.own('own-a-2', w.guest('persist:oby-agent-codex').id)).toBe('Not the owner’s own tab.');
    expect(w.h.own('own-a-3', w.guest(OWN_PARTITION, {}).id)).toBe('Not one of this window’s pages.');
    // one webview is one tab's: another tab id can't take it
    expect(w.h.own('own-a-4', own.id)).not.toBeNull();
  });

  it('browserapp_45: a webview attaches in the owner\'s partition only for a web page, never the app\'s own', () => {
    expect(harden({}, { partition: OWN_PARTITION, src: 'http://localhost:5173/' }, null)).toBe(true);
    expect(harden({}, { partition: OWN_PARTITION, src: 'file:///etc/hosts' }, null)).toBe(false);
    expect(harden({}, { partition: OWN_PARTITION, src: 'about:blank' }, null)).toBe(false);
    expect(harden({}, { partition: 'persist:oby-owner-2', src: 'https://example.com/' }, null)).toBe(false);
  });
});

describe('BROWSERAPP-46: the owner\'s own tab opens web pages only, and moves on their word alone', () => {
  it('browserapp_46: a file:, chrome:, devtools: or javascript: address opens nothing and marks the address with why', async () => {
    await onObid();
    for (const bad of ['file:///etc/passwd', 'chrome://settings', 'devtools://devtools/bundled/inspector.html', 'javascript:alert(1)']) {
      expect(() => resolveAddress(bad), bad).toThrow(NotAnAddress);
      await type(bad);
      expect(webviews(), bad).toHaveLength(0);
      expect(box.querySelector('.br-address')?.classList.contains('is-warn'), bad).toBe(true);
      expect(box.querySelector('.br-address')?.getAttribute('title')).toBe('Oby opens only web pages.');
    }
  });

  it('browserapp_46: main loads only a web page in the owner\'s tab, never the app\'s own, and moves only that tab', async () => {
    const w = aWindow();
    const own = w.guest(OWN_PARTITION, undefined, 'own-a-1'), agent = w.guest('persist:oby-agent-codex');
    expect(w.h.own('own-a-1', own.id)).toBeNull();
    expect(w.h.register({ threadId: 'j1', tabId: 't1' }, agent.id)).toBeNull();
    for (const bad of ['file:///etc/passwd', 'chrome://settings', 'about:blank', 'javascript:alert(1)']) expect(w.h.go('own-a-1', bad), bad).toBe('Oby opens only web pages.');
    expect(own.loadURL).not.toHaveBeenCalled();
    expect(w.h.go('own-a-1', 'https://example.org/')).toBeNull();
    expect(own.loadURL.mock.calls).toEqual([['https://example.org/']]);
    expect(w.h.go('own-a-9', 'https://example.org/')).toBe('This tab has no page in this window.');
    // Back on the owner's tab moves its own webview, never the agent's
    own.back = true; agent.back = true;
    expect(await w.h.nav({ threadId: OWN_THREAD, tabId: 'own-a-1' }, 'back')).toBeNull();
    expect(own.navigationHistory.goBack).toHaveBeenCalledTimes(1);
    expect(agent.navigationHistory.goBack).not.toHaveBeenCalled();
    // a destroyed owner's webview is forgotten
    own.emit('destroyed');
    expect(w.h.go('own-a-1', 'https://example.org/')).toBe('This tab has no page in this window.');
  });

  it('browserapp_46: typed in a tab with a page, the address goes to main for that tab; Back reaches main\'s nav under the owner\'s key', async () => {
    await onObid();
    await type('example.com');
    const id = ownTabId();
    tell({ tabId: id, url: 'https://example.com/', canGoBack: true });
    expect(box.querySelector('.br-address')?.textContent).toBe('example.com');
    await type('localhost:3000/login');
    expect(main.go).toHaveBeenCalledWith(id, 'http://localhost:3000/login');
    expect(webviews()).toHaveLength(1);
    act(() => (box.querySelector('.br-nav [aria-label="Back"]') as HTMLButtonElement).click());
    expect(main.nav).toHaveBeenCalledWith(OWN_THREAD, id, 'back');
    expect(calls.every((c) => c.method === 'GET')).toBe(true);
  });
});

describe('BROWSERAPP-47: with no page, the local servers the agents are running', () => {
  const job = (id: string, title: string, servers?: object[]) => ({ id, title, status: 'running', servers });

  it('browserapp_47: each server a row by port, naming its process and task; one click opens it in a tab of the owner\'s own', async () => {
    the.jobs = { j1: job('j1', 'Fix the sign-up form', [{ port: 5173, process: 'node', firstSeen: '2026-11-23T10:00:00Z' }]), j2: job('j2', 'API docs', [{ port: 3000, process: 'python3', firstSeen: '2026-11-23T10:01:00Z' }]), j3: job('j3', 'Idle') };
    await onObid();
    const rows = [...box.querySelectorAll('[data-browser="servers"] .br-server')];
    expect(rows.map((r) => r.textContent)).toEqual(['localhost:3000python3 · API docs', 'localhost:5173node · Fix the sign-up form']);
    expect(box.querySelector('.empty-words')).toBeNull();
    await act(async () => { (rows[1] as HTMLButtonElement).click(); await new Promise((r) => setTimeout(r, 0)); });
    // a server opens in a throwaway partition of its tab's own, never the owner's (cookies don't keep ports apart)
    const [w] = webviews();
    expect([w.getAttribute('partition'), w.getAttribute('src')]).toEqual([`oby-owner-tab-${ownTabId()}`, 'http://localhost:5173/']);
    expect(main.ownMount).toHaveBeenCalledWith(ownTabId(), `oby-owner-tab-${ownTabId()}`, 'http://localhost:5173/');
    expect(calls.every((c) => c.method === 'GET')).toBe(true);
  });

  it('browserapp_47: with no server, board 17\'s words stay', async () => {
    the.jobs = { j3: job('j3', 'Idle') };
    await onObid();
    expect(box.querySelector('[data-browser="servers"]')).toBeNull();
    expect(box.querySelector('.empty-words')?.textContent).toBe('No pages yet.\nWhen an agent opens one, you see it here, live.');
  });
});

describe('BROWSERAPP-48: New tab opens a blank tab of the owner\'s own, and its × closes it', () => {
  it('browserapp_48: New tab shows a blank tab of the owner\'s, its address ready and the servers under it; typed in, its page opens there', async () => {
    the.jobs = { j1: { id: 'j1', title: 'Fix the sign-up form', status: 'running', servers: [{ port: 5173, process: 'node', firstSeen: '2026-11-23T10:00:00Z' }] } };
    await onObid([agentTab]);
    expect(tabs()).toEqual(['Dashboard']);
    act(() => (box.querySelector('[aria-label="New tab"]') as HTMLButtonElement).click());
    expect(tabs()).toEqual(['Dashboard', 'New tab']);
    expect(document.activeElement).toBe(address());
    expect(box.querySelectorAll('[data-browser="servers"] .br-server')).toHaveLength(1);
    expect(webviews()).toHaveLength(0);
    await type('example.com');
    expect(tabs()).toEqual(['Dashboard', 'example.com']);
    expect(webviews().map((w) => w.getAttribute('src'))).toEqual(['https://example.com/']);
    expect(main.nav).not.toHaveBeenCalled();
    expect(calls.every((c) => c.method === 'GET')).toBe(true);
  });

  it('browserapp_48: the owner\'s tab\'s × closes it, its page with it, and shows the tab before it', async () => {
    await onObid([agentTab]);
    act(() => (box.querySelector('[aria-label="New tab"]') as HTMLButtonElement).click());
    await type('example.com');
    expect(webviews()).toHaveLength(1);
    act(() => (box.querySelector('[aria-label="Close tab"]') as HTMLElement).click());
    expect(tabs()).toEqual(['Dashboard']);
    expect(webviews()).toHaveLength(0);
    expect(box.querySelector('.br-tab.is-on .br-tab-name')?.textContent).toBe('Dashboard');
    // an agent's tab has no close of the owner's
    expect(box.querySelector('[aria-label="Close tab"]')).toBeNull();
  });
});

describe('BROWSERAPP-24: an owner\'s partition holds only the owner\'s own tabs, never one obid names', () => {
  it('browserapp_24: an obid tab naming the owner\'s partition, or a server tab\'s, mounts no webview', async () => {
    await onObid([{ ...agentTab, partition: 'persist:oby-owner' }, { ...agentTab, id: 't2', partition: 'oby-owner-tab-own-a-1' }, { ...agentTab, id: 't3', partition: 'persist:oby-agent-codex' }]);
    expect(webviews().map((w) => w.getAttribute('partition'))).toEqual(['persist:oby-agent-codex']);
    expect(main.ownMount).not.toHaveBeenCalled();
  });

  it('browserapp_24: main lets a webview attach in an owner\'s partition only once for an owner\'s tab the page said it mounts, at that address', () => {
    const w = aWindow();
    // obid's or anyone's, unannounced: refused
    expect(w.attachAsked(OWN_PARTITION)).toBe(false);
    expect(w.attachAsked('oby-owner-tab-own-a-1')).toBe(false);
    // announced: once, in that partition, at that address
    expect(w.h.ownMount('own-a-1', OWN_PARTITION, 'https://example.com/')).toBeNull();
    expect(w.attachAsked(OWN_PARTITION, 'https://example.org/')).toBe(false);
    expect(w.attachAsked(OWN_PARTITION)).toBe(true);
    expect(w.attachAsked(OWN_PARTITION)).toBe(false);
    // only the owner's partition or that tab's own throwaway one, a tab id of the page's shape, and a web page
    expect(w.h.ownMount('own-a-1', 'oby-owner-tab-own-a-2', 'https://example.com/')).toBe('Not the owner’s own tab.');
    expect(w.h.ownMount('own-a-1', 'persist:oby-agent-codex', 'https://example.com/')).toBe('Not the owner’s own tab.');
    expect(w.h.ownMount('t1', OWN_PARTITION, 'https://example.com/')).toBe('Not one of the owner’s tabs.');
    expect(w.h.ownMount('own-a-1', OWN_PARTITION, 'file:///etc/hosts')).toBe('Oby opens only web pages.');
    // an agent's partition needs no word of the page's (obid names it, BROWSERAPP-24 as before)
    expect(w.attachAsked('persist:oby-agent-codex')).toBe(true);
  });

  it('browserapp_24: the page\'s word that it mounts a tab lapses unused after MOUNT_MS', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    try {
      const w = aWindow();
      expect(w.h.ownMount('own-a-1', OWN_PARTITION, 'https://example.com/')).toBeNull();
      vi.advanceTimersByTime(MOUNT_MS + 1);
      expect(w.attachAsked(OWN_PARTITION)).toBe(false);
    } finally { vi.useRealTimers(); }
  });
});

describe('BROWSERAPP-46: an owner\'s tab never reaches the app\'s own page, by a link, a new window or a redirect', () => {
  it('browserapp_46: in a development build, a link, window.open or a redirect to the app\'s dev server is refused in the owner\'s tab', () => {
    const dev = process.env.ELECTRON_RENDERER_URL;
    process.env.ELECTRON_RENDERER_URL = 'http://localhost:5173/';
    try {
      const w = aWindow();
      const own = w.guest(OWN_PARTITION, undefined, 'own-a-1');
      const navigate = (event: string, url: string) => { const ev = { preventDefault: vi.fn() }; own.emit(event, ev, url); return ev.preventDefault.mock.calls.length === 0; };
      for (const app of ['http://127.0.0.1:5173/', 'http://localhost:5173/#/settings', 'http://app.localhost:5173/']) {
        expect(navigate('will-navigate', app), app).toBe(false);
        expect(navigate('will-redirect', app), app).toBe(false);
        expect(own.opener!({ url: app })).toEqual({ action: 'deny' });
      }
      expect(own.loadURL).not.toHaveBeenCalled();
      expect(navigate('will-navigate', 'https://example.org/')).toBe(true);
      expect(navigate('will-redirect', 'https://example.org/next')).toBe(true);
      own.opener!({ url: 'https://example.org/popup' });
      expect(own.loadURL.mock.calls).toEqual([['https://example.org/popup']]);
    } finally {
      if (dev === undefined) delete process.env.ELECTRON_RENDERER_URL; else process.env.ELECTRON_RENDERER_URL = dev;
    }
  });
});

describe('BROWSERAPP-47: a server opens in a throwaway partition, cleared with its tab', () => {
  it('browserapp_47: main keeps a server tab in its own throwaway partition and clears it when its page goes', () => {
    const w = aWindow();
    const scratch = 'oby-owner-tab-own-a-1';
    const g = w.guest(scratch, undefined, 'own-a-1');
    expect(w.h.own('own-a-1', g.id)).toBeNull();
    const s = the.sessions.get(scratch) as { clearStorageData: ReturnType<typeof vi.fn> };
    expect(s.clearStorageData).not.toHaveBeenCalled();
    g.emit('destroyed');
    expect(s.clearStorageData).toHaveBeenCalledTimes(1);
    // the owner's own persistent partition is never cleared so
    const o = w.guest(OWN_PARTITION, undefined, 'own-a-2');
    expect(w.h.own('own-a-2', o.id)).toBeNull();
    o.emit('destroyed');
    expect((the.sessions.get(OWN_PARTITION) as { clearStorageData: ReturnType<typeof vi.fn> }).clearStorageData).not.toHaveBeenCalled();
  });
});

describe('BROWSERAPP-48: closing a tab shows the one before it', () => {
  it('browserapp_48: closing B of A, B and C shows A', async () => {
    await onObid([agentTab]);
    act(() => (box.querySelector('[aria-label="New tab"]') as HTMLButtonElement).click());
    await type('example.com');
    act(() => (box.querySelector('[aria-label="New tab"]') as HTMLButtonElement).click());
    await type('example.org');
    expect(tabs()).toEqual(['Dashboard', 'example.com', 'example.org']);
    act(() => (box.querySelectorAll('.br-tab')[1] as HTMLButtonElement).click());
    act(() => (box.querySelector('[aria-label="Close tab"]') as HTMLElement).click());
    expect(tabs()).toEqual(['Dashboard', 'example.org']);
    expect(box.querySelector('.br-tab.is-on .br-tab-name')?.textContent).toBe('Dashboard');
  });
});

