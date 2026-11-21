import { describe, expect, it, vi } from 'vitest';
import { DOMAINS, KEPT, MAX_MESSAGE, insideDownloads, relayCommand, relayHost, type Guest, type RelayEvent } from '../../../main/browserRelay';
import { sseReader } from '../../../shared/obid-wire';

vi.mock('electron', () => ({ session: { fromPartition: () => ({}) }, webContents: { fromId: () => null } }));
const { harden, partitionOk, pageOk } = await import('../../../main/browserGuest');

// The relay between obid's agents and the window's <webview> (docs/plans/browser-t3.md), with a
// fake debugger for the webview and a fake obid for POST /v1/browser/relay.

const HOME = '/Users/someone/Library/Application Support/Oby/Runner';
const K = { threadId: 'j1', tabId: 't1' };
const settle = () => new Promise((r) => setTimeout(r, 0));

type FakeGuest = Guest & {
  sent: { method: string; params?: Record<string, unknown> }[];
  reloads: number; stops: number; typed: string[]; keys: Record<string, unknown>[]; folder: string | null | undefined; focus: boolean;
};
function fakeGuest(over: Partial<Guest> = {}): FakeGuest {
  const sent: { method: string; params?: Record<string, unknown> }[] = [];
  const g: FakeGuest = {
    sent, reloads: 0, stops: 0, typed: [] as string[], keys: [] as Record<string, unknown>[], folder: undefined as string | null | undefined, focus: true,
    send: vi.fn(async (method: string, params?: Record<string, unknown>) => { sent.push({ method, params }); return { from: method }; }),
    reload() { g.reloads++; }, stop() { g.stops++; },
    focused: () => g.focus,
    insertText: async (t: string) => { g.typed.push(t); },
    key: (p: Record<string, unknown>) => { g.keys.push(p); },
    downloadsTo: (f: string | null) => { g.folder = f; },
    ...over,
  };
  return g;
}
function fakeObid(takes: (n: number) => boolean = () => true) {
  const posts: RelayEvent[][] = [];
  let inFlight = 0, most = 0;
  const post = async (events: RelayEvent[]) => {
    inFlight++; most = Math.max(most, inFlight);
    await settle();
    posts.push(events);
    inFlight--;
    return takes(posts.length);
  };
  return { post, posts, most: () => most, all: () => posts.flat() };
}
const replies = (events: RelayEvent[]) => events.filter((e): e is Extract<RelayEvent, { type: 'cdp' }> => e.type === 'cdp').map((e) => JSON.parse(e.message));
const cdp = (id: number, method: string, params?: unknown, extra: object = {}) => ({ ...K, type: 'cdp' as const, message: JSON.stringify({ id, method, params, ...extra }) });

describe('BROWSERAPP-22: the relay carries the window\'s webviews to obid in order, as T3 Code\'s does', () => {
  it('browserapp_22: attached, the page\'s events and the replies go in order, one request in flight; what obid didn\'t take goes again', async () => {
    const o = fakeObid((n) => n !== 1);
    const host = relayHost(o.post, { home: HOME, wait: async () => undefined });
    const g = fakeGuest();
    host.attach(K, g);
    host.event(K, 'Page.loadEventFired', { timestamp: 1 });
    host.command(cdp(1, 'Runtime.evaluate', { expression: '1' }));
    for (let i = 0; i < 10; i++) await settle();
    expect(o.most()).toBe(1);
    // the first post wasn't taken: sent again, before anything after it
    expect(o.posts[0]).toEqual(o.posts[1].slice(0, o.posts[0].length));
    const taken = o.posts.slice(1).flat();
    expect(taken[0]).toEqual({ ...K, type: 'attached' });
    expect(JSON.parse((taken[1] as { message: string }).message)).toEqual({ method: 'Page.loadEventFired', params: { timestamp: 1 } });
    expect(replies(taken).at(-1)).toEqual({ id: 1, result: { from: 'Runtime.evaluate' } });
    expect(g.sent).toEqual([{ method: 'Runtime.evaluate', params: { expression: '1' } }]);
  });

  it('browserapp_22: a first load\'s failure and whether the tab shows are kept until it attaches; shown outlives a detach; a reconnect announces every live tab again', async () => {
    const o = fakeObid();
    const host = relayHost(o.post, { home: HOME });
    host.loadFailed(K, 'http://localhost:3000/', -102, 'ERR_CONNECTION_REFUSED');
    host.shown(K, true);
    host.attach(K, fakeGuest());
    await settle(); await settle();
    expect(o.all().map((e) => e.type)).toEqual(['attached', 'shown', 'loadFailed']);
    host.detach(K);
    host.attach(K, fakeGuest());
    host.reconnected();
    for (let i = 0; i < 6; i++) await settle();
    expect(o.all().map((e) => e.type)).toEqual(['attached', 'shown', 'loadFailed', 'detached', 'attached', 'shown', 'attached', 'shown']);
  });

  it('browserapp_22: obid\'s commands come on their own stream, each run once; when it reconnects every live tab is announced again and no earlier command runs again', async () => {
    const o = fakeObid();
    const host = relayHost(o.post, { home: HOME });
    const g = fakeGuest();
    host.attach(K, g);
    // GET /v1/browser/relay as main reads it: `event:` the type, `data:` the command's own JSON, no ids
    const read = sseReader((name, data) => { const c = relayCommand(name, data); if (c) host.command(c); });
    read(`event: cdp\ndata: ${JSON.stringify({ type: 'cdp', ...K, message: JSON.stringify({ id: 1, method: 'Input.dispatchMouseEvent', params: { type: 'mousePressed', x: 5, y: 5 } }) })}\n\n`);
    for (let i = 0; i < 6; i++) await settle();
    expect(g.sent).toHaveLength(1);
    // the stream drops and comes back: the tab is announced again, the click is never run again
    host.reconnected();
    read(': keep-alive\n\n');
    for (let i = 0; i < 6; i++) await settle();
    expect(g.sent).toHaveLength(1);
    expect(o.all().map((e) => e.type)).toEqual(['attached', 'cdp', 'attached']);
    expect(replies(o.all())).toEqual([{ id: 1, result: { from: 'Input.dispatchMouseEvent' } }]);
    // a command whose own type isn't its event's is no command
    expect(relayCommand('cdp', JSON.stringify({ type: 'release', ...K, message: '{}' }))).toBeNull();
  });

  it('browserapp_22: obid\'s commands are read only in their shapes; pointer draws, never inputs; a turn keeps its tabs\' focus emulated', async () => {
    expect(relayCommand('cdp', JSON.stringify({ threadId: 'j1', tabId: 't1', message: '{}' }))).toEqual({ ...K, type: 'cdp', message: '{}' });
    expect(relayCommand('pointer', JSON.stringify({ threadId: 'j1', tabId: 't1', phase: 'click', x: 3, y: 4 }))).toEqual({ ...K, type: 'pointer', phase: 'click', x: 3, y: 4 });
    expect(relayCommand('turn', JSON.stringify({ threadId: 'j1', active: true }))).toEqual({ type: 'turn', threadId: 'j1', active: true });
    expect(relayCommand('release', JSON.stringify(K))).toEqual({ ...K, type: 'release' });
    for (const [n, d] of [['tab', '{}'], ['cdp', '{"threadId":"j1","tabId":"t1"}'], ['pointer', '{"threadId":"j1","tabId":"t1","phase":"drag","x":1,"y":1}'], ['turn', '{"threadId":"j1"}'], ['cdp', 'not json']]) expect(relayCommand(n, d)).toBeNull();
    const drawn: unknown[] = [];
    const host = relayHost(fakeObid().post, { home: HOME, pointer: (p) => drawn.push(p) });
    const g = fakeGuest();
    host.attach(K, g);
    host.command({ ...K, type: 'pointer', phase: 'move', x: 10, y: 20 });
    host.command({ type: 'turn', threadId: 'j1', active: true });
    await settle();
    expect(drawn).toEqual([{ ...K, type: 'pointer', phase: 'move', x: 10, y: 20 }]);
    expect(g.sent).toEqual([{ method: 'Emulation.setFocusEmulationEnabled', params: { enabled: true } }]);
  });
});

describe('BROWSERAPP-23: main runs page-level CDP only, on the tab\'s own webview', () => {
  it('browserapp_23: a domain outside the page\'s, or a session, is refused and never reaches the debugger', async () => {
    const o = fakeObid();
    const host = relayHost(o.post, { home: HOME });
    const g = fakeGuest();
    host.attach(K, g);
    for (const [id, method] of [[1, 'Browser.close'], [2, 'Target.createTarget'], [3, 'Storage.clearDataForOrigin'], [4, 'SystemInfo.getInfo']] as const) host.command(cdp(id, method));
    host.command(cdp(5, 'Runtime.evaluate', {}, { sessionId: 'other' }));
    for (let i = 0; i < 6; i++) await settle();
    expect(g.sent).toEqual([]);
    expect(replies(o.all()).map((r) => [r.id, r.error?.code])).toEqual([[1, -32601], [2, -32601], [3, -32601], [4, -32601], [5, -32601]]);
    expect([...DOMAINS].sort()).toEqual(['Accessibility', 'DOM', 'DOMSnapshot', 'Emulation', 'Input', 'Log', 'Network', 'Page', 'Runtime']);
  });

  it('browserapp_23: reload and stop are the guest\'s own; keys and text into a guest with no focused frame go in as its input; presses run one at a time', async () => {
    const o = fakeObid();
    const host = relayHost(o.post, { home: HOME });
    let release: () => void = () => {};
    const order: string[] = [];
    const g = fakeGuest();
    g.send = vi.fn(async (method: string, params?: Record<string, unknown>) => {
      order.push(`start ${String(params?.type)}`);
      if (params?.type === 'mousePressed') await new Promise<void>((done) => { release = done; });
      order.push(`end ${String(params?.type)}`);
      return {};
    });
    host.attach(K, g);
    host.command(cdp(1, 'Page.reload'));
    host.command(cdp(2, 'Page.stopLoading'));
    g.focus = false;
    host.command(cdp(3, 'Input.insertText', { text: 'hello' }));
    host.command(cdp(4, 'Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter' }));
    host.command(cdp(5, 'Input.dispatchMouseEvent', { type: 'mousePressed', x: 1, y: 1 }));
    host.command(cdp(6, 'Input.dispatchMouseEvent', { type: 'mouseReleased', x: 1, y: 1 }));
    await settle();
    expect(order).toEqual(['start mousePressed']);
    release();
    for (let i = 0; i < 6; i++) await settle();
    expect(order).toEqual(['start mousePressed', 'end mousePressed', 'start mouseReleased', 'end mouseReleased']);
    expect([g.reloads, g.stops, g.typed, g.keys]).toEqual([1, 1, ['hello'], [{ type: 'keyDown', key: 'Enter' }]]);
    expect(replies(o.all()).map((r) => r.id).sort()).toEqual([1, 2, 3, 4, 5, 6]);
  });

  it('browserapp_23: a command for a tab not attached is answered with an error; a message over 64 MB ends the tab\'s relay', async () => {
    const o = fakeObid();
    const host = relayHost(o.post, { home: HOME });
    host.command(cdp(7, 'Runtime.evaluate'));
    await settle(); await settle();
    expect(replies(o.all())).toEqual([{ id: 7, error: { code: -32000, message: 'This tab isn’t attached in the window.' } }]);
    const g = fakeGuest({ send: vi.fn(async () => ({ data: 'x'.repeat(MAX_MESSAGE) })) });
    host.attach(K, g);
    host.command(cdp(8, 'Page.captureScreenshot'));
    for (let i = 0; i < 6; i++) await settle();
    expect(o.all().map((e) => e.type).slice(-2)).toEqual(['attached', 'detached']);
    expect(KEPT).toBeGreaterThan(1000);
  });
});

describe('BROWSERAPP-24: a webview lives only in the partition obid named, hardened', () => {
  it('browserapp_24: only the two partition shapes; never the default session or the window\'s own', () => {
    for (const p of ['persist:oby-agent-claude', 'persist:oby-agent-codex', 'persist:oby-agent-cursor', 'oby-box-job-12_ab']) expect(partitionOk(p)).toBe(true);
    for (const p of [undefined, '', 'persist:oby-agent-gemini', 'persist:main', 'oby-box-', 'oby-box-a/b', 'persist:oby-box-j1', 'oby-agent-codex', `oby-box-${'a'.repeat(129)}`]) expect(partitionOk(p)).toBe(false);
  });

  it('browserapp_24: its preferences are sandboxed with no Node and no preload, whatever the page asked; a page off the web isn\'t attached', () => {
    const prefs: Record<string, unknown> = { preload: 'file:///evil.js', preloadURL: 'file:///evil.js', nodeIntegration: true, nodeIntegrationInSubFrames: true, sandbox: false, contextIsolation: false, webSecurity: false };
    expect(harden(prefs, { partition: 'persist:oby-agent-codex', src: 'https://example.test/' })).toBe(true);
    expect(prefs).toEqual({ sandbox: true, nodeIntegration: false, nodeIntegrationInSubFrames: false, contextIsolation: true, webSecurity: true, allowRunningInsecureContent: false, disableHtmlFullscreenWindowResize: true });
    expect(harden({}, { partition: 'persist:main', src: 'https://example.test/' })).toBe(false);
    for (const src of ['file:///etc/passwd', 'javascript:alert(1)', 'data:text/html,hi', 'oby://x']) expect(harden({}, { partition: 'oby-box-j1', src })).toBe(false);
    expect([pageOk('about:blank'), pageOk('http://localhost:3000/'), pageOk('about:srcdoc')]).toEqual([true, true, false]);
  });
});

describe('BROWSERAPP-25: an agent\'s downloads go only where obid says, inside its own folder', () => {
  it('browserapp_25: Page.setDownloadBehavior is kept only for a folder inside obid\'s browser/downloads, else refused; it never reaches the debugger', async () => {
    expect(insideDownloads(HOME, `${HOME}/browser/downloads/j1`)).toBe(true);
    for (const p of [`${HOME}/browser/downloads`, `${HOME}/browser/downloads/../vault`, '/Users/someone/Downloads', 'browser/downloads/j1', 42]) expect(insideDownloads(HOME, p)).toBe(false);
    const o = fakeObid();
    const host = relayHost(o.post, { home: HOME });
    const g = fakeGuest();
    host.attach(K, g);
    host.command(cdp(1, 'Page.setDownloadBehavior', { behavior: 'allow', downloadPath: `${HOME}/browser/downloads/j1` }));
    await settle();
    expect(g.folder).toBe(`${HOME}/browser/downloads/j1`);
    host.command(cdp(2, 'Page.setDownloadBehavior', { behavior: 'allow', downloadPath: '/Users/someone/Desktop' }));
    host.command(cdp(3, 'Page.setDownloadBehavior', { behavior: 'deny' }));
    for (let i = 0; i < 6; i++) await settle();
    expect(g.folder).toBeNull();
    expect(g.sent).toEqual([]);
    expect(replies(o.all()).map((r) => [r.id, r.error ? 'refused' : 'kept'])).toEqual([[1, 'kept'], [2, 'refused'], [3, 'kept']]);
  });
});
