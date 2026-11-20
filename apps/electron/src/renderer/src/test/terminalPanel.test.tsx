// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Job } from '../../../shared/obid-wire';

// The terminal panel (docs/contracts/terminal.md, TERMINAL-9 to 12), rendered for real with main's
// half (window.app.terminal) and xterm.js stood in for: what the page sends main and draws.
vi.mock('../Menu', () => ({ Menu: ({ children }: { children: React.ReactNode }) => <div role="menu">{children}</div> }));
vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));

type Opened = { term: string; cwd: string; name: string } | { error: string };
// a screen as xterm.js would be: its size as the box gives it, what it was told to show
class Screen {
  static all: Screen[] = [];
  room: { cols: number; rows: number } | null = { cols: 100, rows: 30 };
  cols = 80; rows = 24; shown: string[] = []; typed: ((d: string) => void) | null = null; disposed = false; focused = 0;
  constructor() { Screen.all.push(this); }
  open() {}
  measure() { return this.room; }
  size() { return { cols: this.cols, rows: this.rows }; }
  resize(c: number, r: number) { this.cols = c; this.rows = r; }
  write(d: string) { this.shown.push(d); }
  onData(f: (d: string) => void) { this.typed = f; }
  focus() { this.focused++; }
  dispose() { this.disposed = true; }
}
// the box's size changes, as ResizeObserver reports them, a frame later
const observers: (() => void)[] = [];
class RO { constructor(private f: () => void) {} observe() { observers.push(this.f); } disconnect() {} }

let main: {
  open: ReturnType<typeof vi.fn<(place: string, id: string, cols: number, rows: number) => Promise<Opened>>>;
  write: ReturnType<typeof vi.fn>; resize: ReturnType<typeof vi.fn>; close: ReturnType<typeof vi.fn>;
  data: ((term: string, d: string) => void) | null; exit: ((term: string, code: number, signal: number | null) => void) | null;
};
let answers: Opened[];
let pending: ((a: Opened) => void)[];
let hold = false;
let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('ResizeObserver', RO);
  vi.stubGlobal('requestAnimationFrame', (f: () => void) => setTimeout(f, 0));
  vi.stubGlobal('cancelAnimationFrame', (t: number) => clearTimeout(t));
  Screen.all = []; observers.length = 0; answers = []; pending = []; hold = false;
  let n = 0;
  main = {
    open: vi.fn((place: string, id: string) => {
      const answer = answers.shift() ?? { term: `t${++n}`, cwd: `/obid/worktrees/${id}`, name: id };
      return hold ? new Promise<Opened>((r) => pending.push(() => r(answer))) : Promise.resolve(answer);
    }),
    write: vi.fn(), resize: vi.fn(), close: vi.fn(), data: null, exit: null,
  };
  (window as unknown as { app: unknown }).app = {
    obid: { call: async () => ({ status: 404, text: '' }), live: () => () => {} }, editorName: async () => 'Cursor', openWorktree: vi.fn(),
    terminal: {
      open: main.open, write: main.write, resize: main.resize, close: main.close,
      onData: (f: typeof main.data) => { main.data = f; return () => { main.data = null; }; },
      onExit: (f: typeof main.exit) => { main.exit = f; return () => { main.exit = null; }; },
    },
  };
  localStorage.clear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const resized = async () => { await act(async () => observers.forEach((f) => f())); await flush(); };
const button = (name: string) => Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent === name || b.getAttribute('aria-label') === name);
const panel = () => document.querySelector<HTMLElement>('.terminal-panel');
async function mount() {
  const { default: TerminalPanel, drawWith } = await import('../terminal/TerminalPanel');
  const store = await import('../terminal/store');
  drawWith(() => new Screen() as never);
  await act(async () => root.render(<TerminalPanel />));
  return store;
}

describe('TERMINAL-9: opening', () => {
  it('terminal_9_the_panel_opens_a_shell_at_the_size_its_box_holds_and_shows_and_sends_what_flows', async () => {
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    expect(panel()?.hidden).toBe(false);
    expect(panel()?.querySelector('h2')?.textContent).toBe('Terminal');
    expect(panel()?.querySelector('.terminal-place')?.textContent).toBe('Add hello · j1');
    // measured before main is asked: the shell's first prompt fits
    expect(main.open).toHaveBeenCalledExactlyOnceWith('task', 'j1', 100, 30);
    const s = Screen.all[0];
    main.data!('t1', '$ ');
    main.data!('t9', 'not mine');
    expect(s.shown).toEqual(['$ ']);
    s.typed!('l');
    expect(main.write).toHaveBeenCalledWith('t1', 'l');
    expect(s.focused).toBeGreaterThan(0);
  });

  it('terminal_9: main\'s refusal shows its words, and nothing is typed anywhere', async () => {
    answers = [{ error: 'Opening a terminal takes your own click.' }];
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    expect(panel()?.querySelector('[role="alert"]')?.textContent).toBe('Opening a terminal takes your own click.');
    Screen.all[0].typed!('l');
    expect(main.write).not.toHaveBeenCalled();
  });

  it('terminal_9: the task\'s menu offers Terminal beside Changes, with a worktree, and it opens the panel', async () => {
    const store = await mount();
    const live = await import('../live');
    const at = '2026-11-17T10:00:00Z';
    const j = {
      id: 'j1', request: 'Add hello', agent: 'claude', account: 'a1', repo: 'o/r', branch: 'oby/build-j1', base: 'main', issue: null, kind: 'build',
      status: 'done', step: 'Done', summary: '', created: at, finished: at, commits: 1, published: null, rounds: 1, round: 1,
      check: null, model: null, dismissed: null, permission: null, pipeline: null, title: 'Add hello', worktree: '/obid/worktrees/j1',
    } as unknown as Job;
    const s = live.liveState();
    s.jobs = { j1: j };
    const { default: TaskMenu } = await import('../TaskMenu');
    const menuHost = document.createElement('div'); document.body.append(menuHost);
    const menuRoot = createRoot(menuHost);
    await act(async () => menuRoot.render(<TaskMenu task={live.taskOf(s, j)} at={new DOMRect()} onClose={() => {}} />));
    await flush();
    const items = Array.from(document.querySelectorAll('[role="menuitem"]')).map((b) => b.textContent);
    expect(items.slice(items.indexOf('Changes'), items.indexOf('Changes') + 3)).toEqual(['Changes', 'Terminal', 'Open in Terminal']);
    await act(async () => button('Terminal')!.click());
    await flush();
    expect(store.terminalState().shown).toBe('task:j1');
    expect(main.open).toHaveBeenCalledWith('task', 'j1', 100, 30);
    await act(async () => menuRoot.unmount());
  });
});

describe('TERMINAL-10: sizes, as T3 Code\'s fixes taught', () => {
  it('terminal_10_a_resize_while_its_shell_opens_reaches_the_shell_once_it_is_there', async () => {
    hold = true;
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    const s = Screen.all[0];
    // the panel widens before main answers: nothing to tell yet
    s.room = { cols: 120, rows: 30 };
    await resized();
    expect(main.resize).not.toHaveBeenCalled();
    await act(async () => pending.shift()!({} as Opened));
    await flush();
    expect(main.resize).toHaveBeenCalledExactlyOnceWith('t1', 120, 30);
  });

  it('terminal_10: hidden, a terminal sends no size (its box has none); shown again, it sends the one it has', async () => {
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    const s = Screen.all[0];
    await act(async () => store.hideTerminal());
    expect(panel()?.hidden).toBe(true);
    s.room = null;
    await resized();
    expect(main.resize).not.toHaveBeenCalled();
    expect(s.size()).toEqual({ cols: 100, rows: 30 });
    s.room = { cols: 90, rows: 20 };
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await resized();
    expect(main.resize).toHaveBeenCalledExactlyOnceWith('t1', 90, 20);
    // the same size again: nothing more
    await resized();
    expect(main.resize).toHaveBeenCalledTimes(1);
  });
});

describe('TERMINAL-11: several, and closing', () => {
  it('terminal_11_plus_opens_another_and_each_tab_closes_its_own_shell_the_panels_x_closes_them_all', async () => {
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    await act(async () => button('New terminal')!.click());
    await flush();
    expect(main.open).toHaveBeenCalledTimes(2);
    expect(Array.from(document.querySelectorAll('[role="tab"]')).map((t) => t.textContent)).toEqual(['Terminal 1', 'Terminal 2']);
    await act(async () => button('Close Terminal 1')!.click());
    expect(main.close).toHaveBeenCalledWith('t1');
    expect(main.close).not.toHaveBeenCalledWith('t2');
    expect(Screen.all[0].disposed).toBe(true);
    await act(async () => button('New terminal')!.click());
    await flush();
    await act(async () => button('Close Terminal')!.click());
    expect(new Set(main.close.mock.calls.map((c) => c[0]))).toEqual(new Set(['t1', 't2', 't3']));
    expect(panel()).toBeNull();
  });

  it('terminal_11: closed while its shell opens, the shell main started ends at once', async () => {
    hold = true;
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await act(async () => button('Close Terminal')!.click());
    expect(main.close).not.toHaveBeenCalled();
    await act(async () => pending.shift()!({} as Opened));
    await flush();
    expect(main.close).toHaveBeenCalledExactlyOnceWith('t1');
    // opened again, the new one stays (a reopen while an open is pending doesn't close itself)
    hold = false;
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    expect(main.open).toHaveBeenCalledTimes(2);
    expect(main.close).toHaveBeenCalledTimes(1);
    expect(panel()?.hidden).toBe(false);
  });

  it('terminal_11: a shell that exits takes its tab with it; the last one closes the panel, and Terminal opens a new one', async () => {
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    await act(async () => main.exit!('t1', 0, null));
    expect(panel()).toBeNull();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    expect(main.open).toHaveBeenCalledTimes(2);
    expect(panel()?.hidden).toBe(false);
  });
});

describe('TERMINAL-12: one panel at the right at a time', () => {
  it('terminal_12_changes_opening_puts_the_terminal_away_its_shell_kept_and_terminal_brings_it_back', async () => {
    const store = await mount();
    const changes = await import('../ChangesPanel');
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    await act(async () => changes.openChanges('j1'));
    expect(panel()?.hidden).toBe(true);
    expect(main.close).not.toHaveBeenCalled();
    // the shell's output still reaches its screen while away
    main.data!('t1', 'still here');
    expect(Screen.all[0].shown).toEqual(['still here']);
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    expect(panel()?.hidden).toBe(false);
    expect(main.open).toHaveBeenCalledTimes(1);
  });

  it('terminal_12: another place\'s terminals are its own: shown from its menu, kept while another shows', async () => {
    const store = await mount();
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    await flush();
    await act(async () => store.openTerminal('conversation', 'c1', 'Plan'));
    await flush();
    expect(main.open.mock.calls.map((c) => c.slice(0, 2))).toEqual([['task', 'j1'], ['conversation', 'c1']]);
    expect(panel()?.querySelector('.terminal-place')?.textContent).toBe('Plan · c1');
    await act(async () => button('Close Terminal')!.click());
    expect(main.close.mock.calls).toEqual([['t2']]);
    await act(async () => store.openTerminal('task', 'j1', 'Add hello'));
    expect(main.open).toHaveBeenCalledTimes(2);
    expect(panel()?.querySelector('.terminal-place')?.textContent).toBe('Add hello · j1');
  });
});
