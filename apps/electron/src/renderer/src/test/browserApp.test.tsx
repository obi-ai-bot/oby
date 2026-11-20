// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { browserFit, CONVERSATION_MIN } from '../browser/fit';
import { PANEL_DEFAULT, PANEL_MIN, BrowserProvider, demoNumber } from '../browser/store';
import { BrowserGlobe, BrowserPane } from '../browser/BrowserPane';

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

describe('BROWSERAPP-7: not offered on obid until main follows obid\'s browser', () => {
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
  it('browserapp_7: on obid there is no globe and no panel', () => {
    draw(null);
    expect(host.querySelector('[data-browser="globe"]')).toBeNull();
    expect(host.querySelector('aside.br')).toBeNull();
  });
  it('browserapp_7: on the demo data at board 2, the globe and the panel show (the check above can fail)', () => {
    draw(2);
    expect(host.querySelector('[data-browser="globe"]')?.getAttribute('aria-label')).toBe('Hide the browser');
    expect(host.querySelector('aside.br .br-control-text')?.textContent).toBe('Codex is using this tab · Clicking Sign in');
  });
});
