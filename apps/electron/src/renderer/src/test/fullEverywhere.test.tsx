// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Status } from '../obid';

// Full on every account (docs/contracts/usage.md, USAGE-14): Settings › Agents says an account obid
// holds full is at its limit, and until when, whatever its tool, not only a ChatGPT login.

vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));
const state = vi.hoisted(() => ({ status: null as unknown }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: state.status, repos: [] }) }));

const row = (id: string, agent: string, name: string, more: Record<string, unknown> = {}) =>
  ({ id, agent, name, signedIn: true, fiveHour: 0.37, sevenDay: 0.5, sevenDayResets: null, running: 0, ...more });
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

async function agents(accounts: unknown[]) {
  state.status = ({ accounts, availableLogins: [], models: {}, defaultAccount: null, talkAccount: null }) as unknown as Status;
  const settings = await import('../SettingsView');
  settings.replaceAccounts(accounts.map((a) => settings.accountRow(a as never, Date.now())));
  await act(async () => root.render(<settings.Agents />));
}
const card = (title: string) => Array.from(host.querySelectorAll('section.set-group')).find((s) => s.querySelector('h2')?.textContent === title);
const soon = () => new Date(Date.now() + 3_600_000).toISOString();

describe('USAGE-14: every account obid holds full says so in Settings › Agents', () => {
  it('usage_14_a_claude_code_codex_or_cursor_account_held_full_says_limit_reached', async () => {
    await agents([row('default', 'claude', 'Personal', { fullUntil: soon() }), row('cx', 'codex', 'Work', { fullUntil: soon() }), row('cu', 'cursor', 'Editor', { fullUntil: soon() })]);
    for (const name of ['Personal', 'Work', 'Editor']) expect(card(name)?.textContent).toMatch(/Usage limit reached · until /);
  });

  it('usage_14_an_account_with_room_or_a_hold_that_ended_says_nothing_of_a_limit', async () => {
    await agents([row('default', 'claude', 'Personal'), row('cx', 'codex', 'Work', { fullUntil: new Date(Date.now() - 60_000).toISOString() })]);
    expect(card('Personal')?.textContent).not.toContain('Usage limit reached');
    expect(card('Work')?.textContent).not.toContain('Usage limit reached');
  });

  it('usage_14_a_signed_out_account_says_it_is_signed_out_not_full', async () => {
    await agents([row('default', 'claude', 'Personal', { signedIn: false, fullUntil: soon() })]);
    expect(card('Personal')?.textContent).not.toContain('Usage limit reached');
  });

  it('usage_14_a_chatgpt_login_keeps_its_own_line', async () => {
    await agents([row('cx', 'codex', 'Work', { login: 'chatgpt', fullUntil: soon() })]);
    expect(card('Work')?.textContent).toContain('Usage limit reached · Manage usage');
    expect(card('Work')?.textContent).not.toContain('· until');
  });

  it('usage_14_a_week_long_hold_names_its_day', async () => {
    const settings = await import('../SettingsView');
    const now = Date.parse('2026-11-17T12:00:00');
    const a = settings.accountRow(row('cx', 'codex', 'Work', { fullUntil: new Date(Date.parse('2026-11-20T09:00:00')).toISOString() }) as never, now);
    expect(a.full).toBe(true);
    expect(a.fullTill).toMatch(/Fri/);
  });
});
