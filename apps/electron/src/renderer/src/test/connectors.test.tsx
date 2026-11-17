// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

// Settings › Connectors on obid shows the owner's own Claude Code connectors after GitHub and
// Tetrix (docs/contracts/connectors.md, CONNECTORS-4 and 5).

vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: null, repos: [] }) }));

const GITHUB = { gh: true, signedIn: true, user: 'someone', signIn: null };
const TETRIX = { connected: false, url: null, preset: null, presets: [], connectedAt: null, signIn: null, error: null };
const SEEN = {
  claude: {
    seenAt: '2026-11-17T15:00:00Z',
    servers: [
      { name: 'claude.ai Slack', status: 'connected' },
      { name: 'claude.ai Gmail', status: 'needs-auth' },
      { name: 'rea', status: 'failed' },
    ],
  },
};

const answers = vi.hoisted(() => ({ connectors: null as null | { status: number; body: unknown } }));
const call = vi.fn(async (method: string, path: string) => {
  const body = method !== 'GET' ? null
    : path === '/v1/github' ? GITHUB
    : path === '/v1/tetrix' ? TETRIX
    : path === '/v1/connectors' && answers.connectors ? answers.connectors.body : null;
  if (path === '/v1/connectors' && answers.connectors) return { status: answers.connectors.status, text: JSON.stringify(answers.connectors.body) };
  return body ? { status: 200, text: JSON.stringify(body) } : { status: 404, text: '{"error":"There\'s no such endpoint."}' };
});
const openExternal = vi.fn();

let root: Root, host: HTMLDivElement;
// the Settings page's modules transformed once, ahead: on a busy Mac the first import alone can take
// longer than a test may
beforeAll(async () => {
  (window as unknown as { app: unknown }).app = { obid: { call }, openExternal };
  await import('../Connectors');
}, 180_000);
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  (window as unknown as { app: unknown }).app = { obid: { call }, openExternal };
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); openExternal.mockReset(); });

async function page() {
  const { default: Connectors } = await import('../Connectors');
  await act(async () => root.render(<Connectors />));
  // its reads answer, then it shows them: waited for, not timed (a busy Mac answers late)
  for (let i = 0; i < 800 && !(host.querySelector('.set-connector') && call.mock.calls.some(([, path]) => path === '/v1/connectors')); i++) {
    await act(async () => { await new Promise((r) => setTimeout(r, 25)); });
  }
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}
const group = (title: string) => Array.from(host.querySelectorAll('section.set-group')).find((s) => s.querySelector('h2')?.textContent === title);
const rowOf = (name: string) => Array.from(host.querySelectorAll('.set-label')).find((l) => l.firstChild?.textContent === name)?.closest('.set-row');

describe('CONNECTORS-4: Settings › Connectors lists the owner’s own Claude Code connectors', () => {
  it('connectors_4_settings_lists_the_owners_claude_code_connectors', async () => {
    answers.connectors = { status: 200, body: SEEN };
    await page();
    const connected = group('Connected')?.textContent ?? '';
    expect(connected).toContain('GitHub');
    expect(connected).toContain('Slack');
    expect(connected).not.toContain('claude.ai Slack');
    const available = group('Available')?.textContent ?? '';
    expect(available).toContain('Tetrix');
    expect(available).toContain('Gmail');
    expect(available).toContain('rea');
    // GitHub and Tetrix first, as before
    const names = Array.from(host.querySelectorAll('.set-label')).map((l) => l.firstChild?.textContent);
    expect(names.indexOf('GitHub')).toBeLessThan(names.indexOf('Slack'));
    expect(names.indexOf('Tetrix')).toBeLessThan(names.indexOf('Gmail'));
    // a claude.ai one that needs its sign-in: Connect opens claude.ai's connector settings
    const gmail = rowOf('Gmail');
    const connect = Array.from(gmail?.querySelectorAll('button') ?? []).find((b) => b.textContent === 'Connect');
    expect(connect).toBeTruthy();
    await act(async () => connect!.click());
    expect(openExternal).toHaveBeenCalledWith('https://claude.ai/settings/connectors');
    // a server of the owner's that failed says so, with no button
    expect(rowOf('rea')?.textContent).toContain('Couldn’t connect');
    expect(rowOf('rea')?.querySelector('button')).toBeNull();
  });
});

describe('CONNECTORS-5: none seen yet says when they show', () => {
  it('connectors_5_none_seen_says_when_they_show', async () => {
    answers.connectors = { status: 200, body: { claude: null } };
    await page();
    expect(host.textContent).toContain('GitHub');
    expect(host.textContent).toContain('Tetrix');
    expect(host.textContent).toContain('after your first conversation');
  });

  it('connectors_5_an_obid_without_the_route_shows_github_and_tetrix', async () => {
    answers.connectors = null;
    await page();
    expect(host.textContent).toContain('GitHub');
    expect(host.textContent).toContain('Tetrix');
    expect(host.textContent).toContain('after your first conversation');
  });
});
