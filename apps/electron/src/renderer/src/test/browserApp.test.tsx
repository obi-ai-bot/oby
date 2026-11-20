// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { browserFit, CONVERSATION_MIN } from '../browser/fit';
import { PANEL_DEFAULT, PANEL_MIN, BrowserProvider, demoNumber } from '../browser/store';
import { BrowserGlobe, BrowserPane } from '../browser/BrowserPane';
import { BrowserAsks } from '../browser/Asks';
import { headOf } from '../browser/cards';

// The browser in the app (docs/contracts/browser-app.md): how the panel fits beside the
// conversation, and that it isn't offered on obid until main follows obid's browser.

describe('BROWSERAPP-3: the conversation keeps 420px beside the browser', () => {
  it('browserapp_3: a 994px window gives the panel 574px (board 4)', () => {
    expect(browserFit(994, 0, PANEL_DEFAULT)).toEqual({ width: 574, sidebarDocks: true });
  });
  it('browserapp_3: at 1440px beside a docked 362px side panel, a 600px panel keeps its width and the conversation 478px (board 3)', () => {
    const fit = browserFit(1440, 362, 600);
    expect(fit).toEqual({ width: 600, sidebarDocks: true });
    expect(1440 - 362 - fit.width).toBe(478);
  });
  it('browserapp_3: where the docked side panel would leave the browser under 360px, the side panel floats and the browser keeps its room', () => {
    const fit = browserFit(1100, 362, PANEL_DEFAULT);
    expect(fit.sidebarDocks).toBe(false);
    expect(fit.width).toBe(PANEL_DEFAULT);
    expect(1100 - fit.width).toBeGreaterThanOrEqual(CONVERSATION_MIN);
  });
  it('browserapp_3: the panel never goes under its narrowest', () => {
    expect(browserFit(600, 0, PANEL_DEFAULT).width).toBe(PANEL_MIN);
  });
});

describe('BROWSERAPP-7: offered on obid only once obid answers GET /v1/browser', () => {
  let root: Root, host: HTMLDivElement;
  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div'); document.body.append(host); root = createRoot(host);
  });
  afterEach(() => { act(() => root.unmount()); host.remove(); });
  const draw = (board: number | null) => act(() => root.render(<BrowserProvider board={board}><BrowserGlobe /><BrowserPane width={640} /></BrowserProvider>));

  it('browserapp_7: without ?everything there is no demo board to show', () => {
    expect(demoNumber()).toBeNull();
  });
  it('browserapp_7: with neither obid nor the demo there is no globe and no panel', () => {
    draw(null);
    expect(host.querySelector('[data-browser="globe"]')).toBeNull();
    expect(host.querySelector('aside.br')).toBeNull();
  });
  it('browserapp_7: on an obid without the browser (GET /v1/browser answers 404) there is no globe, and nothing is followed', async () => {
    const follow = vi.fn(() => () => {});
    (window as unknown as { app: unknown }).app = { obid: { call: async () => ({ status: 404, text: '{"error":"Not found."}' }) }, browser: { follow, watch: () => () => {} } };
    await act(async () => { root.render(<BrowserProvider board={null} live><BrowserGlobe /><BrowserPane width={640} /></BrowserProvider>); });
    await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
    expect(host.querySelector('[data-browser="globe"]')).toBeNull();
    expect(follow).not.toHaveBeenCalled();
    delete (window as unknown as { app?: unknown }).app;
  });
  it('browserapp_7: on an obid that answers GET /v1/browser, the globe shows and says how many tabs', async () => {
    const tabs = [{ id: 't1', owner: { kind: 'agent', job: 'j1', agent: 'codex' }, title: 'Sign in', url: 'http://localhost:3000/', state: 'ready', heldBy: null, holdGeneration: 0, computer: { id: 'local', name: 'This Mac' }, ask: null, dialog: null }];
    const call = async (_m: string, path: string) => ({ status: 200, text: JSON.stringify(path === '/v1/browser' ? { tabs, settings: { agentsUseBrowser: true, agentsUseMac: true }, seq: 1 } : { downloads: [], seq: 1 }) });
    (window as unknown as { app: unknown }).app = { obid: { call }, browser: { follow: () => () => {}, watch: () => () => {} } };
    await act(async () => { root.render(<BrowserProvider board={null} live><BrowserGlobe /><BrowserPane width={640} /></BrowserProvider>); });
    await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
    expect(host.querySelector('[data-browser="globe"]')?.getAttribute('aria-label')).toBe('Show the browser, 1 tab');
    delete (window as unknown as { app?: unknown }).app;
  });
  it('browserapp_3: one panel at the window\'s right at a time: a task\'s Changes puts the browser away, and the browser opening closes Changes', async () => {
    const { openChanges, useChangesOpen } = await import('../ChangesPanel');
    let changes: string | null = null;
    const Spy = () => { changes = useChangesOpen(); return null; };
    act(() => root.render(<BrowserProvider board={1}><BrowserGlobe /><BrowserPane width={640} /><Spy /></BrowserProvider>));
    act(() => (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click());
    expect(host.querySelector('aside.br')).not.toBeNull();
    act(() => openChanges('j1'));
    expect(host.querySelector('aside.br')).toBeNull();
    expect(changes).toBe('j1');
    act(() => (host.querySelector('[data-browser="globe"]') as HTMLButtonElement).click());
    expect(host.querySelector('aside.br')).not.toBeNull();
    expect(changes).toBeNull();
  });
  it('browserapp_7: on the demo data at board 2, the globe and the panel show (the check above can fail)', () => {
    draw(2);
    expect(host.querySelector('[data-browser="globe"]')?.getAttribute('aria-label')).toBe('Hide the browser');
    expect(host.querySelector('aside.br .br-control-text')?.textContent).toBe('Codex is using this tab · Clicking Sign in');
  });
});

describe('BROWSERAPP-11 to 13: the agent\'s asks over the message box', () => {
  let root: Root, host: HTMLDivElement;
  beforeEach(() => { vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
  afterEach(() => { act(() => root.unmount()); host.remove(); });
  const text = (css: string) => host.querySelector(css)?.textContent ?? null;
  const press = (label: string) => act(() => ([...host.querySelectorAll('button')].find((b) => b.textContent === label) as HTMLButtonElement).click());

  it('browserapp_11: an agent\'s first sentence heads its card, a question keeps its mark', () => {
    expect(headOf('Sign in to the staging admin. The mail log is behind a sign-in.', 'x')).toEqual({ head: 'Sign in to the staging admin', lead: 'The mail log is behind a sign-in.' });
    expect(headOf('Which plan? Both work.', 'x')).toEqual({ head: 'Which plan?', lead: 'Both work.' });
    expect(headOf(undefined, 'Sign in to a.dev')).toEqual({ head: 'Sign in to a.dev', lead: '' });
  });
  it('browserapp_12: Type the password opens the password card; Fill in fills the page and the card goes', () => {
    act(() => root.render(<BrowserProvider board={7}><BrowserAsks /><BrowserPane width={640} /></BrowserProvider>));
    expect(text('.tc-ask-head')).toBe('Sign in to the staging admin · Codex');
    press('Type the password');
    expect(text('.br-control-text')).toBe('You’re filling the password. Codex waits.');
    const input = host.querySelector('[data-browser="secret-input"]') as HTMLInputElement;
    act(() => { const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!; set.call(input, 'hunter22'); input.dispatchEvent(new Event('input', { bubbles: true })); });
    press('Fill in');
    expect(host.querySelector('[data-browser="asks"]')).toBeNull();
    expect(text('[data-browser="page-password"]')).toBe('••••••••From Oby');
  });
  it('browserapp_13: every ask shows one at a time; Skip answers the one shown', () => {
    act(() => root.render(<BrowserProvider board={9}><BrowserAsks /></BrowserProvider>));
    expect(text('.tc-ask-kind')).toBe('Check');
    act(() => (host.querySelector('[aria-label="Next decision"]') as HTMLButtonElement).click());
    expect(text('.tc-ask-head')).toBe('Which plan should I pick at checkout? · Claude Code');
    expect([...host.querySelectorAll('.tc-option')].map((b) => b.textContent)).toEqual(['Team · $12', 'Business · $29', 'I’ll choose in the page']);
    press('Team · $12');
    expect(text('.decision-count')).toBe('2 of 3');
  });
});
