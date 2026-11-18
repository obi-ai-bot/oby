// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const state = vi.hoisted(() => ({
  chosen: 'p', saved: { p: 'claude-opus-5-5', w: 'claude-sonnet-5-5' } as Record<string, string>,
  tune: vi.fn(), choose: vi.fn(), setDefault: vi.fn(), look: vi.fn(), fail: false,
  cursorModels: [] as { id: string; name: string }[], groups: undefined as unknown,
  efforts: undefined as string[] | undefined, effort: {} as Record<string, string>,
}));
const LONG_CURSOR = Array.from({ length: 40 }, (_, i) => ({ id: `m-${i}`, name: `Model ${i}` }));
vi.mock('../SettingsView', async () => {
  const { useState } = await import('react');
  const { cursorPick, cursorCompose, EFFORT_NAMES } = await vi.importActual<typeof import('../SettingsView')>('../SettingsView');
  return {
    cursorPick, cursorCompose, EFFORT_NAMES,
    fullness: () => '',
    useAccounts: () => [[
      { id: 'p', agent: 'claude', name: 'Personal', used: 34, weekly: true, session: null, modelsRead: true },
      { id: 'w', agent: 'claude', name: 'Work', used: null, session: 12 },
      { id: 'x', agent: 'cursor', name: 'Cursor', used: null, models: state.cursorModels },
    ]],
    useTalkDefault: () => [state.chosen, state.setDefault],
    useModels: () => {
      const [, render] = useState(0);
      return [
        (a: { id: string; agent: string }) => ({ model: state.saved[a.id] ?? (a.agent === 'cursor' ? 'auto' : 'claude-opus-5-5'), effort: state.effort[a.id] }),
        { claude: { models: [{ id: 'claude-opus-5-5', name: 'Opus 5.5' }, { id: 'claude-sonnet-5-5', name: 'Sonnet 5.5' }], efforts: state.efforts }, cursor: { models: [{ id: 'auto', name: 'Auto' }], groups: state.groups } },
        async (id: string, change: { model?: string; effort?: string }) => {
          state.tune(id, change);
          if (state.fail) throw new Error('Could not save model');
          if (change.model) state.saved[id] = change.model;
          if (change.effort) state.effort[id] = change.effort;
          render((n) => n + 1);
        },
        { claude: { model: 'claude-opus-5-5' }, cursor: { model: 'auto' } },
      ];
    },
  };
});
vi.mock('../chats', () => ({
  useTalkSetting: () => ({ account: state.chosen, next: 'p' }),
  chooseTalk: state.choose, lookAtUsage: () => state.look(),
}));

// app.css's rules, comments out, by selector: a rule's words can be checked where the DOM has no layout
const css = readFileSync(join(__dirname, '../app.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
const rule = (selector: string) => Array.from(css.matchAll(/([^{}]+)\{([^{}]*)\}/g)).filter((m) => m[1].split(',').some((x) => x.trim() === selector)).map((m) => m[2]);
// what a browser would lay out: the menu 380 wide at (500, 300), its rows 46 apart from 308 down
const stubRects = () => vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (this: Element) {
  if (this.classList.contains('row-menu') && !this.classList.contains('row-submenu')) return new DOMRect(500, 300, 380, 340);
  if (this.classList.contains('talk-choice')) return new DOMRect(508, 308 + Array.from(document.querySelectorAll('.talk-menu .talk-choice')).indexOf(this) * 46, 364, 44);
  return new DOMRect(0, 0, 0, 0);
});
const type = async (input: HTMLInputElement, text: string) => {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
};

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1024 });
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 768 });
  state.look.mockClear();
  state.chosen = 'p'; state.saved = { p: 'claude-opus-5-5', w: 'claude-sonnet-5-5' }; state.fail = false;
  state.cursorModels = LONG_CURSOR; state.groups = undefined; state.efforts = undefined; state.effort = {};
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
  const rects = stubRects();
  await show();
  expect(button('Personal').textContent).toContain('Opus 5.5 · 34% used this week');
  expect(button('Personal').getAttribute('aria-checked')).toBe('true');
  expect(button('Work').textContent).toContain('Sonnet 5.5 · 12% used in 5h');
  await click(button('Work'));
  const models = document.querySelector('[aria-label="Claude · Work models"]')!;
  expect(models.className).toContain('is-left');  // the menu's at the right of the window, so its models go on its left
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
  rects.mockRestore();
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
  // as tall as its models up to what the window has, its own list scrolling inside it
  expect(models.className).not.toContain('is-scroll');
  expect(Number.parseFloat(models.style.height)).toBeLessThanOrEqual(420 - 16);
  expect(models.querySelector('.model-list')).not.toBeNull();
  expect(models.querySelectorAll('button')).toHaveLength(40);
  expect(button('Model 39', models)).toBeTruthy();
  expect(button('Auto', models)).toBeUndefined();
});

it('CONVERSATION-AGENT-22: a menu too tall for the window scrolls its accounts, and an account\'s models still open beside it', async () => {
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 300 });
  await show();
  const menu = document.querySelector<HTMLElement>('.row-menu')!;
  expect(Number.parseFloat(menu.style.maxHeight)).toBeLessThanOrEqual(300);
  // the whole menu scrolling would clip the models beside it
  expect(menu.className).not.toContain('is-scroll');
  const accounts = menu.querySelector<HTMLElement>('.talk-menu')!;
  expect(accounts.className).toContain('menu-scroll');
  await click(button('Cursor'));
  const models = document.querySelector<HTMLElement>('[aria-label="Cursor models"]')!;
  expect(models.parentElement).toBe(menu);
  expect(models.closest('.menu-scroll')).toBeNull();
  // it follows its row as the accounts scroll
  const top = Number.parseFloat(models.style.top);
  accounts.scrollTop = 40;
  await act(async () => accounts.dispatchEvent(new Event('scroll')));
  expect(accounts.scrollTop).toBe(40);
  expect(Number.parseFloat(models.style.top)).toBe(top - 40);
  await click(button('Model 39', models));
  expect(state.tune).toHaveBeenCalledWith('x', { model: 'm-39' });
});

it('AGENTS-11: Cursor\'s models show once each, and a pick keeps the effort and Fast the account runs', async () => {
  state.cursorModels = [{ id: 'auto', name: 'Auto' }, { id: 'grok-4.7-low', name: 'Grok 4.7 Low' }, { id: 'grok-4.7-low-fast', name: 'Grok 4.7 Low Fast' },
    { id: 'gemini-3.1-pro', name: 'Gemini 3.1 Pro' }];
  state.groups = [{ base: 'auto', name: 'Auto', default: 'auto', variants: [{ id: 'auto', fast: false }] },
    { base: 'grok-4.7', name: 'Grok 4.7', default: 'grok-4.7-medium', variants: [{ id: 'grok-4.7-low', effort: 'low', fast: false }, { id: 'grok-4.7-low-fast', effort: 'low', fast: true }, { id: 'grok-4.7-medium', effort: 'medium', fast: false }] },
    { base: 'gemini-3.1-pro', name: 'Gemini 3.1 Pro', default: 'gemini-3.1-pro', variants: [{ id: 'gemini-3.1-pro', fast: false }] }];
  state.saved.x = 'grok-4.7-low-fast';
  await show();
  expect(button('Cursor').textContent).toContain('Grok 4.7 Low Fast');
  await click(button('Cursor'));
  const models = document.querySelector('[aria-label="Cursor models"]')!;
  expect(Array.from(models.querySelectorAll('button'), (b) => b.textContent)).toEqual(['Auto', 'Grok 4.7', 'Gemini 3.1 Pro']);
  expect(button('Grok', models).getAttribute('aria-checked')).toBe('true');
  await click(button('Grok', models));
  expect(state.tune).toHaveBeenLastCalledWith('x', { model: 'grok-4.7-low-fast' });
  state.saved.x = 'gemini-3.1-pro';
  await click(host.querySelector('button')!); await click(button('Cursor', document.querySelector('.talk-menu')!));
  // this account doesn't list Grok's default (medium): the closest id it lists
  await click(button('Grok', document.querySelector('[aria-label="Cursor models"]')!));
  expect(state.tune).toHaveBeenLastCalledWith('x', { model: 'grok-4.7-low' });
});

// The global ring (`:focus-visible { outline: 1px solid; outline-offset: 2px }`) sits outside a row, and the
// menu's own scrolling box (.menu-scroll) clips it: on the first row (Auto) only its bottom edge showed,
// as a stray line above Claude. A row the keyboard is on is lit like a hover instead, as the + menu's are.
it('CONVERSATION-AGENT-27: a row the keyboard is on is lit like a hover, with no focus ring a menu\'s scrolling box could clip', async () => {
  await show();
  const auto = button('Auto');
  // what clips it: the first row sits straight in a scrolling box inside a .row-menu
  expect(auto.parentElement?.className).toContain('menu-scroll');
  expect(auto.closest('.row-menu')).not.toBeNull();
  const focused = rule('.row-menu button:focus-visible');
  expect(focused.length).toBe(1);
  expect(focused[0]).toMatch(/outline:\s*0\b/);
  // lit as the hover is
  const hover = rule('.row-menu button:hover')[0].match(/background:[^;]+/)![0];
  expect(focused[0]).toContain(hover);
  // a danger row keeps its own tint on focus as on hover
  expect(css).toMatch(/\.row-menu \.is-danger:is\(:hover, :focus-visible\)/);
});

it('CONVERSATION-AGENT-28: the menu keeps one width and place whichever model list opens beside it, and its subtitles stay on one line', async () => {
  const rects = stubRects();
  for (const width of [1024, 420]) {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: width });
    await show();
    const menu = document.querySelector<HTMLElement>('.talk-menu')!.closest<HTMLElement>('.row-menu')!;
    const open = menu.getAttribute('style');
    expect(menu.style.width).toBe('380px');   // a width, not a least, so its words never decide it
    for (const name of ['Personal', 'Work', 'Cursor']) {
      await click(button(name, menu.querySelector('.talk-menu')!));
      expect(document.querySelector('.row-submenu')).not.toBeNull();
      expect(menu.getAttribute('style')).toBe(open);
    }
    await click(host.querySelector('button')!);
  }
  // one line each, the full words as the row's title: no row asks for its words to wrap
  await show();
  for (const about of document.querySelectorAll<HTMLElement>('.talk-choice-about')) expect(about.style.whiteSpace).toBe('');
  expect(button('Personal').querySelector('.talk-choice-about')?.getAttribute('title')).toBe('Opus 5.5 · 34% used this week');
  const line = rule('.talk-choice-about').join('');
  expect(line).toMatch(/white-space:\s*nowrap/); expect(line).toMatch(/text-overflow:\s*ellipsis/); expect(line).toMatch(/overflow:\s*hidden/);
  rects.mockRestore();
});

it('CONVERSATION-AGENT-29: a model list is left-aligned rows of one line, the current one checked, level with its row and inside the window', async () => {
  const rects = stubRects();
  state.saved.x = 'm-3';
  const rowTop = { Personal: 308 + 1 * 46, Cursor: 308 + 3 * 46 };
  for (const height of [768, 300]) {
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: height });
    await show();
    for (const [name, label, count] of [['Personal', 'Claude · Personal models', 2], ['Cursor', 'Cursor models', 40]] as const) {
      await click(button(name, document.querySelector('.talk-menu')!));
      const models = document.querySelector<HTMLElement>(`[aria-label="${label}"]`)!;
      const rows = Array.from(models.querySelectorAll<HTMLElement>('button'));
      expect(rows).toHaveLength(count);
      for (const row of rows) {
        expect(row.className).toContain('model-row');
        // the whole name in its title, one line of it shown, the rest an ellipsis
        expect(row.getAttribute('title')).toBe(row.querySelector('.model-name')!.textContent);
      }
      // the current one carries the check, and only it
      expect(rows.filter((r) => r.querySelector('.menu-check')).map((r) => r.getAttribute('aria-checked'))).toEqual(['true']);
      const top = 300 + 1 + Number.parseFloat(models.style.top), h = Number.parseFloat(models.style.height), w = Number.parseFloat(models.style.width);
      expect(top).toBeGreaterThanOrEqual(8); expect(top + h).toBeLessThanOrEqual(height - 8);
      // level with its row (less its own padding above its first row), as far up as the window needs
      expect(top).toBe(Math.max(8, Math.min(rowTop[name] - 9, height - 8 - h)));
      expect(models.className).toContain('is-left');
      expect(500 - 4 - w).toBeGreaterThanOrEqual(8);   // beside the menu, in the window
    }
    await click(host.querySelector('button')!);
  }
  const rowCss = rule('.row-menu button.model-row').join('');
  expect(rowCss).toMatch(/justify-content:\s*flex-start/); expect(rowCss).toMatch(/text-align:\s*left/); expect(rowCss).toMatch(/height:\s*34px/);
  const name = rule('.model-name').join('');
  expect(name).toMatch(/white-space:\s*nowrap/); expect(name).toMatch(/text-overflow:\s*ellipsis/); expect(name).toMatch(/overflow:\s*hidden/);
  expect(rule('.model-list').join('')).toMatch(/overflow-y:\s*auto/);
  rects.mockRestore();
});

const NAMED = ['Claude Opus 5 1M', 'Claude Fable 5 1M Thinking', 'Claude Sonnet 5.5 Thinking', 'Codex 5.3', 'GPT 5.6 Sol', 'Grok 4.7', 'Composer 2'];
const named = (n: number) => NAMED.slice(0, n).map((name, i) => ({ id: `m${i}`, name }));
it('CONVERSATION-AGENT-30: a model list has a search from seven models, not at six', async () => {
  state.cursorModels = named(6); await show();
  await click(button('Cursor', document.querySelector('.talk-menu')!));
  expect(document.querySelector('[aria-label="Cursor models"] input')).toBeNull();
  expect(document.activeElement?.className).toContain('model-row');
  await click(host.querySelector('button')!);
  state.cursorModels = named(7);
  await click(host.querySelector('button')!);
  await click(button('Cursor', document.querySelector('.talk-menu')!));
  const search = document.querySelector<HTMLInputElement>('[aria-label="Cursor models"] input')!;
  expect(search).not.toBeNull();
  expect(document.activeElement).toBe(search);   // typing filters at once
  // another account's four models have none
  await click(button('Work', document.querySelector('.talk-menu')!));
  expect(document.querySelector('[aria-label="Claude · Work models"] input')).toBeNull();
});

it('CONVERSATION-AGENT-30: the search matches anywhere in a name, in any case, in a list that keeps its size and place', async () => {
  const rects = stubRects();
  state.cursorModels = named(7); await show();
  await click(button('Cursor', document.querySelector('.talk-menu')!));
  const models = document.querySelector<HTMLElement>('[aria-label="Cursor models"]')!;
  const search = models.querySelector<HTMLInputElement>('input')!;
  const shown = () => Array.from(models.querySelectorAll('.model-row'), (r) => r.textContent);
  const size = models.getAttribute('style');
  expect(shown()).toEqual(NAMED);
  await type(search, 'OPUS'); expect(shown()).toEqual(['Claude Opus 5 1M']);
  await type(search, 'think'); expect(shown()).toEqual(['Claude Fable 5 1M Thinking', 'Claude Sonnet 5.5 Thinking']);
  await type(search, '5.6'); expect(shown()).toEqual(['GPT 5.6 Sol']);
  await type(search, '  sol '); expect(shown()).toEqual(['GPT 5.6 Sol']);
  expect(models.getAttribute('style')).toBe(size);   // rows go, the list doesn't resize or move
  await type(search, 'nothing like it');
  expect(shown()).toEqual([]);
  expect(models.textContent).toContain('No models match');
  expect(models.getAttribute('style')).toBe(size);
  expect(document.activeElement).toBe(search);
  rects.mockRestore();
});

it('CONVERSATION-AGENT-30: arrows move through the filtered rows, Enter picks, Escape clears the search first and then closes', async () => {
  state.cursorModels = named(7); await show();
  await click(button('Cursor', document.querySelector('.talk-menu')!));
  const models = document.querySelector<HTMLElement>('[aria-label="Cursor models"]')!;
  const search = models.querySelector<HTMLInputElement>('input')!;
  await type(search, 'think');
  await key('ArrowDown'); expect(document.activeElement?.textContent).toBe('Claude Fable 5 1M Thinking');
  await key('ArrowDown'); expect(document.activeElement?.textContent).toBe('Claude Sonnet 5.5 Thinking');
  await key('ArrowUp'); await key('ArrowUp'); expect(document.activeElement).toBe(search);
  // Left moves the caret in the search, it doesn't close the list
  await key('ArrowLeft'); expect(document.querySelector('.row-submenu')).not.toBeNull();
  // Escape: the words first, the list next, then the menu
  await key('Escape');
  expect(search.value).toBe(''); expect(document.querySelector('.row-submenu')).not.toBeNull(); expect(document.activeElement).toBe(search);
  expect(models.querySelectorAll('.model-row')).toHaveLength(7);
  await key('Escape'); expect(document.querySelector('.row-submenu')).toBeNull(); expect(document.activeElement).toBe(button('Cursor'));
  await key('Escape'); expect(document.querySelector('[role="menu"]')).toBeNull();
  // Enter in the search picks the first of what's left
  await click(host.querySelector('button')!); await click(button('Cursor', document.querySelector('.talk-menu')!));
  await type(document.querySelector<HTMLInputElement>('[aria-label="Cursor models"] input')!, 'grok');
  await key('Enter');
  expect(state.tune).toHaveBeenLastCalledWith('x', { model: 'm5' });
  expect(state.setDefault).toHaveBeenCalledWith('x');
});

it('CONVERSATION-AGENT-31: opening the menu asks obid for usage only once it has opened, not while it does', async () => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  try {
    await show();
    expect(state.look).not.toHaveBeenCalled();
    await act(async () => { vi.advanceTimersByTime(300); });
    expect(state.look).toHaveBeenCalledTimes(1);
    // closed before it did: no call
    state.look.mockClear();
    await click(host.querySelector('button')!);   // closes
    await click(host.querySelector('button')!);   // opens
    await click(host.querySelector('button')!);   // closes
    await act(async () => { vi.advanceTimersByTime(300); });
    expect(state.look).not.toHaveBeenCalled();
  } finally { vi.useRealTimers(); }
});

it('COMPOSER-17: an account\'s efforts are under its models, its own checked, and a pick keeps it on the account', async () => {
  const rects = stubRects();
  state.efforts = ['low', 'medium', 'high', 'max'];
  state.effort = { w: 'medium' };
  await show();
  await click(button('Work'));
  const models = document.querySelector('[aria-label="Claude · Work models"]')!;
  const efforts = models.querySelector('[role="group"][aria-label="Effort"]')!;
  expect(Array.from(efforts.querySelectorAll('button')).map((b) => [b.textContent, b.getAttribute('aria-checked')])).toEqual([
    ['Low', 'false'], ['Medium', 'true'], ['High', 'false'], ['Max', 'false'],
  ]);
  // the list is tall enough for them: its models' rows, then the line, "Effort" and one row of pills
  expect((models as HTMLElement).style.height).toBe(`${2 * 9 + 2 * 34 + 2 + 13 + 22 + 28}px`);
  await click(button('High', efforts));
  expect(state.tune).toHaveBeenCalledWith('w', { effort: 'high' });
  expect(state.tune).not.toHaveBeenCalledWith('w', expect.objectContaining({ model: expect.anything() }));
  expect(state.setDefault).toHaveBeenCalledWith('w');
  expect(document.querySelector('[role="menu"]')).toBeNull();
  rects.mockRestore();
});

it('COMPOSER-17: Cursor\'s effort is in its model\'s name, so its list has no efforts', async () => {
  const rects = stubRects();
  state.efforts = ['low', 'high'];
  await show();
  await click(button('Cursor'));
  expect(document.querySelector('.row-submenu [aria-label="Effort"]')).toBeNull();
  rects.mockRestore();
});
