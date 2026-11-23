// @vitest-environment happy-dom
import { EventEmitter } from 'node:events';
import { Fragment, createElement, act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BrowserWindow } from 'electron';
import { namedPartitions, relayCommand, relayHost, type BoxCommand, type Guest, type RelayEvent, type RelayHost } from '../../../main/browserRelay';
import { windowPartition } from '../../../shared/browser-own';

// Another computer's browser in this Mac's window (docs/contracts/browser-app.md, BROWSERAPP-50 to
// 52), as T3 Code routes a remote environment's preview: each agent partition one of that
// computer's own, its loopback only through that computer's door (main/browserGuest.ts), and board
// 14's words for the computer in use (browser/live.ts), on fake sessions, a fake main and a fake obid.

type Handler = (details: { url: string }, callback: (r: { cancel?: boolean; redirectURL?: string }) => void) => void;
type FakeSession = { partition: string; requests: Handler[]; proxies: unknown[]; log: string[] };
const the = vi.hoisted(() => ({
  sessions: new Map<string, unknown>(), guests: new Map<number, unknown>(),
  computer: null as string | null,
  // what main's send answers a door's ask with: obid's answer, or one that never comes
  door: (): Promise<unknown> => new Promise(() => {}),
}));
function fakeSession(partition: string) {
  const s = {
    partition, requests: [] as Handler[], proxies: [] as unknown[], log: [] as string[],
    setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, on() {},
    registerPreloadScript: () => '',
    webRequest: { onBeforeRequest: (h: Handler) => { s.requests.push(h); } },
    setProxy: async (config: unknown) => { s.proxies.push(config); s.log.push('setProxy'); },
    closeAllConnections: async () => { s.log.push('closeAllConnections'); },
  };
  return s;
}
vi.mock('electron', () => ({
  app: { getPath: () => '/nonexistent' },
  session: { fromPartition: (p: string) => { if (!the.sessions.has(p)) the.sessions.set(p, fakeSession(p)); return the.sessions.get(p); } },
  webContents: { fromId: (id: number) => the.guests.get(id) ?? null },
}));
// main's send to this Mac's obid, faked: what main/browser.ts hands the window's webviews
const send = vi.fn((..._args: unknown[]) => the.door());
const inUse = { computer: () => the.computer, send };
const { BOX_HOLD_MS, boxDoor, hosting } = await import('../../../main/browserGuest');
const { session } = await import('electron');
const sessionOf = (p: string) => session.fromPartition(p) as unknown as FakeSession;
const made = (p: string) => the.sessions.has(p);
const settle = async () => { for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0)); };

const token = 'b'.repeat(64);
const doorAnswer = (port: number) => async () => new Response(JSON.stringify({ proxy: `http://oby:${token}@127.0.0.1:${port}` }), { status: 200 });
const tab = (job: string, id: string, partition: string) => JSON.stringify({ seq: 1, type: 'tab', tab: { id, owner: { kind: 'agent', job, agent: 'claude' }, url: 'http://localhost:3000/', partition } });

// a request on the session: what it was answered, or 'held' while it waits
function request(s: FakeSession, url = 'http://localhost:3000/') {
  const answer: { got: { cancel?: boolean; redirectURL?: string } | 'held' } = { got: 'held' };
  for (const h of s.requests) h({ url }, (r) => { answer.got = r; s.log.push(`answered ${url}`); });
  return answer;
}
function aWindow(named: () => string | undefined = () => undefined) {
  const win = { webContents: new EventEmitter() };
  const host = { attach: vi.fn(), detach: vi.fn(), forget: vi.fn(), event: vi.fn(), shown: vi.fn(), loadFailed: vi.fn() };
  const h = hosting(win as unknown as BrowserWindow, host as unknown as RelayHost, () => named(), () => {}, inUse);
  const attaching = (partition: string) => {
    const e = { preventDefault: vi.fn() };
    win.webContents.emit('will-attach-webview', e, {}, { partition, src: 'http://localhost:3000/' });
    return e.preventDefault.mock.calls.length === 0;
  };
  const guest = (partition: string) => {
    const wc = Object.assign(new EventEmitter(), {
      id: Math.floor(Math.random() * 1e9), session: session.fromPartition(partition),
      setWindowOpenHandler() {}, setWebRTCIPHandlingPolicy: vi.fn(),
      getType: () => 'webview', hostWebContents: win.webContents, isDestroyed: () => false, isDevToolsOpened: () => false,
      isLoading: () => false, isLoadingMainFrame: () => false, getURL: () => 'http://localhost:3000/',
      navigationHistory: { canGoBack: () => false, canGoForward: () => false },
      debugger: Object.assign(new EventEmitter(), { isAttached: () => true, attach() {}, sendCommand: vi.fn(async () => ({})) }),
    });
    the.guests.set(wc.id, wc);
    win.webContents.emit('did-attach-webview', {}, wc);
    return wc;
  };
  return { attaching, guest, h };
}
function login(wc: EventEmitter, authInfo: { isProxy: boolean; host: string; port: number }) {
  let answered: unknown[] | null = null;
  wc.emit('login', { preventDefault() {} }, {}, { scheme: 'basic', realm: 'oby', ...authInfo }, (...args: unknown[]) => { answered = args; });
  return answered;
}

beforeEach(() => { the.computer = null; the.door = () => new Promise(() => {}); send.mockClear(); });
afterEach(() => { vi.useRealTimers(); });

describe('BROWSERAPP-50: another computer\'s agent tab opens in a partition of that computer\'s own', () => {
  it('browserapp_50_another_computers_tab_opens_in_its_own_partition', () => {
    const ID = '0123456789abcd45', OTHER = 'fedcba9876543245';
    // on devbox: obid's agent partition opens as devbox's own, named and checked at will-attach-webview
    the.computer = ID;
    const p = namedPartitions(() => the.computer);
    expect(p.heard('tab', tab('j45', 't1', 'persist:oby-agent-claude'))).toBe('kept');
    const mapped = `persist:oby-agent-claude-on-${ID}`;
    expect(p.of({ threadId: 'j45', tabId: 't1' })).toBe(mapped);
    const w = aWindow(() => p.of({ threadId: 'j45', tabId: 't1' }));
    expect(w.attaching(mapped)).toBe(true);
    const wc = w.guest(mapped);
    expect(w.h.register({ threadId: 'j45', tabId: 't1' }, wc.id)).toBeNull();
    // this Mac's own partition, or another computer's, isn't taken while devbox is in use
    expect(w.attaching('persist:oby-agent-claude')).toBe(false);
    expect(w.attaching(`persist:oby-agent-claude-on-${OTHER}`)).toBe(false);
    expect(made(`persist:oby-agent-claude-on-${OTHER}`)).toBe(false);
    // a box on another computer: refused, and never attached
    expect(p.heard('tab', tab('j45', 't2', 'oby-box-j45'))).toBe('refused');
    expect(p.of({ threadId: 'j45', tabId: 't2' })).toBeUndefined();
    expect(w.attaching('oby-box-j45')).toBe(false);
    expect(made('oby-box-j45')).toBe(false);
    // a bad id gives no partition
    expect(windowPartition('persist:oby-agent-claude', 'ABCDEF0123456789')).toBeNull();
    expect(windowPartition('persist:oby-agent-claude', '0123')).toBeNull();
    // on This Mac: obid's name stands, and no `-on-<id>` partition is taken
    the.computer = null;
    const q = namedPartitions(() => the.computer);
    expect(q.heard('tab', tab('j45b', 't1', 'persist:oby-agent-claude'))).toBe('kept');
    expect(q.of({ threadId: 'j45b', tabId: 't1' })).toBe('persist:oby-agent-claude');
    expect(w.attaching('persist:oby-agent-claude')).toBe(true);
    expect(w.attaching(mapped)).toBe(false);
    // the webview devbox's tab is in no longer counts as its tab's on This Mac
    expect(w.h.register({ threadId: 'j45', tabId: 't1' }, wc.id)).toBe('Not the partition obid named for this tab.');
  });
});

describe('BROWSERAPP-51: a computer partition\'s loopback goes only to that computer\'s door', () => {
  it('browserapp_51_its_loopback_goes_only_to_the_computers_door', async () => {
    const ID = '0123456789abcd46', partition = `persist:oby-agent-codex-on-${ID}`;
    the.computer = ID;
    let answer!: (r: unknown) => void;
    the.door = () => new Promise((r) => { answer = r; });
    const w = aWindow();
    expect(w.attaching(partition)).toBe(true);
    const s = sessionOf(partition);
    // the door is this Mac's own route, asked with `here`
    expect(send).toHaveBeenCalledWith('POST', `/v1/computers/${ID}/door`, undefined, undefined, undefined, true);
    // a request before the door is set waits; the proxy is the door with T3's bypass, every
    // connection closed, then the held request goes
    const early = request(s);
    expect(early.got).toBe('held');
    answer(await doorAnswer(50146)());
    await settle();
    expect(s.proxies).toEqual([{ mode: 'fixed_servers', proxyRules: '127.0.0.1:50146', proxyBypassRules: '*,<-loopback>,169.254.0.0/16,fe80::/10' }]);
    expect(s.log).toEqual(['setProxy', 'closeAllConnections', 'answered http://localhost:3000/']);
    expect(early.got).toEqual({});
    expect(send).toHaveBeenCalledTimes(1);
    // 0.0.0.0 and [::] are that computer's loopback: to localhost, and never a WebSocket
    expect(request(s, 'http://0.0.0.0:3000/').got).toEqual({ redirectURL: 'http://localhost:3000/' });
    expect(request(s, 'ws://0.0.0.0/').got).toEqual({ cancel: true });
    expect(request(s, 'ws://[::]:3000/').got).toEqual({ cancel: true });
    expect(request(s, 'https://example.com/').got).toEqual({});
    // the door's login only for the door, as a proxy
    const wc = w.guest(partition);
    expect(login(wc, { isProxy: true, host: '127.0.0.1', port: 50146 })).toEqual(['oby', token]);
    expect(login(wc, { isProxy: true, host: '127.0.0.1', port: 50147 })).toEqual([]);
    expect(login(wc, { isProxy: true, host: 'localhost', port: 50146 })).toEqual([]);
    expect(login(wc, { isProxy: false, host: '127.0.0.1', port: 50146 })).toEqual([]);
    expect(wc.setWebRTCIPHandlingPolicy).not.toHaveBeenCalled();
    // a box while a computer is in use: no session, no proxy
    await boxDoor(relayCommand('box', JSON.stringify({ threadId: 'j46', partition: 'oby-box-j46', proxy: `http://oby:${token}@127.0.0.1:50148` })) as BoxCommand, the.computer);
    expect(made('oby-box-j46')).toBe(false);
    // back on This Mac: the computer's session stays routed
    the.computer = null;
    expect(request(s).got).toEqual({});
    expect(s.proxies).toHaveLength(1);
  });

  it('browserapp_51: with no door in BOX_HOLD_MS a request is cancelled, never let through; the same when the door call fails, and the next request asks again', async () => {
    vi.useFakeTimers();
    const ID = '0123456789abcd47';
    the.computer = ID;
    aWindow().attaching(`persist:oby-agent-claude-on-${ID}`);
    const s = sessionOf(`persist:oby-agent-claude-on-${ID}`);
    const r = request(s);
    vi.advanceTimersByTime(BOX_HOLD_MS - 1);
    expect(r.got).toBe('held');
    vi.advanceTimersByTime(1);
    expect(r.got).toEqual({ cancel: true });
    // a door call that fails: held, then cancelled, and asked again on a later request
    const FAILS = '0123456789abcd48';
    the.computer = FAILS;
    the.door = async () => ({ status: 0, text: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' });
    aWindow().attaching(`persist:oby-agent-claude-on-${FAILS}`);
    const f = sessionOf(`persist:oby-agent-claude-on-${FAILS}`);
    await vi.advanceTimersByTimeAsync(0);
    const held = request(f);
    await vi.advanceTimersByTimeAsync(BOX_HOLD_MS);
    expect(held.got).toEqual({ cancel: true });
    expect(f.proxies).toEqual([]);
    expect(send.mock.calls.filter((c) => c[1] === `/v1/computers/${FAILS}/door`)).toHaveLength(2);
    // a door of another shape is no door either
    the.door = async () => new Response(JSON.stringify({ proxy: 'http://127.0.0.1:3000' }), { status: 200 });
    const again = request(f);
    await vi.advanceTimersByTimeAsync(BOX_HOLD_MS);
    expect(again.got).toEqual({ cancel: true });
    expect(f.proxies).toEqual([]);
  });
});

describe('BROWSERAPP-51: another computer\'s downloads folder is that computer\'s', () => {
  it('browserapp_51: with no folder of this Mac\'s obid, a page\'s downloads are kept nowhere here, and obid\'s setDownloadBehavior is answered', async () => {
    const posted: RelayEvent[] = [];
    const folders: (string | null)[] = [];
    const guest = { send: vi.fn(async () => ({})), downloadsTo: (f: string | null) => { folders.push(f); }, passwordFields: async () => [] } as unknown as Guest;
    const k = { threadId: 'j46d', tabId: 't1' };
    // this Mac's obid's home, then another computer's in use: none
    let home: string | null = '/mac/obid';
    const relay = relayHost(async (e) => { posted.push(...e); return true; }, { get home() { return home; } });
    relay.attach(k, guest);
    const download = (id: number, downloadPath: string) => relay.command({ ...k, type: 'cdp', message: JSON.stringify({ id, method: 'Page.setDownloadBehavior', params: { behavior: 'allow', downloadPath } }) });
    const answer = (id: number) => posted.map((e) => (e.type === 'cdp' ? JSON.parse(e.message) : null)).find((m) => m?.id === id);
    download(1, '/devbox/obid/browser/downloads/j46d');
    await settle();
    expect(answer(1).error).toBeDefined();
    home = null;
    download(2, '/devbox/obid/browser/downloads/j46d');
    await settle();
    expect(answer(2)).toEqual({ id: 2, result: {} });
    expect(folders).toEqual([null]);
  });
});

describe('BROWSERAPP-51: on another computer the local servers are that computer\'s', () => {
  it('browserapp_51: on another computer no local server is listed, since an owner\'s tab would open this Mac\'s localhost; on This Mac they are', async () => {
    const { runningServers } = await import('../browser/Servers');
    const jobs = { j1: { id: 'j1', title: 'Dashboard', servers: [{ port: 3000, process: 'node' }] } } as unknown as Parameters<typeof runningServers>[0];
    expect(runningServers(jobs, null).map((v) => v.port)).toEqual([3000]);
    expect(runningServers(jobs, '0123456789abcd51')).toEqual([]);
  });
});

// ---- the page: board 14's words for the computer in use ----

const { BrowserProvider } = await import('../browser/store');
const { BrowserGlobe, BrowserPane } = await import('../browser/BrowserPane');
const { BrowserWebviews } = await import('../browser/Webviews');
const { resetComputers, startComputers } = await import('../computers/store');
type ComputersApi = Parameters<typeof startComputers>[0];
type ComputerList = Awaited<ReturnType<NonNullable<ComputersApi>['list']>>;

describe('BROWSERAPP-52: board 14\'s words show for the computer in use, whatever the tab says', () => {
  let root: Root, host: HTMLDivElement;
  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div'); document.body.append(host); root = createRoot(host);
  });
  afterEach(() => { act(() => root.unmount()); host.remove(); delete (window as unknown as { app?: unknown }).app; resetComputers(); });

  it('browserapp_52_board_14s_words_show_for_the_computer_in_use', async () => {
    const ID = '0123456789abcd49';
    const devbox: ComputerList = { current: ID, computers: [{ id: ID, name: 'devbox', os: 'linux', state: 'connected' }] };
    let take: ((l: ComputerList) => void) | null = null;
    await act(async () => {
      await startComputers({ list: async () => devbox, follow: (on: (l: ComputerList) => void) => { take = on; return () => {}; } } as unknown as ComputersApi);
    });
    // devbox's obid calls its own tab `local`; This Mac's list, once asked, waits until the test lets it go
    const devboxTab = { id: 't1', owner: { kind: 'agent', job: 'j1', agent: 'claude' }, title: 'Dashboard', url: 'http://localhost:3000/dashboard', state: 'ready', heldBy: null, holdGeneration: 0, computer: { id: 'local', name: 'This Mac' }, ask: null, dialog: null, partition: 'persist:oby-agent-claude' };
    const macTab = { ...devboxTab, id: 'm1', owner: { kind: 'agent', job: 'mj', agent: 'claude' }, title: 'The Mac page' };
    let onMac = false;
    let letMacGo!: () => void;
    const macList = new Promise<void>((r) => { letMacGo = r; });
    const call = async (_m: string, path: string) => {
      if (onMac) await macList;
      return { status: 200, text: JSON.stringify(path === '/v1/browser' ? { tabs: [onMac ? macTab : devboxTab], settings: { agentsUseBrowser: true, agentsUseMac: true }, seq: 1 } : { downloads: [], seq: 1 }) };
    };
    (window as unknown as { app: unknown }).app = {
      obid: { call },
      browser: { follow: () => () => {}, fill: async () => null, reveal: async () => true, navState: () => () => {}, register: async () => null, shown: () => {}, pointer: () => () => {} },
    };
    await act(async () => {
      root.render(createElement(BrowserProvider, { board: null, live: true, children: createElement(Fragment, null, createElement(BrowserGlobe), createElement(BrowserPane, { width: 640 }), createElement(BrowserWebviews)) }));
    });
    await act(async () => { await settle(); });
    await act(async () => { (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click(); });
    const strip = 'This browser runs on devbox. Its localhost is devbox’s, not this Mac’s.';
    const text = () => host.textContent ?? '';
    const newTab = () => [...host.querySelectorAll('button')].map((b) => b.getAttribute('aria-label')).filter((l) => l?.startsWith('New tab'));
    expect(text()).toContain(strip);
    expect(host.querySelector('.br-where')?.textContent).toBe('on devbox');
    expect(newTab()).toEqual(['New tab on devbox']);
    expect([...host.querySelectorAll('webview')].map((w) => w.getAttribute('partition'))).toEqual([`persist:oby-agent-claude-on-${ID}`]);
    // back on This Mac: devbox's tab goes before This Mac's list is drawn, then none of the three shows
    onMac = true;
    await act(async () => { take!({ current: 'this', computers: devbox.computers }); });
    await act(async () => { await settle(); });
    expect(text()).not.toContain('Dashboard');
    expect(host.querySelectorAll('webview')).toHaveLength(0);
    letMacGo();
    await act(async () => { await settle(); });
    expect(text()).toContain('The Mac page');
    expect(text()).not.toContain(strip);
    expect(host.querySelector('.br-where')).toBeNull();
    expect(newTab()).toEqual(['New tab']);
    expect([...host.querySelectorAll('webview')].map((w) => w.getAttribute('partition'))).toEqual(['persist:oby-agent-claude']);
  });
});
