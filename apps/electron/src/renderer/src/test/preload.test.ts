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
  class KeyboardEvent {
    isComposing = false;
    type = 'keydown';
    constructor(public key: string, public target: unknown, public isTrusted = true) {}
  }
  const listeners = new Map<string, (e: unknown) => void>();
  Object.assign(g, {
    Element, HTMLElement, KeyboardEvent, HTMLTextAreaElement: class extends HTMLElement {}, HTMLInputElement: class extends HTMLElement {},
    addEventListener: (name: string, f: (e: unknown) => void) => listeners.set(name, f),
  });
  return { HTMLElement, KeyboardEvent, listeners, invoked: [] as unknown[][], exposed: {} as Record<string, { obid: { confirm: (id: string) => unknown } }> };
});
vi.mock('electron', () => ({
  contextBridge: { exposeInMainWorld: (name: string, api: never) => { the.exposed[name] = api; } },
  ipcRenderer: { sendSync: () => null, on: () => {}, off: () => {}, send: () => {}, invoke: async (...args: unknown[]) => { the.invoked.push(args); return { status: 200, text: '' }; } },
}));
await import('../../../preload/index');

const press = (label: string, how: Partial<{ isTrusted: boolean; shown: boolean }> = {}) =>
  the.listeners.get('pointerdown')!({ isTrusted: how.isTrusted ?? true, type: 'pointerdown', target: new the.HTMLElement(label, how.shown ?? true) });
const sent = async () => { await the.exposed.app.obid.confirm('c1'); return the.invoked.at(-1)!; };

describe('the preload on a press of Confirm', () => {
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
