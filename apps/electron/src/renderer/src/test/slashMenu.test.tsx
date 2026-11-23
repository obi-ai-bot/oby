// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { $createParagraphNode, $createTextNode, $getRoot, type LexicalEditor } from 'lexical';
import type { Chat } from '../chats';
import { liveBridge } from './liveBridge';

// `/`'s skills as Claude's own slash menu shows them (docs/contracts/composer.md, COMPOSER-25): a
// popover over the box, bounded and scrolling inside, the lit row kept in view, only its description
// shown, in a tip beside it; Esc and a click outside close it. The real Composer and its real
// message box (Lexical), keys and clicks sent as the page gets them; main's bridge stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));

let root: Root | null = null;
vi.setConfig({ testTimeout: 120_000 });
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
});

const at = '2026-11-23T10:00:00Z';
const STATUS = { runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities: ['composer-v1', 'folders-v1', 'skills-v1'] };
const convo = { id: 'c1', title: 'Skills', pinned: false, created: at, lastActivity: at, cursor: '', folders: [] };
// as many skills as the owner's plugins give: far more than fit over the box
const SKILLS = Array.from({ length: 40 }, (_, i) => ({ name: `skill-${String(i).padStart(2, '0')}`, description: `What skill ${i} does`, userInvocable: true, source: 'user' }));

async function box() {
  const call = vi.fn(async (method: string, path: string) => {
    if (method === 'GET' && path.startsWith('/v1/skills')) return { status: 200, text: JSON.stringify({ tool: 'claude', account: 'p', skills: SKILLS }) };
    if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  (window as { app?: unknown }).app = { obid: { call, ...bridge.obid }, onFullScreen: () => () => {} };
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const { default: Composer } = await import('../Composer');
  const live = await import('../live');
  live.connect();
  bridge.sync({ status: STATUS, conversations: [convo] });
  const talk: Chat = { id: 'c1', title: 'Skills', said: [], at: 0 };
  const host = document.body.appendChild(document.createElement('div'));
  const outside = document.body.appendChild(document.createElement('p'));
  root = createRoot(host);
  await act(async () => root!.render(<Composer label="Reply to Oby" talk={talk} conversation="c1" draft="conversation:c1" />));
  const input = host.querySelector<HTMLElement>('.composer-input')!;
  await act(async () => { input.focus(); });
  return { input, outside };
}
// what's typed, as the real box holds it, the cursor at its end
const type = (input: HTMLElement, text: string) => act(async () => {
  const editor = (input as unknown as { __lexicalEditor: LexicalEditor }).__lexicalEditor;
  editor.update(() => { const root = $getRoot(); root.clear(); const t = $createTextNode(text); root.append($createParagraphNode().append(t)); t.select(); }, { discrete: true });
});
const key = (input: HTMLElement, k: string) => act(async () => { input.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true })); });
const panel = () => document.querySelector<HTMLElement>('[role="listbox"][aria-label="Skills"]');
const rows = () => [...(panel()?.querySelectorAll<HTMLElement>('[role="option"]') ?? [])];
const lit = () => rows().findIndex((r) => r.getAttribute('aria-selected') === 'true');
const tip = () => document.querySelector('.skill-tip')?.textContent ?? null;
async function opened(input: HTMLElement) {
  await type(input, '/');
  await vi.waitFor(() => expect(rows().length).toBe(40));
}

describe('COMPOSER-25: the slash menu, as Claude’s own', () => {
  it('composer_25_the_list_is_bounded_and_scrolls_with_the_lit_row_in_view', async () => {
    const seen = vi.spyOn(Element.prototype, 'scrollIntoView');
    const { input } = await box();
    await opened(input);
    // a popover of its own height, scrolling inside (app.css .skill-panel), never past the window
    expect(panel()!.classList).toContain('skill-panel');
    const height = parseFloat(panel()!.style.maxHeight);
    expect(height).toBeGreaterThan(0);
    expect(height).toBeLessThanOrEqual(392);
    // the first row is the list's first, lit, and its names alone
    expect(lit()).toBe(0);
    expect(rows()[0].textContent).toBe('skill-00');
    // ↓ past what shows: the lit row is brought into view each time
    for (let i = 0; i < 20; i++) await key(input, 'ArrowDown');
    expect(lit()).toBe(20);
    expect(seen.mock.contexts.at(-1)).toBe(rows()[20]);
    // ↑ from the first goes round to the last, which comes into view too
    for (let i = 0; i < 21; i++) await key(input, 'ArrowUp');
    expect(lit()).toBe(39);
    expect(seen.mock.contexts.at(-1)).toBe(rows()[39]);
    await key(input, 'ArrowDown');
    expect(seen.mock.contexts.at(-1)).toBe(rows()[0]);
  });

  it('composer_25_only_the_lit_rows_description_shows_beside_it', async () => {
    const { input } = await box();
    await opened(input);
    expect(rows().every((r) => !r.textContent!.includes('What skill'))).toBe(true);
    expect(tip()).toBe('What skill 0 does');
    await key(input, 'ArrowDown');
    expect(tip()).toBe('What skill 1 does');
    expect(document.querySelectorAll('.skill-tip')).toHaveLength(1);
    // after a lone /, what to do next
    expect(document.querySelector('.composer-hint')?.textContent).toBe('/Type to filter');
    await type(input, '/skill-3');
    expect(document.querySelector('.composer-hint')).toBeNull();
    expect(rows().map((r) => r.textContent).slice(0, 2)).toEqual(['skill-30', 'skill-31']);
  });

  it('composer_25_esc_closes_it_leaving_the_words_and_the_focus_in_the_box', async () => {
    const { input } = await box();
    await opened(input);
    await type(input, '/skill-1');
    await key(input, 'Escape');
    expect(panel()).toBeNull();
    expect(tip()).toBeNull();
    expect(input.textContent).toBe('/skill-1');
    expect(document.activeElement).toBe(input);
  });

  it('composer_25_a_click_outside_closes_it_picking_nothing', async () => {
    const { input, outside } = await box();
    await opened(input);
    // a click on a row is the list's own: it stays until the row picks
    await act(async () => { rows()[3].dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); });
    expect(panel()).not.toBeNull();
    await act(async () => { outside.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); });
    expect(panel()).toBeNull();
    expect(input.textContent).toBe('/');
    expect(document.querySelector('.composer-skill')).toBeNull();
    // elsewhere in the box too
    await type(input, '');
    await opened(input);
    await act(async () => { input.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); });
    expect(panel()).toBeNull();
  });
});
