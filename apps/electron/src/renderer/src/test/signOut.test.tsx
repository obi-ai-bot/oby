// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Status } from '../obid';

// Sign out (docs/contracts/accounts.md, ACCOUNTS-23 to 25) in Settings › Agents: an account obid
// signs in has Sign out beside Remove, which asks obid for the tool's own logout and keeps the row.

vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));
vi.mock('../lean', async (original) => ({ ...await original<typeof import('../lean')>(), onObid: true }));
const state = vi.hoisted(() => ({ status: null as unknown }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: state.status, repos: [] }), patchStatus: vi.fn() }));

const row = (id: string, agent: string, name: string, more: Record<string, unknown> = {}) =>
  ({ id, agent, name, signedIn: true, fiveHour: 0.37, sevenDay: 0.5, sevenDayResets: null, running: 0, ...more });
const tools = [
  { agent: 'claude', bin: 'claude', binSet: false, path: '/usr/local/bin/claude', version: '2.1.302' },
  { agent: 'codex', bin: '/opt/codex/bin/codex', binSet: true, path: '/opt/codex/bin/codex', version: '0.162.1' },
];
const call = vi.fn(async (method: string, path: string, body?: unknown) => {
  if (method === 'GET' && path === '/v1/status') return { status: 200, text: JSON.stringify(state.status) };
  if (method === 'POST' && path.endsWith('/sign-out')) return { status: 200, text: JSON.stringify(row('w', 'claude', 'Work', { signedIn: false })) };
  if (method === 'PUT' && path.startsWith('/v1/settings/agents/')) {
    const bin = (body as { bin: string | null }).bin;
    if (bin === '/nope') return { status: 422, text: JSON.stringify({ error: 'obid can’t find /nope to run as Claude Code.', reason: 'notFound' }) };
    return { status: 200, text: JSON.stringify({ agent: path.split('/').pop(), bin: bin ?? 'claude', binSet: bin !== null, path: bin ?? '/usr/local/bin/claude', version: '2.1.302' }) };
  }
  return { status: 404, text: '{}' };
});

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  call.mockClear();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  (window as unknown as { app: unknown }).app = { obid: { call } };
  localStorage.clear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

async function agents(accounts: unknown[]) {
  state.status = ({ accounts, availableLogins: [], models: {}, defaultAccount: null, talkAccount: null, tools }) as unknown as Status;
  const settings = await import('../SettingsView');
  settings.replaceAccounts(accounts.map((a) => settings.accountRow(a as never, Date.now())));
  await act(async () => root.render(<settings.Agents />));
}
const card = (title: string) => Array.from(host.querySelectorAll('section.set-group')).find((s) => s.querySelector('h2')?.textContent === title);
const button = (within: Element | undefined, words: string) => Array.from(within?.querySelectorAll('button') ?? []).find((b) => b.textContent === words);

describe('ACCOUNTS-23 to 25: Sign out in Settings › Agents', () => {
  it('accounts_23_sign_out_asks_obid_for_the_tools_own_logout_and_keeps_the_row', async () => {
    await agents([row('w', 'claude', 'Work')]);
    const out = button(card('Work'), 'Sign out');
    expect(out).toBeTruthy();
    await act(async () => out!.click());
    expect(call).toHaveBeenCalledWith('POST', '/v1/accounts/w/sign-out', undefined);
    expect(card('Work')).toBeTruthy();
  });

  it('accounts_24_the_tools_own_login_and_a_signed_out_account_have_no_sign_out', async () => {
    await agents([row('default', 'claude', 'Default'), row('w', 'claude', 'Work', { signedIn: false })]);
    expect(button(card('Default'), 'Sign out')).toBeUndefined();
    expect(button(card('Work'), 'Sign out')).toBeUndefined();
    expect(button(card('Work'), 'Remove')).toBeTruthy();
  });
});
