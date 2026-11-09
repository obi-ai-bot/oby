// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const state = vi.hoisted(() => ({
  chosen: 'p', saved: { p: 'claude-opus-5-5', w: 'claude-sonnet-5-5' } as Record<string, string>,
  tune: vi.fn(), choose: vi.fn(), setDefault: vi.fn(), fail: false,
}));
const LONG_CURSOR = Array.from({ length: 40 }, (_, i) => ({ id: `m-${i}`, name: `Model ${i}` }));
vi.mock('../SettingsView', async () => {
  const { useState } = await import('react');
  return {
    fullness: () => '',
    useAccounts: () => [[
      { id: 'p', agent: 'claude', name: 'Personal', used: 34, weekly: true, session: null, modelsRead: true },
      { id: 'w', agent: 'claude', name: 'Work', used: null, session: 12 },
      { id: 'x', agent: 'cursor', name: 'Cursor', used: null, models: LONG_CURSOR },
    ]],
    useTalkDefault: () => [state.chosen, state.setDefault],
    useModels: () => {
      const [, render] = useState(0);
      return [
        (a: { id: string; agent: string }) => ({ model: state.saved[a.id] ?? (a.agent === 'cursor' ? 'auto' : 'claude-opus-5-5') }),
        { claude: { models: [{ id: 'claude-opus-5-5', name: 'Opus 5.5' }, { id: 'claude-sonnet-5-5', name: 'Sonnet 5.5' }] }, cursor: { models: [{ id: 'auto', name: 'Auto' }] } },
        async (id: string, change: { model: string }) => {
          state.tune(id, change);
          if (state.fail) throw new Error('Could not save model');
          state.saved[id] = change.model;
          render((n) => n + 1);
        },
        { claude: { model: 'claude-opus-5-5' }, cursor: { model: 'auto' } },
      ];
    },
  };
});
vi.mock('../chats', () => ({
  useTalkSetting: () => ({ account: state.chosen, next: 'p' }),
  chooseTalk: state.choose, lookAtUsage: vi.fn(),
}));

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  state.chosen = 'p'; state.saved = { p: 'claude-opus-5-5', w: 'claude-sonnet-5-5' }; state.fail = false;
  state.tune.mockClear(); state.choose.mockClear(); state.setDefault.mockReset().mockImplementation(async (id: string) => { state.chosen = id; });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
const button = (text: string, within: ParentNode = document) => Array.from(within.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.includes(text))!;
const click = async (b: HTMLButtonElement) => { await act(async () => b.click()); };
const key = async (key: string) => { await act(async () => document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))); };
async function show(chat = false) {
  const { default: TalkPill } = await import('../TalkPill');
  await act(async () => root.render(<TalkPill chat={chat ? { id: 'chat', talkAccount: 'p' } as never : undefined} />));
  await click(host.querySelector('button')!);
}

it('CONVERSATION-AGENT-19: renders account models, a left submenu, checks, and the chosen model on the pill', async () => {
  await show();
  expect(button('Personal').textContent).toContain('Opus 5.5 · 34% used this week');
  expect(button('Personal').getAttribute('aria-checked')).toBe('true');
  expect(button('Work').textContent).toContain('Sonnet 5.5 · 12% used in 5h');
  await click(button('Work'));
  const models = document.querySelector('[aria-label="Claude · Work models"]')!;
  expect(models.className).toContain('is-left');
  expect(button('Sonnet', models).getAttribute('aria-checked')).toBe('true');
  await click(button('Opus', models));
  expect(state.tune).toHaveBeenCalledWith('w', { model: 'claude-opus-5-5' });
  expect(state.setDefault).toHaveBeenCalledWith('w');
  expect(state.saved.p).toBe('claude-opus-5-5');
  expect(document.querySelector('[role="menu"]')).toBeNull();
  expect(host.textContent).toContain('Claude Code · Opus 5.5');
  await click(host.querySelector('button')!);
  await click(button('Auto'));
  expect(state.setDefault).toHaveBeenLastCalledWith('auto');
  expect(document.querySelector('[role="menu"]')).toBeNull();
  expect(host.textContent).toContain('Auto');
});

it('CONVERSATION-AGENT-19: choosing a model in a conversation chooses its account', async () => {
  await show(true); await click(button('Work'));
  await click(button('Sonnet', document.querySelector('.row-submenu')!));
  expect(state.choose).toHaveBeenCalledWith('chat', 'w');
  expect(state.setDefault).not.toHaveBeenCalled();
});

it('CONVERSATION-AGENT-20: arrows navigate both rendered menus, Right opens, Left and Escape return focus', async () => {
  await show();
  expect(document.activeElement).toBe(button('Personal'));
  await key('ArrowUp'); expect(document.activeElement).toBe(button('Auto'));
  await key('ArrowRight'); expect(document.querySelector('.row-submenu')).toBeNull();
  await key('ArrowDown'); expect(document.activeElement).toBe(button('Personal'));
  await key('ArrowDown'); expect(document.activeElement).toBe(button('Work'));
  await key('ArrowRight'); expect(document.activeElement?.textContent).toBe('Sonnet 5.5');
  await key('ArrowDown'); expect(document.activeElement?.textContent).toBe('Opus 5.5');
  await key('ArrowUp'); expect(document.activeElement?.textContent).toBe('Sonnet 5.5');
  await key('ArrowLeft'); expect(document.querySelector('.row-submenu')).toBeNull(); expect(document.activeElement).toBe(button('Work'));
  await key('ArrowRight'); await key('Escape'); expect(document.querySelector('.row-submenu')).toBeNull(); expect(document.activeElement).toBe(button('Work'));
  await key('Escape'); expect(document.querySelector('[role="menu"]')).toBeNull(); expect(document.activeElement).toBe(host.querySelector('button'));
});

it('CONVERSATION-AGENT-20: a failed model save leaves the account unchanged and the menu open with its error', async () => {
  state.fail = true; await show(); await click(button('Work')); await click(button('Opus', document.querySelector('.row-submenu')!));
  expect(document.querySelector('[role="alert"]')?.textContent).toBe('Could not save model');
  expect(document.querySelector('.row-submenu')).not.toBeNull();
  expect(state.setDefault).not.toHaveBeenCalled(); expect(state.chosen).toBe('p');
});

it('CONVERSATION-AGENT-21: renders an unavailable model fallback and checks the effective default', async () => {
  state.saved.p = 'retired'; await show();
  expect(button('Personal').textContent).toContain('Opus 5.5 · 34% used this week');
  expect(button('Personal').textContent).toContain('Previous model unavailable · using default');
  expect(host.textContent).toContain('Claude Code · Opus 5.5');
  await click(button('Personal'));
  expect(button('Opus', document.querySelector('.row-submenu')!).getAttribute('aria-checked')).toBe('true');
  expect(document.querySelector('.row-submenu')?.textContent).not.toContain('retired');
});

it('CONVERSATION-AGENT-22: the model menu uses that account\'s list and scrolls when long', async () => {
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 420 });
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 420 });
  await show();
  await click(button('Cursor'));
  const models = document.querySelector<HTMLElement>('[aria-label="Cursor models"]')!;
  expect(models.className).toContain('is-scroll');
  expect(models.style.maxHeight).not.toBe('');
  expect(Number.parseFloat(models.style.maxHeight)).toBeLessThanOrEqual(420);
  expect(models.querySelectorAll('button')).toHaveLength(40);
  expect(button('Model 39', models)).toBeTruthy();
  expect(button('Auto', models)).toBeUndefined();
});
