// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// A login another profile holds is named in Settings › Accounts (docs/contracts/profiles.md, PROFILES-50):
// an account is in one profile only (B00-40), so this profile is told where the Mac's own login is
// and that Add account signs in another.

vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));
const state = vi.hoisted(() => ({ status: null as unknown }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: state.status, repos: [] }) }));

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

async function agents(otherProfileLogins: unknown[]) {
  state.status = { accounts: [], availableLogins: [], otherProfileLogins, models: {}, defaultAccount: null, talkAccount: null };
  const settings = await import('../SettingsView');
  settings.replaceAccounts([]);
  await act(async () => root.render(<settings.Agents />));
}
const SAID = 'Claude Code’s login on this Mac is in Default. Add another account to sign in with a different login here.';

describe('PROFILES-50: a login another profile holds is named', () => {
  it('profiles_50_a_login_another_profile_holds_is_named', async () => {
    await agents([{ agent: 'claude', id: 'default', profile: 'a1b2c3d4-0000-4000-8000-000000000001', profileName: 'Default' }]);
    await act(async () => { await vi.waitFor(() => expect(host.textContent).toContain(SAID)); });
  });

  it('profiles_50_none_held_elsewhere_says_nothing', async () => {
    await agents([]);
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    expect(host.textContent).not.toContain('login on this Mac is in');
  });
});
