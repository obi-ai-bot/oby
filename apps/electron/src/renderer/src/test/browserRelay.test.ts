import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { KEPT, METHODS, MAX_MESSAGE, insideDownloads, namedPartitions, relayCommand, relayHost, type Guest, type RelayEvent } from '../../../main/browserRelay';
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
  });

  it('browserapp_23: reload is the guest\'s own and stopLoading is refused; keys and text into a guest with no focused frame go in as its input; presses run one at a time', async () => {
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
    expect([g.reloads, g.stops, g.typed, g.keys]).toEqual([1, 0, ['hello'], [{ type: 'keyDown', key: 'Enter' }]]);
    expect(replies(o.all()).map((r) => r.id).sort()).toEqual([1, 2, 3, 4, 5, 6]);
    // obid never sends Page.stopLoading: it's refused (BROWSERAPP-27)
    expect(replies(o.all()).find((r) => r.id === 2)?.error?.code).toBe(-32601);
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

describe('BROWSERAPP-27: main runs only the CDP methods obid sends', () => {
  for (const method of ['Network.getAllCookies', 'DOMSnapshot.captureSnapshot', 'Page.addScriptToEvaluateOnNewDocument', 'Runtime.addBinding']) {
    it(`browserapp_27: ${method} is refused and never reaches the guest`, async () => {
      const o = fakeObid();
      const host = relayHost(o.post, { home: HOME });
      const g = fakeGuest();
      host.attach(K, g);
      host.command(cdp(1, method, {}));
      for (let i = 0; i < 6; i++) await settle();
      expect(replies(o.all()).map((r) => [r.id, r.error?.code])).toEqual([[1, -32601]]);
      expect([g.sent, g.reloads, g.stops, g.typed, g.keys, g.folder]).toEqual([[], 0, 0, [], [], undefined]);
    });
  }
});

describe('BROWSERAPP-28: Page.navigate goes only to a page of the web', () => {
  for (const [name, params] of [
    ['file:///etc/hosts', { url: 'file:///etc/hosts' }],
    ['chrome://settings', { url: 'chrome://settings' }],
    ['devtools://devtools/bundled/inspector.html', { url: 'devtools://devtools/bundled/inspector.html' }],
    ['the app\'s own page', { url: 'file:///Applications/Oby.app/Contents/Resources/app.asar/out/renderer/index.html' }],
    ['no url', {}],
  ] as const) {
    it(`browserapp_28: Page.navigate to ${name} is refused and never reaches the guest`, async () => {
      const o = fakeObid();
      const host = relayHost(o.post, { home: HOME });
      const g = fakeGuest();
      host.attach(K, g);
      host.command(cdp(1, 'Page.navigate', params));
      for (let i = 0; i < 6; i++) await settle();
      expect(replies(o.all()).map((r) => [r.id, r.error?.code])).toEqual([[1, -32601]]);
      expect(g.sent).toEqual([]);
    });
  }

  it('browserapp_28: in a development run, Page.navigate to the app\'s own dev server is refused, on any loopback name for it; another local server isn\'t', async () => {
    vi.stubEnv('ELECTRON_RENDERER_URL', 'http://localhost:5175/');
    try {
      const o = fakeObid();
      const host = relayHost(o.post, { home: HOME });
      const g = fakeGuest();
      host.attach(K, g);
      // every loopback host on the dev server's port: Chromium sends any *.localhost name to loopback too
      const refused = [
        'http://localhost:5175/', 'http://localhost:5175/#/settings', 'http://LOCALHOST:5175/index.html', 'https://localhost:5175/',
        'http://app.localhost:5175/', 'http://a.b.localhost:5175/', 'http://localhost.:5175/', 'http://APP.LocalHost.:5175/',
        'http://127.0.0.1:5175/', 'http://127.1:5175/', 'http://0x7f000001:5175/', 'http://127.255.255.254:5175/', 'http://0.0.0.0:5175/', 'http://0:5175/',
        'http://[::1]:5175/', 'http://[0:0:0:0:0:0:0:1]:5175/', 'http://[::]:5175/', 'http://[::ffff:127.0.0.1]:5175/', 'http://[::ffff:7f00:1]:5175/', 'http://[::ffff:0.0.0.0]:5175/',
      ];
      // another port on loopback, or the same port on another host, is another server
      const kept = ['http://localhost:3000/', 'http://127.0.0.1:5176/', 'http://localhost.example.com:5175/', 'http://notlocalhost:5175/', 'http://128.0.0.1:5175/', 'http://[::2]:5175/'];
      [...refused, ...kept].forEach((url, i) => host.command(cdp(i + 1, 'Page.navigate', { url })));
      for (let i = 0; i < 30; i++) await settle();
      const answered = new Map(replies(o.all()).map((r) => [r.id, r.error?.code]));
      expect([...refused, ...kept].map((url, i) => [url, answered.get(i + 1)])).toEqual([...refused.map((u) => [u, -32601]), ...kept.map((u) => [u, undefined])]);
      expect(g.sent.map((s) => s.params?.url)).toEqual(kept);
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('browserapp_28: Page.navigate to https://example.com/ reaches the guest and is answered', async () => {
    const o = fakeObid();
    const host = relayHost(o.post, { home: HOME });
    const g = fakeGuest();
    host.attach(K, g);
    host.command(cdp(1, 'Page.navigate', { url: 'https://example.com/' }));
    for (let i = 0; i < 6; i++) await settle();
    expect(g.sent).toEqual([{ method: 'Page.navigate', params: { url: 'https://example.com/' } }]);
    expect(replies(o.all())).toEqual([{ id: 1, result: { from: 'Page.navigate' } }]);
  });
});

// What obid's browser sends over CDP, read from its Rust sources: every "Domain.method" literal outside
// its test items; one right after `==` is an event it listens for, any other a method it sends. A CDP
// call (`send`/`cdp` on a tab) whose method isn't a literal can't be read, and is listed as such.
// ponytail: a scanner of Rust's strings, chars and comments only, not a parser; enough for obid's style
// where the token at `i` of `text` ends, when it's a string, raw string, char or comment (else `i`)
function skip(text: string, i: number): number {
  const raw = /^b?r(#*)"/.exec(text.slice(i, i + 260));
  if (raw && (i === 0 || !/\w/.test(text[i - 1]))) { const end = text.indexOf(`"${raw[1]}`, i + raw[0].length); return end < 0 ? text.length : end + 1 + raw[1].length; }
  if (text[i] === '"') { let j = i + 1; while (j < text.length && text[j] !== '"') j += text[j] === '\\' ? 2 : 1; return j + 1; }
  const ch = /^'(\\.[^']*|[^\\'])'/.exec(text.slice(i, i + 12));
  if (ch) return i + ch[0].length;
  if (text.startsWith('//', i)) { const end = text.indexOf('\n', i); return end < 0 ? text.length : end; }
  if (text.startsWith('/*', i)) { const end = text.indexOf('*/', i + 2); return end < 0 ? text.length : end + 2; }
  return i;
}
// from `i`, where the item there ends: after its `;`, or its `{` and the `}` that closes it
function itemEnd(text: string, i: number): number {
  let depth = 0;
  while (i < text.length) {
    const j = skip(text, i);
    if (j !== i) { i = j; continue; }
    const c = text[i++];
    if (c === '{') depth++;
    else if (c === '}' && --depth === 0) break;
    else if (c === ';' && depth === 0) break;
  }
  return i;
}
// the top-level arguments of the call whose `(` is at `open`, trimmed
function args(text: string, open: number): string[] {
  const out: string[] = [];
  let depth = 0, from = open + 1, i = open;
  while (i < text.length) {
    const j = skip(text, i);
    if (j !== i) { i = j; continue; }
    const c = text[i++];
    if ('([{'.includes(c)) depth++;
    else if (')]}'.includes(c) && --depth === 0) { out.push(text.slice(from, i - 1)); break; }
    else if (c === ',' && depth === 1) { out.push(text.slice(from, i - 1)); from = i; }
  }
  return out.map((a) => a.trim()).filter((a) => a !== '');
}
function withoutTests(text: string) {
  let out = '', from = 0;
  for (const m of text.matchAll(/#\[cfg\(test\)\]/g)) {
    if (m.index! < from) continue;
    out += text.slice(from, m.index);
    from = itemEnd(text, m.index! + m[0].length);
  }
  return out + text.slice(from);
}
// Page::send, the one place obid's browser hands a method on that it was given (page.rs): every other
// CDP call names its method as a literal
const FORWARD = { file: join('window', 'page.rs'), fn: 'async fn send(&self, tab: &str, method: &str, params: Value, until: Until) -> Step<Value> {', call: 'self.relay.cdp(tab, method, params, until.left())' };
function obidSends(files: { name: string; text: string }[]) {
  const sent = new Set<string>(), events = new Set<string>(), unread: string[] = [];
  for (const { name, text } of files) {
    const code = withoutTests(text);
    for (const m of code.matchAll(/(==\s*)?"([A-Z][A-Za-z]*\.[a-z][A-Za-z]*)"/g)) (m[1] ? events : sent).add(m[2]);
    // Page::send's body, if this is its file (the signature appears once)
    const fn = name === FORWARD.file ? code.indexOf(FORWARD.fn) : -1;
    const body = fn < 0 ? [0, 0] : [fn, itemEnd(code, fn)];
    // a CDP call is a `send` or `cdp` with a tab and a method (a channel's `send` takes one value)
    for (const m of code.matchAll(/\.(send|cdp)\s*\(/g)) {
      const open = m.index! + m[0].length - 1, a = args(code, open);
      if (a.length < 2 || /^"[^"]*"$/.test(a[1])) continue;
      const forward = m.index! > body[0] && m.index! < body[1] && `${code.slice(m.index! - 'self.relay'.length, m.index!)}.${m[1]}(${a.join(', ')})` === FORWARD.call;
      if (!forward) unread.push(`${name}: .${m[1]}(${a.join(', ')})`);
    }
  }
  return { sent, events, unread };
}
const browserSources = () => {
  const root = resolve(__dirname, '../../../../../..');
  const dir = join(root, 'core/crates/obid/src/browser');
  const files = (at: string): string[] => readdirSync(join(dir, at)).flatMap((n) => statSync(join(dir, at, n)).isDirectory() ? files(join(at, n)) : n.endsWith('.rs') ? [join(at, n)] : []);
  return files('').map((name) => ({ name, text: readFileSync(join(dir, name), 'utf8') }));
};

describe('BROWSERAPP-29: the methods main runs are the ones obid sends', () => {
  it('browserapp_29: METHODS is every CDP method obid\'s browser sends, plus main\'s own focus emulation, and nothing else', () => {
    const { sent, events, unread } = obidSends(browserSources());
    expect(unread).toEqual([]);
    expect([...events].sort()).toEqual(['Page.frameNavigated', 'Page.javascriptDialogOpening', 'Page.loadEventFired', 'Page.navigatedWithinDocument']);
    expect(sent.has('Page.navigate')).toBe(true);
    expect([...METHODS].sort()).toEqual([...sent, 'Emulation.setFocusEmulationEnabled'].sort());
  });

  it('browserapp_29: a method sent by production code after a test module is read; only the test items themselves are left out', () => {
    const sources = browserSources();
    const relay = sources.find((f) => f.name === 'relay.rs')!;
    expect(relay.text).toMatch(/^#\[cfg\(test\)\]/m);
    const later = { ...relay, text: `${relay.text}\nimpl WindowDriver {\n    async fn later(&self, tab: &str, until: Until) {\n        let _ = self.send(tab, "Network.getCookies", json!({}), until).await;\n    }\n}\n` };
    const { sent } = obidSends(sources.map((f) => (f === relay ? later : f)));
    expect(sent.has('Network.getCookies')).toBe(true);
    // the test module's own literals stay out (relay.rs's tests send DOM.focus only in test code)
    expect(obidSends([relay]).sent.has('DOM.focus')).toBe(false);
  });

  it('browserapp_29: a CDP call whose method isn\'t a literal is listed as unread, however its tab is passed; Page::send\'s own forward is the one exempt, and only where it is', () => {
    const sources = browserSources();
    const page = sources.find((f) => f.name === join('window', 'page.rs'))!;
    const added = (rust: string) => obidSends(sources.map((f) => (f === page ? { ...f, text: `${f.text}\nimpl WindowDriver {\n${rust}\n}\n` } : f))).unread;
    for (const call of [
      'self.send(tab, &format!("Network.{m}"), json!({}), until)',
      'self.send(tab.as_str(), &format!("Network.{m}"), json!({}), until)',
      'self.send(\n            &tabs[0],\n            m,\n            json!({}),\n            until,\n        )',
      'self.relay.cdp(&self.tab(job)?, METHOD, json!({}), until.left())',
    ]) expect(added(`    async fn built(&self, tab: &str, m: &str, until: Until) {\n        let _ = ${call}.await;\n    }`), call).toHaveLength(1);
    // the forward Page::send makes is exempt only inside Page::send itself
    expect(added('    async fn again(&self, tab: &str, method: &str, params: Value, until: Until) -> Step<Value> {\n        Ok(self.relay.cdp(tab, method, params, until.left()).await?)\n    }')).toHaveLength(1);
  });
});

describe('BROWSERAPP-24: a webview lives only in the partition obid named, hardened', () => {
  it('browserapp_24: only the two partition shapes; never the default session or the window\'s own', () => {
    for (const p of ['persist:oby-agent-claude', 'persist:oby-agent-codex', 'persist:oby-agent-cursor', 'oby-box-job-12_ab']) expect(partitionOk(p)).toBe(true);
    for (const p of [undefined, '', 'persist:oby-agent-gemini', 'persist:main', 'oby-box-', 'oby-box-a/b', 'persist:oby-box-j1', 'oby-agent-codex', `oby-box-${'a'.repeat(129)}`]) expect(partitionOk(p)).toBe(false);
  });

  it('browserapp_24: the partition is the one obid\'s first tab event named for that tab; another one later for it, or a bad shape, is refused; tabClosed forgets it', () => {
    const p = namedPartitions();
    const tab = (partition: unknown, id = 't1', kind = 'agent') => JSON.stringify({ seq: 1, type: 'tab', tab: { id, owner: { kind, job: 'j1', agent: 'codex' }, url: 'https://example.test/', partition } });
    expect(p.heard('tab', tab('persist:oby-agent-codex'))).toBe('kept');
    expect(p.heard('tab', tab('persist:oby-agent-codex'))).toBe('kept');
    expect(p.heard('tab', tab('oby-box-j1'))).toBe('refused');
    expect(p.of(K)).toBe('persist:oby-agent-codex');
    expect(p.heard('tab', tab('persist:main', 't2'))).toBe('refused');
    expect(p.of({ threadId: 'j1', tabId: 't2' })).toBeUndefined();
    expect(p.heard('tab', tab('persist:oby-agent-codex', 't3', 'you'))).toBeNull();
    expect(p.heard('tabClosed', JSON.stringify({ seq: 2, type: 'tabClosed', tabId: 't1' }))).toBeNull();
    expect(p.of(K)).toBeUndefined();
    expect(p.heard('tab', tab('oby-box-j1'))).toBe('kept');
  });

  it('browserapp_24: drop forgets a tab, and names its partition only once no other tab names it; a second drop names nothing', () => {
    const p = namedPartitions();
    const tab = (id: string) => JSON.stringify({ seq: 1, type: 'tab', tab: { id, owner: { kind: 'agent', job: 'j1', agent: 'codex' }, url: 'https://example.test/', partition: 'oby-box-j1' } });
    const kept: string[] = [], last: string[] = [];
    const on = { kept: (x: string) => kept.push(x), last: (x: string) => last.push(x) };
    expect(p.heard('tab', tab('t1'), on)).toBe('kept');
    expect(p.heard('tab', tab('t2'), on)).toBe('kept');
    expect(kept).toEqual(['oby-box-j1', 'oby-box-j1']);
    expect(p.drop(K)).toBeNull();
    expect(p.of(K)).toBeUndefined();
    expect(p.drop(K)).toBeNull();
    // the box's last tab closing goes the same way, its return value unchanged
    expect(p.heard('tabClosed', JSON.stringify({ seq: 2, type: 'tabClosed', tabId: 't2' }), on)).toBeNull();
    expect(last).toEqual(['oby-box-j1']);
    expect(p.heard('tab', tab('t1'), on)).toBe('kept');
    expect(p.drop(K)).toBe('oby-box-j1');
    expect(p.drop(K)).toBeNull();
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

describe('BROWSERAPP-39: a foreign job\'s door reaches main alone, in its one shape', () => {
  const token = 'a'.repeat(64);
  const door = (over: Record<string, unknown> = {}) => JSON.stringify({ type: 'box', threadId: 'j1', partition: 'oby-box-j1', proxy: `http://oby:${token}@127.0.0.1:50123`, ...over });

  it('browserapp_39: relayCommand takes box only for its own job\'s box partition and a loopback door with a 64-hex token', () => {
    expect(relayCommand('box', door())).toEqual({ type: 'box', threadId: 'j1', partition: 'oby-box-j1', proxy: `http://oby:${token}@127.0.0.1:50123` });
    expect(relayCommand('box', door({ proxy: `http://oby:${token}@127.0.0.1:65535` }))).not.toBeNull();
    for (const over of [
      { partition: 'oby-box-j2' },
      { partition: 'persist:oby-agent-codex' },
      { proxy: `http://oby:${token}@10.0.0.1:50123` },
      { proxy: `https://oby:${token}@127.0.0.1:50123` },
      { proxy: `http://me:${token}@127.0.0.1:50123` },
      { proxy: 'http://oby:@127.0.0.1:50123' },
      { proxy: `http://oby:${'a'.repeat(63)}@127.0.0.1:50123` },
      { proxy: `http://oby:${token}@127.0.0.1:0` },
      { proxy: `http://oby:${token}@127.0.0.1:65536` },
      { proxy: `http://oby:${token}@127.0.0.1:50123/` },
      { proxy: undefined },
      { type: 'release' },
    ]) expect(relayCommand('box', door(over)), JSON.stringify(over)).toBeNull();
  });

  it('browserapp_39: a box command reaches only its box option: never obid, the pointer or a console line', async () => {
    const said = (['log', 'info', 'warn', 'error', 'debug'] as const).map((m) => vi.spyOn(console, m));
    const o = fakeObid();
    const boxes: unknown[] = [], drawn: unknown[] = [];
    const host = relayHost(o.post, { home: HOME, pointer: (p) => drawn.push(p), box: (c) => boxes.push(c) });
    host.attach(K, fakeGuest());
    const c = relayCommand('box', door())!;
    host.command(c);
    await settle();
    expect(boxes).toEqual([c]);
    expect(drawn).toEqual([]);
    expect(JSON.stringify(o.all())).not.toContain(token);
    for (const spy of said) {
      expect(JSON.stringify(spy.mock.calls)).not.toContain(token);
      spy.mockRestore();
    }
  });
});
