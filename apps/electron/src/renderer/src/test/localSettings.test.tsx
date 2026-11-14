// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Settings rows obid has no setting for yet, kept by main (docs/contracts/live-state.md, LIVE-25
// and 26): the page's half, useSetting, with main's bridge (window.app.settings) stood in for.

type Local = Record<string, unknown> | null;
let root: Root | null = null;
let host: HTMLElement;

afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
});

// main's bridge, its kept rows answered when the test says; then SettingsView loaded fresh, as
// the app starts
async function start() {
  let answer!: (local: Local) => void;
  const local = new Promise<Local>((r) => { answer = r; });
  const keep = vi.fn(async (_key: string, _value: unknown) => true);
  (window as { app?: unknown }).app = { settings: { local: () => local, keep } };
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const view = await import('../SettingsView');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  return { view, keep, answer: async (l: Local) => { await act(async () => { answer(l); await local; }); } };
}

const general = async (view: typeof import('../SettingsView')) => {
  await act(async () => root!.render(<view.default page="general" leaving={false} onClose={() => {}} onGone={() => {}} />));
};
const login = () => host.querySelector<HTMLButtonElement>('[role="switch"][aria-label="Open at login"]')!;

describe('LIVE-25', () => {
  it('live_25_a_local_row_set_through_main_is_there_after_a_restart', async () => {
    const { view, answer } = await start();
    await general(view);
    // the default until main's answer comes
    expect(login().getAttribute('aria-checked')).toBe('true');
    await answer({ login: false });
    expect(login().getAttribute('aria-checked')).toBe('false');
  });

  it('live_25: a row turned before main\'s answer comes keeps the owner\'s turn, and is kept', async () => {
    const { view, keep, answer } = await start();
    await general(view);
    await act(async () => login().click());
    expect(login().getAttribute('aria-checked')).toBe('false');
    expect(keep).toHaveBeenLastCalledWith('login', false);
    await act(async () => login().click());
    expect(keep).toHaveBeenLastCalledWith('login', true);
    await answer({ login: false });
    expect(login().getAttribute('aria-checked')).toBe('true');
  });

  it('live_25: accounts is never kept, nor read from what main kept', async () => {
    const { view, keep, answer } = await start();
    let accounts: ReturnType<typeof view.useAccounts> | null = null;
    function Probe() { accounts = view.useAccounts(); return null; }
    await act(async () => root!.render(<Probe />));
    const shown = accounts![0];
    await answer({ accounts: [{ id: 'stale', agent: 'claude', name: 'Old', payer: 'Old', used: 0, resets: '', plan: 'personal', session: null, weekly: false }] });
    expect(accounts![0]).toBe(shown);
    await act(async () => accounts![1]([]));
    expect(accounts![0]).toEqual([]);
    expect(keep).not.toHaveBeenCalledWith('accounts', expect.anything());
    expect(keep).not.toHaveBeenCalled();
  });

  it('live_25: a refused or failed answer keeps nothing, and a failed keep leaves the row as set', async () => {
    const { view, keep, answer } = await start();
    keep.mockRejectedValueOnce(new Error('main is gone'));
    await general(view);
    await answer(null);
    expect(login().getAttribute('aria-checked')).toBe('true');
    await act(async () => login().click());
    expect(login().getAttribute('aria-checked')).toBe('false');
  });
});

describe('LIVE-26', () => {
  it('live_26_a_kept_value_of_the_wrong_type_shows_the_rows_default', async () => {
    const { view, answer } = await start();
    let rows: { login: boolean; chosen: string[]; names: string[] } | null = null;
    function Probe() {
      const [login] = view.useSetting('login', true);
      const [chosen] = view.useSetting<string[]>('share.chosen', []);
      const [names] = view.useSetting<string[]>('vault', ['a']);
      rows = { login, chosen, names };
      return null;
    }
    await act(async () => root!.render(<Probe />));
    await answer({ login: 'yes', 'share.chosen': 3, vault: { a: 1 } });
    expect(rows).toEqual({ login: true, chosen: [], names: ['a'] });
    // of the right type, it shows
    vi.resetModules();
    await act(async () => root!.unmount());
    const again = await start();
    function Again() { [rows!.login] = again.view.useSetting('login', true); [rows!.chosen] = again.view.useSetting<string[]>('share.chosen', []); return null; }
    await act(async () => root!.render(<Again />));
    await again.answer({ login: false, 'share.chosen': ['g1'] });
    expect(rows).toMatchObject({ login: false, chosen: ['g1'] });
  });
});

describe('no bridge', () => {
  it('live_25: in a browser, rows stay in memory as before', async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('location', { search: '' });
    const view = await import('../SettingsView');
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await general(view);
    await act(async () => login().click());
    expect(login().getAttribute('aria-checked')).toBe('false');
  });
});
