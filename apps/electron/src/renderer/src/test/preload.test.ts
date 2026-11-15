import { beforeEach, describe, expect, it, vi } from 'vitest';

// What the preload tells main of a press of Confirm (docs/contracts/signed-words.md, WORDS-11),
// through its real listeners, with the page's few DOM parts stood in for.
const the = vi.hoisted(() => {
  const g = globalThis as unknown as Record<string, unknown>;
  class Element {
    constructor(public innerText = '', public shown = true) {}
    closest(what: string) { return what === 'button' ? this : null; }
    querySelector() { return null; }
    checkVisibility() { return this.shown; }
  }
  class HTMLElement extends Element {}
  class HTMLSelectElement extends HTMLElement {
    selectedOptions = [{ text: 'Bypass' }];
  }
  class KeyboardEvent {
    isComposing = false;
    type = 'keydown';
    constructor(public key: string, public target: unknown, public isTrusted = true) {}
  }
  const listeners = new Map<string, (e: unknown) => void>();
  Object.assign(g, {
    Element, HTMLElement, HTMLSelectElement, KeyboardEvent, HTMLTextAreaElement: class extends HTMLElement {}, HTMLInputElement: class extends HTMLElement {},
    addEventListener: (name: string, f: (e: unknown) => void) => listeners.set(name, f),
  });
  type Live = (onSnapshot: (s: { seq: number }) => void, onMessage: (m: { seq: number }) => void) => () => void;
  return {
    HTMLElement, HTMLSelectElement, KeyboardEvent, listeners, invoked: [] as unknown[][],
    exposed: {} as Record<string, { obid: { confirm: (id: string) => unknown; call: (method: string, path: string, body: unknown) => unknown; live: Live } }>,
    // what the page listens to from main, and main's answer to obid-snapshot
    on: new Map<string, (e: unknown, m: unknown) => void>(),
    snapshot: Promise.resolve(null as unknown),
  };
});
vi.mock('electron', () => ({
  contextBridge: { exposeInMainWorld: (name: string, api: never) => { the.exposed[name] = api; } },
  ipcRenderer: {
    sendSync: () => null, send: () => {},
    on: (channel: string, f: (e: unknown, m: unknown) => void) => { the.on.set(channel, f); },
    off: (channel: string, f: unknown) => { if (the.on.get(channel) === f) the.on.delete(channel); },
    invoke: async (...args: unknown[]) => { the.invoked.push(args); return args[0] === 'obid-snapshot' ? the.snapshot : { status: 200, text: '' }; },
  },
}));
await import('../../../preload/index');

const press = (label: string, how: Partial<{ isTrusted: boolean; shown: boolean }> = {}) =>
  the.listeners.get('pointerdown')!({ isTrusted: how.isTrusted ?? true, type: 'pointerdown', target: new the.HTMLElement(label, how.shown ?? true) });
const sent = async () => { await the.exposed.app.obid.confirm('c1'); return the.invoked.at(-1)!; };

describe('the preload on a press of Confirm', () => {
  it('words_14_a_select_change_holds_its_option_once_within_two_seconds', async () => {
    vi.useFakeTimers();
    try {
      const select = new the.HTMLSelectElement();
      const change = (isTrusted = true) => the.listeners.get('change')!({ type: 'change', isTrusted, target: select });
      const send = async () => {
        await the.exposed.app.obid.call('PUT', '/v1/settings/permissions', { mode: 'bypass' });
        return the.invoked.at(-1)![4];
      };
      change(false);
      expect(await send()).toBe(false);
      select.shown = false;
      change();
      expect(await send()).toBe(false);
      select.shown = true;
      change();
      expect(await send()).toBe(true);
      expect(await send()).toBe(false);
      change();
      vi.advanceTimersByTime(2000);
      expect(await send()).toBe(false);
    } finally { vi.useRealTimers(); }
  });
  beforeEach(async () => { while ((await sent())[2]) { /* a press from another test */ } });

  it('words_11: a real press of Confirm says so, once', async () => {
    press('Confirm');
    expect((await sent())[2]).toBe(true);
    expect((await sent())[2]).toBe(false);
  });
  it('words_11: an input elsewhere, a script\'s event or a hidden button says no', async () => {
    press('Cancel');
    expect((await sent())[2]).toBe(false);
    press('Confirm', { isTrusted: false });
    expect((await sent())[2]).toBe(false);
    press('Confirm', { shown: false });
    expect((await sent())[2]).toBe(false);
  });
  it('words_11: Enter and Space on the focused button count', async () => {
    for (const key of ['Enter', ' ']) {
      the.listeners.get('keydown')!(new the.KeyboardEvent(key, new the.HTMLElement('Confirm')));
      expect((await sent())[2]).toBe(true);
    }
  });
});

describe('the preload following main (main/live.ts)', () => {
  it('obid.live: main’s snapshot first, then each message past it once, in order; none at or below it', async () => {
    let answer!: (s: unknown) => void;
    the.snapshot = new Promise((done) => { answer = done; });
    const got: string[] = [];
    const stop = the.exposed.app.obid.live((s) => got.push(`snapshot ${s.seq}`), (m) => got.push(`message ${m.seq}`));
    // listening before it asks
    expect(the.invoked.at(-1)).toEqual(['obid-snapshot']);
    const send = (seq: number, kind = 'event') => the.on.get('obid-live')?.({}, { seq, kind, event: { kind: 'jobGone', id: `j${seq}` } });
    // sent while the snapshot was on its way: 4 and 5 are in it, 6 isn't
    send(4);
    send(5);
    send(6);
    expect(got).toEqual([]);
    answer({ seq: 5, connection: 'live', problem: null, state: {}, limits: [], complete: true });
    await new Promise((done) => setTimeout(done, 0));
    expect(got).toEqual(['snapshot 5', 'message 6']);
    // the same again, then what comes next: a snapshot after a sync goes to onSnapshot
    send(6);
    send(7, 'snapshot');
    send(8);
    expect(got).toEqual(['snapshot 5', 'message 6', 'snapshot 7', 'message 8']);
    stop();
    send(9);
    expect(got).toHaveLength(4);
    expect(the.on.has('obid-live')).toBe(false);
  });
});
