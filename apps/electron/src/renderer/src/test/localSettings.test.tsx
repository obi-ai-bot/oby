// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Settings rows obid has no setting for yet, kept by main (docs/contracts/live-state.md, LIVE-25
// to 29): the page's half, useSetting, with main's bridge (window.app.settings, and Open at login's
// window.app.loginItem) stood in for.

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

// main's bridge, its kept rows answered when the test says, and the app's login item when given;
// then SettingsView loaded fresh, as the app starts
async function start(loginItem?: (on?: boolean) => Promise<boolean | null>) {
  let answer!: (local: Local) => void;
  const local = new Promise<Local>((r) => { answer = r; });
  const keep = vi.fn(async (_key: string, _value: unknown) => true);
  (window as { app?: unknown }).app = { settings: { local: () => local, keep }, ...(loginItem && { loginItem }) };
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

describe('WORDS-17', () => {
  it('words_17: Settings › Vault removes a secret by its own Remove Secret button, never a bare Remove', async () => {
    const { view, answer } = await start();
    await act(async () => root!.render(<view.default page="vault" leaving={false} onClose={() => {}} onGone={() => {}} />));
    await answer({ vault: ['staging-key'] });
    const labels = Array.from(host.querySelectorAll('button')).map((b) => b.textContent);
    expect(labels).toContain('Remove Secret');
    expect(labels).not.toContain('Remove');
  });
});

describe('LIVE-27', () => {
  it('live_27: turning the switch keeps the row and asks main to apply it', async () => {
    const loginItem = vi.fn(async (_on?: boolean) => true as boolean | null).mockResolvedValueOnce(true).mockResolvedValueOnce(false);
    const { view, keep, answer } = await start(loginItem);
    await general(view);
    await answer({});
    expect(loginItem).toHaveBeenCalledWith();
    expect(login().getAttribute('aria-checked')).toBe('true');
    await act(async () => login().click());
    expect(keep).toHaveBeenLastCalledWith('login', false);
    expect(loginItem).toHaveBeenLastCalledWith(false);
    expect(login().getAttribute('aria-checked')).toBe('false');
  });
});

describe('LIVE-29', () => {
  it('live_29_the_switch_shows_macos_state_when_it_differs_from_the_saved_value', async () => {
    const { view, answer } = await start(async () => false);
    await general(view);
    await answer({ login: true });
    expect(login().getAttribute('aria-checked')).toBe('false');
    // and the other way round
    await act(async () => root!.unmount());
    vi.resetModules();
    const again = await start(async () => true);
    root = createRoot(host);
    await general(again.view);
    await again.answer({ login: false });
    expect(login().getAttribute('aria-checked')).toBe('true');
  });

  it('live_29: a change macOS doesn\'t take goes back to what macOS says', async () => {
    const loginItem = vi.fn(async (_on?: boolean) => false as boolean | null);
    const { view, keep, answer } = await start(loginItem);
    await general(view);
    await answer({ login: false });
    await act(async () => login().click());
    expect(keep).toHaveBeenLastCalledWith('login', true);
    expect(loginItem).toHaveBeenLastCalledWith(true);
    expect(login().getAttribute('aria-checked')).toBe('false');
  });

  it('live_29: no answer from main shows the kept row', async () => {
    const loginItem = vi.fn(async (_on?: boolean) => null as boolean | null).mockRejectedValueOnce(new Error('main is gone'));
    const { view, answer } = await start(loginItem);
    await general(view);
    await answer({ login: false });
    expect(login().getAttribute('aria-checked')).toBe('false');
    // a null answer to a turn: the row as turned
    await act(async () => login().click());
    expect(loginItem).toHaveBeenLastCalledWith(true);
    expect(login().getAttribute('aria-checked')).toBe('true');
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

describe('ALERTS-18', () => {
  it('alerts_18_settings_notifications_shows_the_three_switches_under_alerts', async () => {
    // on obid, main's bridge for the alerts and their kinds (main/notify.ts)
    const kinds = vi.fn(async (change?: object) => ({ finished: true, merged: true, limit: true, ...change }));
    (window as { app?: unknown }).app = {
      obid: async () => ({ status: 503, text: '{}' }),
      settings: { local: async () => ({}), keep: async () => true },
      notifications: { alerts: async () => true, kinds, openSystemSettings: () => {} },
    };
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('location', { search: '' });
    const view = await import('../SettingsView');
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await general(view);
    const group = [...host.querySelectorAll('.set-label')].map((l) => l.textContent);
    const from = group.indexOf('Alerts');
    expect(group.slice(from, from + 5)).toEqual(['Alerts', 'Finished tasks', 'Merged pull requests', 'Accounts at their limit', 'Banners, sounds and Focus']);
    const merged = host.querySelector<HTMLButtonElement>('[role="switch"][aria-label="Merged pull requests"]')!;
    expect(merged.getAttribute('aria-checked')).toBe('true');
    await act(async () => merged.click());
    expect(kinds).toHaveBeenLastCalledWith({ merged: false });
    expect(merged.getAttribute('aria-checked')).toBe('false');
  });
});

describe('ALERTS-19', () => {
  it('alerts_19_the_banners_button_asks_main_for_the_settings_with_no_argument', async () => {
    // as contextBridge does: a call carrying the page's click event can't be cloned and throws
    const asked: unknown[][] = [];
    const openSystemSettings = vi.fn((...args: unknown[]) => {
      if (args.length) throw new Error('An object could not be cloned.');
      asked.push(args);
    });
    (window as { app?: unknown }).app = {
      obid: async () => ({ status: 503, text: '{}' }),
      settings: { local: async () => ({}), keep: async () => true },
      notifications: { alerts: async () => true, kinds: async () => ({ finished: true, merged: true, limit: true }), openSystemSettings },
    };
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('location', { search: '' });
    const view = await import('../SettingsView');
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await general(view);
    const button = [...host.querySelectorAll<HTMLButtonElement>('button.set-button')].find((b) => b.textContent === 'Open System Settings')!;
    await act(async () => button.click());
    expect(asked).toEqual([[]]);
  });
});
