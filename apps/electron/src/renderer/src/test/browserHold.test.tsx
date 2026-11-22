// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BrowserProvider, useBrowser, type Browser } from '../browser/store';
import { BrowserGlobe, BrowserPane } from '../browser/BrowserPane';
import { BrowserWebviews } from '../browser/Webviews';

// Board 6 on obid (docs/contracts/browser-app.md, BROWSERAPP-40, 41): once obid takes the tab the
// keyboard is on its page, as T3 Code's Take control puts it there; once obid answers the give back
// it is back in the message box. The held page is ringed over its webview, with no agent cursor.

const TAKE = '/v1/browser/tabs/t1/take', GIVE = '/v1/browser/tabs/t1/giveBack';
const tab = (heldBy: string | null) => ({
  id: 't1', owner: { kind: 'agent', job: 'j1', agent: 'codex' }, title: 'Example', url: 'https://example.test/t1', state: 'ready',
  partition: 'persist:oby-agent-codex', heldBy, holdGeneration: heldBy ? 1 : 0, computer: { id: 'local', name: 'This Mac' }, ask: null, dialog: null,
});

describe('BROWSERAPP-40, 41: Take control puts the keyboard on the page, Give back returns it, the ring over the webview', () => {
  let root: Root, host: HTMLDivElement;
  let pointer: ((p: { threadId: string; tabId: string; phase: 'move' | 'click'; x: number; y: number }) => void) | null;
  // what obid answers: the take's and the give back's status, and who holds the tab after
  let obid: { take: number; give: number; heldBy: string | null };
  let browser: Browser;
  const calls: string[] = [];
  function Spy() { browser = useBrowser(); return null; }
  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div'); document.body.append(host); root = createRoot(host);
    pointer = null; calls.length = 0; obid = { take: 200, give: 200, heldBy: null };
  });
  afterEach(() => { act(() => root.unmount()); host.remove(); document.body.innerHTML = ''; delete (window as unknown as { app?: unknown }).app; });

  async function live(heldBy: string | null = null) {
    obid.heldBy = heldBy;
    const call = async (m: string, path: string) => {
      calls.push(`${m} ${path}`);
      if (path === TAKE || path === GIVE) {
        const status = path === TAKE ? obid.take : obid.give;
        if (status !== 200) return { status, text: JSON.stringify({ error: 'Another of your devices has this tab.', reason: 'held_by_other' }) };
        obid.heldBy = path === TAKE ? 'device-1' : null;
        return { status, text: JSON.stringify({ tabId: 't1', heldBy: obid.heldBy, holdGeneration: 1 }) };
      }
      return { status: 200, text: JSON.stringify(path === '/v1/browser' ? { tabs: [tab(obid.heldBy)], settings: { agentsUseBrowser: true, agentsUseMac: true }, seq: 1 } : { downloads: [], seq: 1 }) };
    };
    (window as unknown as { app: unknown }).app = {
      obid: { call },
      browser: { follow: () => () => {}, fill: async () => null, reveal: async () => true, navState: () => () => {}, register: async () => null, shown: () => {}, pointer: (fn: typeof pointer) => { pointer = fn; return () => { pointer = null; }; } },
    };
    await act(async () => { root.render(<BrowserProvider board={null} live><Spy /><BrowserGlobe /><BrowserPane width={640} /><BrowserWebviews /></BrowserProvider>); });
    await settle();
  }
  const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  const open = () => act(async () => { (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click(); });
  const button = (name: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === name) as HTMLButtonElement;
  const click = async (name: string) => { await act(async () => { button(name).click(); }); await settle(); };
  const webview = () => host.querySelector('[data-browser="webview"] webview');
  const hosted = () => host.querySelector('[data-browser="webview"]')!;
  const drawn = () => host.querySelector('[data-browser="webview"] [data-browser="pointer"]') as HTMLElement | null;
  const planted = (html: string, css: string) => { const d = document.createElement('div'); d.innerHTML = html; document.body.append(d); return d.querySelector(css) as HTMLElement; };

  it('browserapp_40_take_control_puts_the_keyboard_on_the_tabs_page_once_obid_takes_it', async () => {
    await live();
    await open();
    await click('Take control');
    expect(calls).toContain(`POST ${TAKE}`);
    expect(button('Give back')).toBeDefined();
    expect(document.activeElement).toBe(webview());
  });

  it('browserapp_40_a_refused_take_or_a_tab_off_screen_moves_no_focus', async () => {
    const where = planted('<input id="elsewhere">', 'input');
    where.focus();
    obid.take = 409;
    await live();
    await open();
    await click('Take control');
    expect(calls).toContain(`POST ${TAKE}`);
    expect(document.activeElement).toBe(where);
    // taken, but its page isn't on screen: the panel closed
    obid.take = 200;
    await open();
    expect(host.querySelector('[data-browser="slot"]')).toBeNull();
    await act(async () => { await browser.api.take('t1'); });
    await settle();
    expect(obid.heldBy).toBe('device-1');
    expect(document.activeElement).toBe(where);
  });

  it('browserapp_41_give_back_returns_the_keyboard_to_the_message_box', async () => {
    // the conversation's message box first, Home's beside it
    const home = planted('<div class="composer"><div class="composer-input" tabindex="0"></div></div>', '.composer-input');
    const chat = planted('<div class="chat-layer"><div class="composer"><div contenteditable="true"></div></div></div>', '[contenteditable="true"]');
    await live('device-1');
    await open();
    (webview() as HTMLElement).focus();
    obid.give = 409;
    await click('Give back');
    expect(calls).toContain(`POST ${GIVE}`);
    expect(document.activeElement).toBe(webview());
    obid.give = 200;
    await click('Give back');
    expect(document.activeElement).toBe(chat);
    // with no conversation's, Home's
    chat.closest('.chat-layer')!.remove();
    await click('Take control');
    expect(document.activeElement).toBe(webview());
    await click('Give back');
    expect(document.activeElement).toBe(home);
  });

  it('browserapp_41_a_held_tabs_page_is_ringed_over_its_webview_and_shows_no_agent_cursor', async () => {
    await live('device-1');
    await open();
    expect(hosted().classList.contains('is-held')).toBe(true);
    await act(async () => { pointer?.({ threadId: 'j1', tabId: 't1', phase: 'click', x: 12, y: 34 }); });
    expect(drawn()).toBeNull();
    await click('Give back');
    expect(hosted().classList.contains('is-held')).toBe(false);
    await act(async () => { pointer?.({ threadId: 'j1', tabId: 't1', phase: 'move', x: 56, y: 78 }); });
    expect([drawn()?.style.left, drawn()?.style.top]).toEqual(['56px', '78px']);
    // a pointer already drawn when the hold starts is cleared
    await click('Take control');
    expect(hosted().classList.contains('is-held')).toBe(true);
    expect(drawn()).toBeNull();
  });
});
