import { EventEmitter } from 'node:events';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { BrowserWindow } from 'electron';
import { namedPartitions, relayCommand, relayHost, type BoxCommand, type RelayHost } from '../../../main/browserRelay';

// A foreign job's box partition in the window (docs/contracts/browser-app.md, BROWSERAPP-36 to 38),
// as T3 Code makes a partition's session: made before its guest attaches, every request held for its
// job's egress door, the door's login answered only for that door, and cleared with its last tab
// (main/browserGuest.ts), on fake sessions and webviews.

type Handler = (details: unknown, callback: (r: { cancel?: boolean }) => void) => void;
type FakeSession = {
  partition: string;
  requests: Handler[];
  proxies: unknown[];
  cleared: unknown[];
  cache: number;
  auth: number;
  proxied: { resolve: () => void } | null;
  preloads: { type: string; id?: string; filePath: string }[];
  asked: (permission: string) => boolean;
  checked: (permission: string) => boolean;
};
const the = vi.hoisted(() => ({ sessions: new Map<string, unknown>(), guests: new Map<number, unknown>(), userData: '' }));
function fakeSession(partition: string) {
  const s = {
    partition, requests: [] as Handler[], proxies: [] as unknown[], cleared: [] as unknown[], cache: 0, auth: 0,
    proxied: null as { resolve: () => void } | null,
    preloads: [] as { type: string; id?: string; filePath: string }[],
    registerPreloadScript: (p: { type: string; id?: string; filePath: string }) => { s.preloads.push(p); return p.id ?? ''; },
    asked: (_p: string) => false, checked: (_p: string) => false,
    setPermissionRequestHandler(h: (wc: unknown, p: string, done: (ok: boolean) => void) => void) { s.asked = (p) => { let ok = false; h(null, p, (v) => { ok = v; }); return ok; }; },
    setPermissionCheckHandler(h: (wc: unknown, p: string) => boolean) { s.checked = (p) => h(null, p); },
    on() {},
    webRequest: { onBeforeRequest: (h: Handler) => { s.requests.push(h); } },
    setProxy: (config: unknown) => { s.proxies.push(config); return new Promise<void>((resolve) => { s.proxied = { resolve }; }); },
    clearStorageData: async (o: unknown) => { s.cleared.push(o); },
    clearCache: async () => { s.cache++; },
    clearAuthCache: async () => { s.auth++; },
  };
  return s;
}
vi.mock('electron', () => ({
  app: { getPath: () => the.userData },
  session: { fromPartition: (p: string) => { if (!the.sessions.has(p)) the.sessions.set(p, fakeSession(p)); return the.sessions.get(p); } },
  webContents: { fromId: (id: number) => the.guests.get(id) ?? null },
}));
the.userData = mkdtempSync(join(tmpdir(), 'oby-box-guard-'));
const { BOX_CLIPBOARD_GUARD, BOX_HOLD_MS, boxDoor, boxing, clearBox, hosting } = await import('../../../main/browserGuest');
const { session } = await import('electron');
const sessionOf = (p: string) => session.fromPartition(p) as unknown as FakeSession;
const made = (p: string) => the.sessions.has(p);

const token = 'a'.repeat(64);
const boxFor = (job: string, port = 50123): BoxCommand => relayCommand('box', JSON.stringify({ threadId: job, partition: `oby-box-${job}`, proxy: `http://oby:${token}@127.0.0.1:${port}` })) as BoxCommand;
const settle = () => new Promise((r) => setTimeout(r, 0));

// a request on the session: what it was answered, or 'held' while it waits
function request(s: FakeSession) {
  const answer: { got: { cancel?: boolean } | 'held' } = { got: 'held' };
  for (const h of s.requests) h({ url: 'https://example.com/' }, (r) => { answer.got = r; });
  return answer;
}
function aWindow(named: string | undefined = undefined) {
  const win = { webContents: new EventEmitter() };
  const host = { attach: vi.fn(), detach: vi.fn(), forget: vi.fn(), event: vi.fn(), shown: vi.fn(), loadFailed: vi.fn() };
  const h = hosting(win as unknown as BrowserWindow, host as unknown as RelayHost, () => named, () => {}, { computer: () => null, send: vi.fn() });
  const attaching = (partition: string, src = 'https://example.com/', prefs: Record<string, unknown> = {}) => {
    const e = { preventDefault: vi.fn() };
    win.webContents.emit('will-attach-webview', e, prefs, { partition, src });
    return e.preventDefault.mock.calls.length === 0;
  };
  const guest = (partition: string) => {
    const wc = Object.assign(new EventEmitter(), {
      id: Math.floor(Math.random() * 1e9), session: session.fromPartition(partition),
      setWindowOpenHandler() {}, setWebRTCIPHandlingPolicy: vi.fn(),
      getType: () => 'webview', hostWebContents: win.webContents, isDestroyed: () => false, isDevToolsOpened: () => false,
      isLoading: () => false, isLoadingMainFrame: () => false, getURL: () => 'https://example.com/',
      navigationHistory: { canGoBack: () => false, canGoForward: () => false },
      debugger: Object.assign(new EventEmitter(), {
        attached: false,
        isAttached: () => wc.debugger.attached,
        attach: () => { wc.debugger.attached = true; },
        // what main sent the guest's debugger, in order: [method, params, sessionId]
        sent: [] as unknown[][],
        sendCommand: vi.fn(async (method: string, params: unknown, sessionId?: string) => { wc.debugger.sent.push([method, params, sessionId]); return {}; }),
      }),
    });
    the.guests.set(wc.id, wc);
    win.webContents.emit('did-attach-webview', {}, wc);
    return wc;
  };
  return { attaching, guest, h, host };
}
type Login = { prevented: boolean; answered: unknown[] | null };
function login(wc: EventEmitter, authInfo: { isProxy: boolean; host: string; port: number }): Login {
  const l: Login = { prevented: false, answered: null };
  wc.emit('login', { preventDefault: () => { l.prevented = true; } }, {}, { scheme: 'basic', realm: 'oby', ...authInfo }, (...args: unknown[]) => { l.answered = args; });
  return l;
}
const tab = (job: string, id: string, partition = `oby-box-${job}`) => JSON.stringify({ seq: 1, type: 'tab', tab: { id, owner: { kind: 'agent', job, agent: 'codex' }, url: 'https://example.com/', partition } });

afterEach(() => { vi.useRealTimers(); });

describe('BROWSERAPP-36: a box partition\'s session is made before its guest attaches, its requests held for its job\'s door', () => {
  it('browserapp_36: will-attach-webview prepares a box once harden passed, never a page it refused', () => {
    const w = aWindow();
    expect(w.attaching('oby-box-b36a')).toBe(true);
    expect(sessionOf('oby-box-b36a').requests).toHaveLength(1);
    expect(w.attaching('oby-box-b36b', 'file:///etc/passwd')).toBe(false);
    expect(made('oby-box-b36b')).toBe(false);
  });

  it('browserapp_36: the first kept tab event naming a box prepares its session; a refused one doesn\'t', () => {
    const p = namedPartitions(() => null);
    const boxed = boxing((k) => p.drop(k));
    expect(p.heard('tab', tab('b36c', 't1'), boxed)).toBe('kept');
    expect(sessionOf('oby-box-b36c').requests).toHaveLength(1);
    expect(p.heard('tab', tab('b36c', 't1'), boxed)).toBe('kept');
    expect(sessionOf('oby-box-b36c').requests).toHaveLength(1);
    expect(p.heard('tab', tab('b36c', 't1', 'oby-box-b36d'), boxed)).toBe('refused');
    expect(made('oby-box-b36d')).toBe(false);
  });

  it('browserapp_36: a request before box waits, and goes once box\'s setProxy resolved', async () => {
    aWindow().attaching('oby-box-b36e');
    const s = sessionOf('oby-box-b36e');
    const early = request(s);
    const set = boxDoor(boxFor('b36e'), null);
    await settle();
    expect(early.got).toBe('held');
    s.proxied!.resolve();
    await set;
    expect(early.got).toEqual({});
    expect(request(s).got).toEqual({});
  });

  it('browserapp_36: a request with no door is cancelled after 10 s', () => {
    vi.useFakeTimers();
    aWindow().attaching('oby-box-b36f');
    const r = request(sessionOf('oby-box-b36f'));
    vi.advanceTimersByTime(BOX_HOLD_MS - 1);
    expect(r.got).toBe('held');
    vi.advanceTimersByTime(1);
    expect(r.got).toEqual({ cancel: true });
  });

  it('browserapp_36: a box page gets only fullscreen and pointer lock, never the clipboard; an agent\'s page keeps the clipboard\'s sanitized write', () => {
    const w = aWindow();
    w.attaching('oby-box-b36h');
    w.attaching('persist:oby-agent-cursor');
    const box = sessionOf('oby-box-b36h'), agent = sessionOf('persist:oby-agent-cursor');
    for (const p of ['fullscreen', 'pointerLock']) expect([box.asked(p), box.checked(p)]).toEqual([true, true]);
    for (const p of ['clipboard-sanitized-write', 'clipboard-read', 'media']) expect([box.asked(p), box.checked(p)], p).toEqual([false, false]);
    expect([agent.asked('clipboard-sanitized-write'), agent.checked('clipboard-sanitized-write')]).toEqual([true, true]);
  });

  it('browserapp_36: a box session\'s preload is main\'s clipboard guard, in every frame, before its guest\'s first page; an agent\'s session has none', () => {
    const box: Record<string, unknown> = {}, agent: Record<string, unknown> = {};
    const w = aWindow();
    expect(w.attaching('oby-box-b36j', 'https://example.com/', box)).toBe(true);
    expect(w.attaching('persist:oby-agent-codex', 'https://example.com/', agent)).toBe(true);
    const preloads = sessionOf('oby-box-b36j').preloads;
    expect(preloads.map((p) => [p.type, p.id])).toEqual([['frame', 'oby-box-clipboard-guard']]);
    expect(readFileSync(preloads[0].filePath, 'utf8')).toBe(BOX_CLIPBOARD_GUARD);
    expect(preloads[0].filePath.startsWith(the.userData)).toBe(true);
    // every frame runs it (sandboxed: no Node), where an agent page's frames run none
    expect([box.nodeIntegrationInSubFrames, box.sandbox, box.nodeIntegration]).toEqual([true, true, false]);
    expect(agent.nodeIntegrationInSubFrames).toBe(false);
    expect(sessionOf('persist:oby-agent-codex').preloads).toEqual([]);
    // the first tab event naming a box sets it up the same way, once
    const p = namedPartitions(() => null);
    p.heard('tab', tab('b36k', 't1'), boxing((k) => p.drop(k)));
    w.attaching('oby-box-b36k');
    expect(sessionOf('oby-box-b36k').preloads).toHaveLength(1);
  });

  it('browserapp_36: an agent partition\'s session gets no request handler and no proxy', () => {
    const w = aWindow();
    expect(w.attaching('persist:oby-agent-claude')).toBe(true);
    const p = namedPartitions(() => null);
    expect(p.heard('tab', tab('b36g', 't1', 'persist:oby-agent-claude'), boxing((k) => p.drop(k)))).toBe('kept');
    w.guest('persist:oby-agent-claude');
    const s = sessionOf('persist:oby-agent-claude');
    expect(s.requests).toEqual([]);
    expect(s.proxies).toEqual([]);
  });
});

describe('BROWSERAPP-37: a box\'s session goes only through its job\'s door', () => {
  it('browserapp_37: box sets exactly the door as the session\'s proxy, loopback included', async () => {
    const set = boxDoor(boxFor('b37a', 50137), null);
    const s = sessionOf('oby-box-b37a');
    expect(s.proxies).toEqual([{ mode: 'fixed_servers', proxyRules: '127.0.0.1:50137', proxyBypassRules: '<-loopback>' }]);
    s.proxied!.resolve();
    await set;
  });

  it('browserapp_37: the door\'s login is answered only for that door; a server\'s, another host\'s or another box\'s port gets none', async () => {
    const w = aWindow();
    w.attaching('oby-box-b37b');
    const wc = w.guest('oby-box-b37b');
    const set = boxDoor(boxFor('b37b', 50138), null);
    sessionOf('oby-box-b37b').proxied!.resolve();
    await set;
    const theirs = boxDoor(boxFor('b37c', 50139), null);
    sessionOf('oby-box-b37c').proxied!.resolve();
    await theirs;
    const door = login(wc, { isProxy: true, host: '127.0.0.1', port: 50138 });
    expect(door).toEqual({ prevented: true, answered: ['oby', token] });
    for (const other of [
      { isProxy: false, host: 'example.com', port: 443 },
      { isProxy: true, host: '10.0.0.1', port: 50138 },
      { isProxy: true, host: '127.0.0.1', port: 50139 },
    ]) expect(login(wc, other), JSON.stringify(other)).toEqual({ prevented: true, answered: [] });
  });

  it('browserapp_37: a box page is given no client certificate of the owner\'s; an agent page is left to Electron', () => {
    const w = aWindow();
    w.attaching('oby-box-b37e');
    const pick = (wc: EventEmitter) => {
      const got = { prevented: false, answered: null as unknown[] | null };
      wc.emit('select-client-certificate', { preventDefault: () => { got.prevented = true; } }, 'https://example.com/', [{ subjectName: 'the owner' }], (...args: unknown[]) => { got.answered = args; });
      return got;
    };
    expect(pick(w.guest('oby-box-b37e'))).toEqual({ prevented: true, answered: [] });
    expect(pick(w.guest('persist:oby-agent-claude'))).toEqual({ prevented: false, answered: null });
  });

  it('browserapp_37: a box cleared while its door is still being set doesn\'t get that door back', async () => {
    const w = aWindow();
    w.attaching('oby-box-b37f');
    const wc = w.guest('oby-box-b37f');
    const s = sessionOf('oby-box-b37f');
    const early = request(s);
    const set = boxDoor(boxFor('b37f', 50140), null);
    await clearBox('oby-box-b37f');
    s.proxied!.resolve();
    await set;
    expect(early.got).toBe('held');
    expect(request(s).got).toBe('held');
    expect(login(wc, { isProxy: true, host: '127.0.0.1', port: 50140 }).answered).toEqual([]);
  });

  it('browserapp_37: every frame of a box tab gets main\'s clipboard guard, and obid drives the tab only once it\'s on; a frame\'s own events never reach obid', async () => {
    const w = aWindow('oby-box-b37h');
    w.attaching('oby-box-b37h');
    const wc = w.guest('oby-box-b37h');
    const k = { threadId: 'b37h', tabId: 't1' };
    expect(w.h.register(k, wc.id)).toBeNull();
    expect(w.host.attach).not.toHaveBeenCalled();
    await settle();
    expect(wc.debugger.sent).toEqual([
      ['Page.enable', {}, undefined],
      ['Page.addScriptToEvaluateOnNewDocument', { source: BOX_CLIPBOARD_GUARD, worldName: 'oby-box-guard', runImmediately: true }, undefined],
      ['Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }, undefined],
    ]);
    expect(w.host.attach).toHaveBeenCalledTimes(1);
    // a cross-site frame the page starts: guarded, then let run
    wc.debugger.sent.length = 0;
    wc.debugger.emit('message', {}, 'Target.attachedToTarget', { sessionId: 's1', targetInfo: { type: 'iframe' }, waitingForDebugger: true });
    await settle();
    expect(wc.debugger.sent.map(([m, , s]) => [m, s])).toEqual([
      ['Page.enable', 's1'], ['Page.addScriptToEvaluateOnNewDocument', 's1'], ['Target.setAutoAttach', 's1'], ['Runtime.runIfWaitingForDebugger', 's1'],
    ]);
    wc.debugger.emit('message', {}, 'Page.loadEventFired', { timestamp: 1 }, 's1');
    wc.debugger.emit('message', {}, 'Page.loadEventFired', { timestamp: 2 });
    expect(w.host.event.mock.calls).toEqual([[k, 'Page.loadEventFired', { timestamp: 2 }]]);
  });

  it('browserapp_37: an agent tab gets no clipboard guard', async () => {
    const w = aWindow('persist:oby-agent-claude');
    w.attaching('persist:oby-agent-claude');
    const wc = w.guest('persist:oby-agent-claude');
    expect(w.h.register({ threadId: 'b37i', tabId: 't1' }, wc.id)).toBeNull();
    await settle();
    expect(wc.debugger.sent).toEqual([]);
    expect(w.host.attach).toHaveBeenCalledTimes(1);
  });

  it('browserapp_37: a box guest keeps WebRTC to the proxy; an agent guest is left as it was', () => {
    const w = aWindow();
    w.attaching('oby-box-b37d');
    const box = w.guest('oby-box-b37d');
    expect(box.setWebRTCIPHandlingPolicy).toHaveBeenCalledWith('disable_non_proxied_udp');
    const agent = w.guest('persist:oby-agent-codex');
    expect(agent.setWebRTCIPHandlingPolicy).not.toHaveBeenCalled();
    expect(login(agent, { isProxy: true, host: '127.0.0.1', port: 50138 }).prevented).toBe(false);
  });
});

describe('BROWSERAPP-38: a box is cleared with its last tab', () => {
  const STORAGES = { storages: ['cookies', 'localstorage', 'indexdb', 'serviceworkers', 'cachestorage'] };
  async function opened(job: string, tabs: string[]) {
    const p = namedPartitions(() => null);
    const boxed = boxing((k) => p.drop(k));
    for (const id of tabs) p.heard('tab', tab(job, id), boxed);
    const host = relayHost(async () => true, { home: '/nowhere', released: boxed.released });
    const set = boxDoor(boxFor(job), null);
    const s = sessionOf(`oby-box-${job}`);
    s.proxied!.resolve();
    await set;
    return { p, boxed, host, s };
  }
  const cleared = (s: FakeSession) => ({ storage: s.cleared, cache: s.cache, auth: s.auth });

  it('browserapp_38: release of a box\'s last tab clears its storage, cache and logins, and its next request waits again', async () => {
    const { host, s } = await opened('b38a', ['t1']);
    expect(request(s).got).toEqual({});
    host.command({ type: 'release', threadId: 'b38a', tabId: 't1' });
    await settle();
    expect(cleared(s)).toEqual({ storage: [STORAGES], cache: 1, auth: 1 });
    expect(request(s).got).toBe('held');
  });

  it('browserapp_38: tabClosed of a box\'s last tab clears it the same way; another tab still named keeps it', async () => {
    const { p, boxed, s } = await opened('b38b', ['t1', 't2']);
    p.heard('tabClosed', JSON.stringify({ seq: 2, type: 'tabClosed', tabId: 't1' }), boxed);
    await settle();
    expect(cleared(s)).toEqual({ storage: [], cache: 0, auth: 0 });
    expect(request(s).got).toEqual({});
    p.heard('tabClosed', JSON.stringify({ seq: 3, type: 'tabClosed', tabId: 't2' }), boxed);
    await settle();
    expect(cleared(s)).toEqual({ storage: [STORAGES], cache: 1, auth: 1 });
  });

  it('browserapp_38: a second drop of the same tab clears nothing', async () => {
    const { p, boxed, host, s } = await opened('b38c', ['t1']);
    host.command({ type: 'release', threadId: 'b38c', tabId: 't1' });
    await settle();
    host.command({ type: 'release', threadId: 'b38c', tabId: 't1' });
    p.heard('tabClosed', JSON.stringify({ seq: 2, type: 'tabClosed', tabId: 't1' }), boxed);
    await settle();
    expect(cleared(s)).toEqual({ storage: [STORAGES], cache: 1, auth: 1 });
  });
});
