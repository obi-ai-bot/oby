// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Status } from '../obid';

// Why an account can't run (docs/contracts/accounts.md, ACCOUNTS-21 and 22): Settings › Accounts says
// obid's reason on the account's row and doesn't show it as where new tasks go, and a failed task's
// line says obid's reason with no home folder in it.

vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));
const state = vi.hoisted(() => ({ status: null as unknown }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: state.status, repos: [] }) }));

const row = (id: string, agent: string, name: string, more: Record<string, unknown> = {}) =>
  ({ id, agent, name, signedIn: true, fiveHour: 0.37, sevenDay: null, sevenDayResets: null, running: 0, ...more });
const WHY = 'settings.json sets CLAUDE_CODE_X in its env';
const personal = row('default', 'claude', 'Personal', { cantRun: WHY });
const codex = row('cx', 'codex', 'Work');

const call = vi.fn(async (method: string, path: string) => (method === 'GET' && path === '/v1/status' ? { status: 200, text: JSON.stringify(state.status) } : { status: 404, text: '{}' }));

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  (window as unknown as { app: unknown }).app = { obid: { call } };
  localStorage.clear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

async function agents(accounts: unknown[], defaultAccount: string | null) {
  state.status = ({ accounts, availableLogins: [], models: {}, defaultAccount, talkAccount: null }) as unknown as Status;
  const settings = await import('../SettingsView');
  settings.replaceAccounts(accounts.map((a) => settings.accountRow(a as never, Date.now())));
  await act(async () => root.render(<settings.Agents />));
}
const card = (title: string) => Array.from(host.querySelectorAll('section.set-group')).find((s) => s.querySelector('h2')?.textContent === title);
const goesTo = () => host.querySelector<HTMLSelectElement>('select[aria-label="Default agent"]')?.value;

describe('ACCOUNTS-21: an account that can’t run says why in Settings › Accounts', () => {
  it('accounts_21_the_row_says_why_and_new_tasks_go_elsewhere', async () => {
    await agents([personal, codex], 'default');
    expect(card('Personal')?.textContent).toContain(`Claude Code can’t start: ${WHY}`);
    expect(goesTo()).toBe('cx');
    expect(card('Work')?.textContent).not.toContain('can’t start');
  });

  it('accounts_21_once_it_can_run_again_it_is_where_new_tasks_go', async () => {
    await agents([{ ...personal, cantRun: undefined }, codex], 'default');
    expect(card('Personal')?.textContent).not.toContain('can’t start');
    expect(goesTo()).toBe('default');
  });

  it('accounts_21_with_none_that_can_run_the_chosen_one_stays_shown', async () => {
    await agents([personal], 'default');
    expect(goesTo()).toBe('default');
    expect(card('Personal')?.textContent).toContain('can’t start');
  });
});

describe('ACCOUNTS-22: a failed task’s line has no home folder', () => {
  it('accounts_22_a_failed_line_says_the_reason_without_a_home_path', async () => {
    const { stoppedLine } = await import('../view');
    const failed = (reason: string) => stoppedLine({ status: 'failed', reason, step: '' } as never);
    expect(failed('~/.claude/settings.json sets X in its env, so it didn’t start.\nmore'))
      .toBe('settings.json sets X in its env, so it didn’t start.');
    expect(failed('obid can’t find Codex’s binary (/Users/someone/bin/codex).')).toBe('obid can’t find Codex’s binary (codex).');
    expect(failed('Two tests still fail after 2 rounds')).toBe('Two tests still fail after 2 rounds');
    expect(failed('Sign in again: run claude, then /login, at 50/50.')).toBe('Sign in again: run claude, then /login, at 50/50.');
    expect(failed('No login in ~/.codex, nor [oby]/accounts/a1/.')).toBe('No login in .codex, nor a1.');
  });
});
