// @vitest-environment happy-dom
import { act, useEffect } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// First run is never a dead end, and promises only what's built (docs/contracts/first-run.md,
// FIRSTRUN-1 to 4): the real FirstRun, its rows from obid's status (stood in for), main's bridge
// (window.app.obid.call) as obid.

vi.mock('../Oby', () => ({ default: ({ onDrawn }: { onDrawn?: () => void }) => { useEffect(() => onDrawn?.(), []); return null; } }));
const state = vi.hoisted(() => ({ status: null as unknown }));
vi.mock('../live', async (original) => ({
  ...await original<typeof import('../live')>(),
  useLive: () => ({ status: state.status, repos: [], problem: null, why: null }),
  liveState: () => ({ status: state.status, repos: [] }),
}));

const tool = (agent: string, found: boolean) => ({ agent, path: found ? `/opt/homebrew/bin/${agent}` : null, version: found ? '1.0.0' : null });
const statusWith = (tools: unknown, accounts: unknown[] = []) =>
  ({ accounts, availableLogins: [], models: {}, capabilities: [], ...(tools ? { tools } : {}) });
// what obid answers to the app's next look
let next: unknown = null;
const call = vi.fn(async (method: string, path: string) =>
  method === 'GET' && path === '/v1/status' ? { status: 200, text: JSON.stringify(next ?? state.status) } : { status: 404, text: '{"error":"Not found."}' });
const writeText = vi.fn(async () => {});

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  (window as unknown as { app: unknown }).app = { obid: { call }, openExternal: vi.fn() };
  localStorage.clear();
  next = null;
  writeText.mockClear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

// state that comes on a timer (a step's leaving, the app's next look) renders once each act ends: so
// a short act at a time, until the check holds
async function until(check: () => void, timeout = 2000) {
  for (const end = Date.now() + timeout; ; ) {
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    try { check(); return; } catch (e) { if (Date.now() > end) throw e; }
  }
}
const button = (text: string) => Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent === text || b.getAttribute('aria-label') === text);
const rowOf = (name: string) => Array.from(host.querySelectorAll('.setup-row')).find((r) => r.querySelector('.wl-name')?.textContent === name)!;
const rowButtons = (name: string) => Array.from(rowOf(name).querySelectorAll('button')).map((b) => b.textContent);
const words = () => host.querySelector('.onb-copy')?.textContent ?? '';

async function firstRun(at: 'tools' | 'knowledge' | 'sphere', tools: unknown, accounts: unknown[] = []) {
  state.status = statusWith(tools, accounts);
  vi.stubGlobal('location', { search: `?onboarding=${at}` });
  const { default: FirstRun } = await import('../FirstRun');
  await act(async () => root.render(<FirstRun />));
}
const ALL = [tool('claude', true), tool('codex', true), tool('cursor', true)];
const CLAUDE_IN = { id: 'default', agent: 'claude', name: 'Personal', signedIn: true };

describe('FIRSTRUN-1: Skip for now goes on without an agent', () => {
  it('firstrun_1_skip_goes_on_without_an_agent', async () => {
    await firstRun('tools', ALL);
    await until(() => expect(button('Skip for now')).toBeTruthy());
    expect(button('Skip for now')!.className).not.toContain('is-gone');
    await act(async () => button('Skip for now')!.click());
    await until(() => expect(words()).toContain('Connect your team’s knowledge system.'));
  });

  it('firstrun_1_with_an_agent_ready_skip_fades_and_continue_goes_on', async () => {
    await firstRun('tools', ALL, [CLAUDE_IN]);
    await until(() => expect(button('Skip for now')).toBeTruthy());
    const skip = button('Skip for now')!;
    expect(skip.className).toContain('is-gone');
    expect(skip.tabIndex).toBe(-1);
    await act(async () => button('Continue')!.click());
    await until(() => expect(words()).toContain('Connect your team’s knowledge system.'));
  });
});

describe('FIRSTRUN-2: an agent whose CLI isn’t found shows Install first', () => {
  it('firstrun_2_an_agent_not_installed_shows_install_first', async () => {
    await firstRun('tools', [tool('claude', true), tool('codex', false), tool('cursor', true)]);
    await until(() => expect(rowButtons('Codex')).toEqual(['Install']));
    expect(rowButtons('Claude Code')).toEqual(['Connect']);
    expect(rowButtons('Cursor')).toEqual(['Connect']);
    await act(async () => button('Install')!.click());
    const command = 'curl -fsSL https://chatgpt.com/codex/install.sh | sh';
    expect(rowOf('Codex').querySelector('code')?.textContent).toBe(command);
    expect(rowOf('Codex').textContent).toContain('Paste it in Terminal. Oby finds it once it’s installed.');
    await act(async () => button(`Copy Codex’s install command`)!.click());
    expect(writeText).toHaveBeenCalledWith(command);
  });

  it('firstrun_2_each_vendor_has_its_own_command', async () => {
    const { INSTALL } = await import('../install');
    expect(INSTALL).toEqual({
      claude: 'curl -fsSL https://claude.ai/install.sh | bash',
      codex: 'curl -fsSL https://chatgpt.com/codex/install.sh | sh',
      cursor: 'curl https://cursor.com/install -fsS | bash',
    });
  });
});

describe('FIRSTRUN-3: an install is found without a restart', () => {
  it('firstrun_3_an_install_is_found_without_a_restart', async () => {
    await firstRun('tools', [tool('claude', true), tool('codex', true), tool('cursor', false)]);
    await until(() => expect(rowButtons('Cursor')).toEqual(['Install']));
    next = statusWith(ALL);
    await until(() => expect(rowButtons('Cursor')).toEqual(['Connect']), 5000);
  });

  it('firstrun_3_an_obid_without_tools_shows_connect', async () => {
    await firstRun('tools', undefined);
    await until(() => expect(rowButtons('Cursor')).toEqual(['Connect']));
    expect(rowButtons('Claude Code')).toEqual(['Connect']);
    expect(rowButtons('Codex')).not.toContain('Install');
  });
});

describe('FIRSTRUN-4: first run promises only what is built', () => {
  it('firstrun_4_first_run_promises_only_what_is_built', async () => {
    await firstRun('tools', ALL);
    await until(() => expect(words()).toContain('Connect your coding agents and repositories.'));
    expect(words()).not.toMatch(/GitLab|Bitbucket/);
    await act(async () => root.unmount());
    root = createRoot(host);
    vi.resetModules();
    await firstRun('knowledge', ALL);
    await until(() => expect(words()).toContain('Connect your team’s knowledge system.'));
    expect(words()).not.toContain('MCP server');
    await act(async () => root.unmount());
    root = createRoot(host);
    vi.resetModules();
    await firstRun('sphere', ALL);
    await until(() => expect(words()).toContain('Your Oby can talk to other Obys.'));
    expect(words()).not.toContain('Create');
    expect(words()).toContain('invite link');
  });
});
