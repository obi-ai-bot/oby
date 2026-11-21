// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BrowserProvider } from '../browser/store';
import { BrowserGlobe, BrowserPane } from '../browser/BrowserPane';
import { BrowserWebviews } from '../browser/Webviews';

// The agents' pages as real <webview>s in the window (docs/plans/browser-t3.md, W2), as T3 Code
// hosts them: one per agent tab, mounted once at the root in obid's partition, tied to obid's tab
// by main (browser-register), drawn where the panel's slot is.

const tab = (id: string, extra: object = {}) => ({
  id, owner: { kind: 'agent', job: 'j1', agent: 'codex' }, title: 'Example', url: `https://example.test/${id}`, state: 'ready',
  heldBy: null, holdGeneration: 0, computer: { id: 'local', name: 'This Mac' }, ask: null, dialog: null, ...extra,
});

describe('BROWSERAPP-26: each agent tab is a real webview in the window, in the partition obid named', () => {
  let root: Root, host: HTMLDivElement;
  let pointer: ((p: { threadId: string; tabId: string; phase: 'move' | 'click'; x: number; y: number }) => void) | null;
  const register = vi.fn(async (_j: string, _t: string, _id: number) => null as string | null);
  const shown = vi.fn((_j: string, _t: string, _s: boolean) => {});
  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div'); document.body.append(host); root = createRoot(host);
    register.mockClear(); shown.mockClear(); pointer = null;
  });
  afterEach(() => { act(() => root.unmount()); host.remove(); delete (window as unknown as { app?: unknown }).app; vi.useRealTimers(); });

  async function live(tabs: object[]) {
    const call = async (_m: string, path: string) => ({ status: 200, text: JSON.stringify(path === '/v1/browser' ? { tabs, settings: { agentsUseBrowser: true, agentsUseMac: true }, seq: 1 } : { downloads: [], seq: 1 }) });
    (window as unknown as { app: unknown }).app = {
      obid: { call },
      browser: { follow: () => () => {}, fill: async () => null, reveal: async () => true, register, shown, pointer: (fn: typeof pointer) => { pointer = fn; return () => { pointer = null; }; } },
    };
    await act(async () => { root.render(<BrowserProvider board={null} live><BrowserGlobe /><BrowserPane width={640} /><BrowserWebviews /></BrowserProvider>); });
    await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  }
  const webviews = () => [...host.querySelectorAll('webview')] as (HTMLElement & { getWebContentsId?: () => number; getURL?: () => string })[];

  it('browserapp_26: a tab obid gave a partition gets one webview at its address in that partition; one without (an obid before the relay) gets none', async () => {
    await live([tab('t1', { partition: 'persist:oby-agent-codex' }), tab('t2')]);
    expect(webviews().map((w) => [w.getAttribute('src'), w.getAttribute('partition')])).toEqual([['https://example.test/t1', 'persist:oby-agent-codex']]);
    expect(host.querySelector('[data-browser="webview"]')?.getAttribute('data-tab')).toBe('t1');
  });

  it('browserapp_26: attached, it is registered with main as obid\'s tab; shown while the panel draws its slot, not otherwise', async () => {
    await live([tab('t1', { partition: 'persist:oby-agent-codex' })]);
    const w = webviews()[0];
    w.getWebContentsId = () => 42;
    await act(async () => { w.dispatchEvent(new Event('did-attach')); });
    expect(register).toHaveBeenCalledWith('j1', 't1', 42);
    expect(shown).toHaveBeenLastCalledWith('j1', 't1', false);
    await act(async () => { (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click(); });
    expect(host.querySelector('[data-browser="slot"]')).not.toBeNull();
    expect(shown).toHaveBeenLastCalledWith('j1', 't1', true);
    // the same webview, not a new one: Electron would destroy a moved one's page
    expect(webviews()).toEqual([w]);
  });

  it('browserapp_26: a webview already attached when it mounts (its did-attach came first) is still registered, once', async () => {
    // the live run's race: the guest attached 9 ms after the tab event, before the page's listener
    const proto = HTMLElement.prototype as unknown as { getWebContentsId?: () => number };
    proto.getWebContentsId = () => 7;
    try {
      await live([tab('t1', { partition: 'persist:oby-agent-codex' })]);
      expect(register).toHaveBeenCalledWith('j1', 't1', 7);
      await act(async () => { webviews()[0].dispatchEvent(new Event('did-attach')); webviews()[0].dispatchEvent(new Event('dom-ready')); });
      expect(register).toHaveBeenCalledTimes(1);
    } finally {
      delete proto.getWebContentsId;
    }
  });

  it('browserapp_9: on obid, Take control puts the keyboard on the tab\'s own page; the strip stays as the board draws a tab nobody holds; nothing goes to obid', async () => {
    const posts: string[] = [];
    await live([tab('t1', { partition: 'persist:oby-agent-codex' })]);
    const call = (window as unknown as { app: { obid: { call: (m: string, p: string) => Promise<unknown> } } }).app.obid.call;
    (window as unknown as { app: { obid: { call: unknown } } }).app.obid.call = async (m: string, p: string) => { if (m !== 'GET') posts.push(`${m} ${p}`); return call(m, p); };
    await act(async () => { (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click(); });
    const page = webviews()[0];
    const focus = vi.spyOn(page, 'focus');
    const button = (label: string) => [...host.querySelectorAll('button')].find((b) => b.textContent?.trim() === label) as HTMLButtonElement | undefined;
    const strip = () => host.querySelector('[data-browser="control"]')?.textContent;
    const before = strip();
    await act(async () => { button('Take control')!.click(); });
    expect(focus).toHaveBeenCalled();
    // no words of ours: Sangam's screens have none yet for you on a page nobody holds
    expect(strip()).toBe(before);
    expect(strip()).toContain('Codex opened this tab · idle');
    expect(button('Give back')).toBeUndefined();
    expect(posts).toEqual([]);
  });

  it('browserapp_26: a crashed page comes back at its last address; the agent\'s pointer is drawn over it while it shows', async () => {
    await live([tab('t1', { partition: 'persist:oby-agent-codex' })]);
    await act(async () => { (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click(); });
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const w = webviews()[0];
    w.getURL = () => 'https://example.test/t1/after';
    await act(async () => { w.dispatchEvent(new Event('render-process-gone')); });
    await act(async () => { vi.advanceTimersByTime(300); });
    expect(webviews()).toHaveLength(1);
    expect(webviews()[0]).not.toBe(w);
    expect(webviews()[0].getAttribute('src')).toBe('https://example.test/t1/after');
    await act(async () => { pointer?.({ threadId: 'j1', tabId: 't1', phase: 'click', x: 12, y: 34 }); });
    const drawn = host.querySelector('[data-browser="webview"] [data-browser="pointer"]') as HTMLElement | null;
    expect([drawn?.style.left, drawn?.style.top]).toEqual(['12px', '34px']);
    await act(async () => { pointer?.({ threadId: 'j2', tabId: 't1', phase: 'move', x: 1, y: 1 }); });
    expect((host.querySelector('[data-browser="webview"] [data-browser="pointer"]') as HTMLElement).style.left).toBe('12px');
    await act(async () => { vi.advanceTimersByTime(800); });
    expect(host.querySelector('[data-browser="webview"] [data-browser="pointer"]')).toBeNull();
  });
});
