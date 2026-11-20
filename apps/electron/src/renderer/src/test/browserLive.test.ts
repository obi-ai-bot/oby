import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { owners, wordsOf, TAKE_CONTROL, GIVE_BACK } from '../../../shared/words';
import { mayInput, sawInput } from '../../../main/browserInput';
import { connectLive, LINGER_MS, type Sinks } from '../browser/live';
import type { BrowserState } from '../browser/view';
import type { BrowserTab } from '../../../shared/obid-wire';

// The browser on obid (docs/contracts/browser-app.md, BROWSERAPP-8 to 10), against a mock of the
// contract (docs/api/browser.md) until obid serves its routes.

const TAKE = '/v1/browser/tabs/t1/take', GIVE = '/v1/browser/tabs/t1/giveBack';

describe('BROWSERAPP-9: Take control and Give back are signed by their own labels', () => {
  it('browserapp_9: an empty take is Take control, an empty give back is Give back', () => {
    expect(wordsOf('POST', TAKE, {})).toEqual([TAKE_CONTROL]);
    expect(wordsOf('POST', GIVE, {})).toEqual([GIVE_BACK]);
  });
  it('browserapp_9: a body with anything in it, another method, or another verb signs nothing', () => {
    expect(wordsOf('POST', TAKE, { device: 'x' })).toBeNull();
    expect(wordsOf('PUT', TAKE, {})).toBeNull();
    expect(wordsOf('POST', '/v1/browser/tabs/t1/close', {})).toBeNull();
    expect(wordsOf('POST', TAKE, undefined)).toBeNull();
  });
  it('browserapp_9: a take is held by Take control, Take control to answer or Type the password; never by Give back or a card\'s words', () => {
    for (const label of ['Take control', 'Take control to answer', 'Type the password']) expect(owners('POST', TAKE, {}, [label], [])).toEqual([0]);
    expect(owners('POST', TAKE, {}, ['Give back'], [])).toBeNull();
    expect(owners('POST', TAKE, {}, ['Codex asks you to sign in here'], [])).toBeNull();
    expect(owners('POST', GIVE, {}, ['Give back'], [])).toEqual([0]);
    expect(owners('POST', GIVE, {}, ['Take control'], [])).toBeNull();
  });
});

describe('BROWSERAPP-10: the owner\'s input reaches the page only right after the very input main saw', () => {
  const press = { type: 'mouse', eventType: 'mousePressed', x: 10, y: 20, button: 'left', clickCount: 1 } as const;
  const move = { type: 'mouse', eventType: 'mouseMoved', x: 10, y: 20 } as const;
  const key = (eventType: 'keyDown' | 'keyUp' | 'char', k: string) => ({ type: 'key', eventType, key: k } as const);

  it('browserapp_10: with no real input, nothing is signed', () => {
    expect(mayInput(press, 10_000_000)).toBe(false);
    expect(mayInput(key('keyDown', 'a'), 10_000_000)).toBe(false);
  });
  it('browserapp_10: a press follows one real mouse-down, once; a move follows any real move in the last 2 s', () => {
    sawInput('mouseDown', undefined, 20_000_000);
    sawInput('mouseMove', undefined, 20_000_000);
    expect(mayInput(press, 20_000_100)).toBe(true);
    expect(mayInput(press, 20_000_200)).toBe(false);
    expect(mayInput(move, 20_000_300)).toBe(true);
    expect(mayInput(move, 20_000_400)).toBe(true);
    expect(mayInput(move, 20_003_000)).toBe(false);
  });
  it('browserapp_10: a key follows the very key main saw, and a char rides on its keyDown', () => {
    sawInput('keyDown', 'a', 30_000_000);
    expect(mayInput(key('keyDown', 'b'), 30_000_050)).toBe(false);
    expect(mayInput(key('char', 'a'), 30_000_060)).toBe(true);
    expect(mayInput(key('keyDown', 'a'), 30_000_070)).toBe(true);
    expect(mayInput(key('keyDown', 'a'), 30_000_080)).toBe(false);
    expect(mayInput(key('keyUp', 'a'), 30_000_090)).toBe(false);
  });
  it('browserapp_10: a shape that isn\'t the contract\'s signs nothing', () => {
    sawInput('mouseDown', undefined, 40_000_000);
    expect(mayInput({ type: 'mouse', eventType: 'keyDown', x: 1, y: 1 }, 40_000_010)).toBe(false);
    expect(mayInput({ kind: 'mouse', type: 'mousePressed' }, 40_000_010)).toBe(false);
    expect(mayInput(null, 40_000_010)).toBe(false);
  });
});

// A mock of obid's browser: its lists, its events (pushed by the test) and its frames
function mockObid(lists: { tabs: BrowserTab[]; seq?: number; status?: number }) {
  const calls: { method: string; path: string; body?: unknown }[] = [];
  let push: ((m: { name: string; data: string }) => void) | null = null;
  const frames = new Map<string, (data: string) => void>();
  const api = {
    call: vi.fn(async (method: string, path: string, body?: unknown) => {
      calls.push({ method, path, body });
      if (lists.status) return { status: lists.status, text: '{"error":"Not found."}' };
      if (path === '/v1/browser') return { status: 200, text: JSON.stringify({ tabs: lists.tabs, settings: { agentsUseBrowser: true, agentsUseMac: true }, browsers: [], seq: lists.seq }) };
      if (path === '/v1/browser/downloads') return { status: 200, text: JSON.stringify({ downloads: [], seq: lists.seq }) };
      return { status: 200, text: '{}' };
    }),
    follow: vi.fn((fn: (m: { name: string; data: string }) => void) => { push = fn; return () => { push = null; }; }),
    watch: vi.fn((tab: string, fn: (data: string) => void) => { frames.set(tab, fn); return () => frames.delete(tab); }),
  };
  return { api, calls, event: (e: object) => push?.({ name: (e as { type: string }).type, data: JSON.stringify(e) }), frame: (tab: string, f: object) => frames.get(tab)?.(JSON.stringify(f)), following: () => !!push };
}
const tab = (id: string, extra: Partial<BrowserTab> = {}): BrowserTab => ({
  id, owner: { kind: 'agent', job: 'j1', agent: 'codex' }, title: 'Sign in · Acme', url: 'http://localhost:3000/login', state: 'ready',
  heldBy: null, holdGeneration: 0, computer: { id: 'local', name: 'This Mac' }, ask: null, dialog: null, ...extra,
});
const settle = () => new Promise((r) => setTimeout(r, 0));

describe('BROWSERAPP-8: on obid, the panel follows obid\'s browser through main', () => {
  let state: BrowserState | null, offered: boolean | null;
  const frames: { tab: string; url: string }[] = [];
  const sinks: Sinks = { offered: (on) => { offered = on; }, state: (s) => { state = s; }, frame: (t, url) => frames.push({ tab: t, url }), problem: () => {} };
  beforeEach(() => { state = null; offered = null; frames.length = 0; });
  afterEach(() => vi.useRealTimers());

  it('browserapp_8: an obid without the browser (404) isn\'t offered and nothing is followed', async () => {
    const o = mockObid({ tabs: [], status: 404 });
    connectLive(sinks, o.api);
    await settle(); await settle();
    expect(offered).toBe(false);
    expect(o.following()).toBe(false);
  });
  it('browserapp_8: the tabs come from GET /v1/browser, each tab\'s frames are drawn, and every call is a /v1/browser one (main adds the token)', async () => {
    const o = mockObid({ tabs: [tab('t1')], seq: 3 });
    connectLive(sinks, o.api);
    await settle(); await settle();
    expect(offered).toBe(true);
    expect(state?.tabs.map((t) => [t.id, t.url, t.holder, t.job])).toEqual([['t1', 'localhost:3000/login', 'agent', 'j1']]);
    expect(o.following()).toBe(true);
    o.frame('t1', { tabId: 't1', seq: 1, data: 'AAAA', mime: 'image/jpeg', width: 800, height: 600, dropped: 0 });
    expect(frames).toEqual([{ tab: 't1', url: 'data:image/jpeg;base64,AAAA' }]);
    expect(o.calls.every((c) => c.path.startsWith('/v1/browser'))).toBe(true);
  });
  it('browserapp_8: an event after the list is applied; one the list already reflects isn\'t; a list older than an event never rolls the screen back', async () => {
    const lists = { tabs: [tab('t1')], seq: 5 };
    const o = mockObid(lists);
    connectLive(sinks, o.api);
    await settle(); await settle();
    o.event({ seq: 4, type: 'tab', tab: tab('t1', { title: 'Old' }) });
    expect(state?.tabs[0].title).toBe('Sign in · Acme');
    o.event({ seq: 6, type: 'tab', tab: tab('t2', { title: 'Sign up · Acme' }) });
    expect(state?.tabs.map((t) => t.id)).toEqual(['t1', 't2']);
    // the stream reconnects and the list is read again, but this answer is older than event 6
    lists.tabs = [tab('t1')]; lists.seq = 5;
    o.api.follow.mock.calls[0][0]({ name: 'open', data: '' });
    await settle(); await settle();
    expect(state?.tabs.map((t) => t.id)).toEqual(['t1', 't2']);
  });
  it('browserapp_8: what an agent does shows on its tab, and stays LINGER_MS after it ends', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout'] });
    const o = mockObid({ tabs: [tab('t1')], seq: 1 });
    connectLive(sinks, o.api);
    await vi.waitFor(() => expect(offered).toBe(true));
    o.event({ seq: 2, type: 'action', tabId: 't1', job: 'j1', tool: 'oby_browser_act', text: 'Clicking Sign in', pointer: { x: 3, y: 4 }, outcome: 'started', reason: null });
    expect(state?.tabs[0].action).toBe('Clicking Sign in');
    o.event({ seq: 3, type: 'action', tabId: 't1', job: 'j1', tool: 'oby_browser_act', text: 'Clicking Sign in', pointer: { x: 3, y: 4 }, outcome: 'done', reason: null });
    expect(state?.tabs[0].action).toBe('Clicking Sign in');
    vi.advanceTimersByTime(LINGER_MS + 10);
    expect(state?.tabs[0].action).toBeUndefined();
  });
  it('browserapp_9: Take control posts an empty body to the tab\'s take, Give back to its giveBack; a held tab reads as yours', async () => {
    const lists = { tabs: [tab('t1')], seq: 1 };
    const o = mockObid(lists);
    const live = connectLive(sinks, o.api);
    await settle(); await settle();
    lists.tabs = [tab('t1', { heldBy: 'device-1', holdGeneration: 1 })]; lists.seq = 2;
    await live.take('t1');
    expect(o.calls).toContainEqual({ method: 'POST', path: TAKE, body: {} });
    expect(state?.tabs[0].holder).toBe('you');
    await live.giveBack('t1');
    expect(o.calls).toContainEqual({ method: 'POST', path: GIVE, body: {} });
  });
});
