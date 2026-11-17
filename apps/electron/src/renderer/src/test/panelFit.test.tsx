// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { CHAT_MIN, panelFit } from '../panelFit';

// CHATFIRST-35: the side panel never pushes the chat past the window's edge. It docks while the chat
// keeps T3 Code's 640px, giving way down to its narrowest, and floats over the chat in a narrower
// window (T3 Code's sheet). Home is rendered for real in a DOM, its heavy pieces as markers.
vi.mock('../ChatView', () => ({ default: ({ id }: { id: string }) => <div data-chat={id} />, ThreadTask: () => null, clock: () => '' }));
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));
vi.mock('../AskStack', () => ({ default: () => <section data-asks="" /> }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));
vi.mock('../Sidebar', () => ({ default: ({ open, width }: { open: boolean; width: number }) => <div className="sidebar-slot" data-open={open} data-width={width} /> }));

describe('how the side panel fits (panelFit)', () => {
  it('CHATFIRST-35: docked, the chat keeps 640px: the panel gives way down to its narrowest', () => {
    expect(CHAT_MIN).toBe(640);
    expect(panelFit(1512, 362, 252)).toEqual({ docked: true, width: 362 });
    expect(panelFit(994, 362, 252)).toEqual({ docked: true, width: 354 });
    expect(panelFit(892, 362, 252)).toEqual({ docked: true, width: 252 });
    for (const w of [892, 994, 1200, 1512]) {
      const fit = panelFit(w, 472, 252);
      expect(fit.width + CHAT_MIN, `${w}px`).toBeLessThanOrEqual(w);
    }
  });

  it('CHATFIRST-35: narrower than its narrowest beside 640px, it floats over the chat, never past the edge', () => {
    expect(panelFit(891, 362, 252).docked).toBe(false);
    expect(panelFit(700, 362, 252)).toEqual({ docked: false, width: 362 });
    // at the window's narrowest (420px) a strip of the chat still shows beside it
    expect(panelFit(420, 362, 252)).toEqual({ docked: false, width: 362 });
    expect(panelFit(420, 472, 252)).toEqual({ docked: false, width: 364 });
  });

  it('CHATFIRST-35: floating, it lies over the view (absolute), with a strip beside it that puts it away', () => {
    const css = readFileSync(join(__dirname, '..', 'app.css'), 'utf8');
    expect(css).toMatch(/\.app\.is-floating \.sidebar-slot \{ position: absolute;/);
    expect(css).toMatch(/\.panel-scrim \{ position: absolute;/);
  });
});

let root: Root | undefined;
let host: HTMLElement;
const kept: Record<string, string> = {};
beforeAll(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('localStorage', { getItem: (k: string) => kept[k] ?? null, setItem: (k: string, v: string) => { kept[k] = v; }, removeItem: (k: string) => { delete kept[k]; } });
});
afterAll(() => { vi.unstubAllGlobals(); });
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; for (const k of Object.keys(kept)) delete kept[k]; });
const width = (w: number) => Object.defineProperty(window, 'innerWidth', { configurable: true, value: w });
async function open(w: number) {
  vi.resetModules();
  width(w);
  history.replaceState(null, '', '/?everything&chat');
  const { default: Home } = await import('../Home');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Home />));
}
const app = () => host.querySelector('.app')!;
const toggle = () => host.querySelector<HTMLButtonElement>('.panel-toggle')!;

describe('the side panel in a narrow window (Home)', () => {
  it('CHATFIRST-35: at 700px it opens over the chat, the chat at the window\'s width, and a click beside it puts it away', async () => {
    await open(700);
    expect(app().className).toContain('is-floating');
    expect(app().className).not.toContain('has-sidebar');
    await act(async () => toggle().click());
    expect(app().className).toMatch(/has-sidebar.*is-floating/);
    expect(toggle().getAttribute('aria-label')).toBe('Hide sidebar');
    const shown = Number(host.querySelector('.sidebar-slot')!.getAttribute('data-width'));
    expect(shown).toBeLessThan(700);
    expect((app() as HTMLElement).style.getPropertyValue('--sidebar')).toBe(`${shown}px`);
    // floating isn't the docked choice: nothing is remembered
    expect(kept['oby.chatPanelOpen']).toBeUndefined();
    await act(async () => host.querySelector<HTMLButtonElement>('.panel-scrim')!.click());
    expect(app().className).not.toContain('has-sidebar');
    expect(host.querySelector('.panel-scrim')).toBeNull();
  });

  it('CHATFIRST-35: widened past 640px of chat beside it, it docks again as the owner left it, giving way to the chat', async () => {
    kept['oby.chatPanelOpen'] = '1';
    await open(1000);
    expect(app().className).toContain('has-sidebar');
    expect(app().className).not.toContain('is-floating');
    expect((app() as HTMLElement).style.getPropertyValue('--sidebar')).toBe('360px');
    // narrowed to 700: it steps aside, and the chat has the window
    width(700);
    await act(async () => { window.dispatchEvent(new Event('resize')); });
    expect(app().className).toContain('is-floating');
    expect(app().className).not.toContain('has-sidebar');
    // widened again: back, docked
    width(1000);
    await act(async () => { window.dispatchEvent(new Event('resize')); });
    expect(app().className).toContain('has-sidebar');
    expect(app().className).not.toContain('is-floating');
  });
});
