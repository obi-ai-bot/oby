// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Settings › Repositories' Setup command (docs/contracts/setup.md, SETUP-4): the real page and the
// real changeSetup (live.ts), with main's bridge standing in for obid. The repositories are what
// live state holds (GET /v1/repos).

const repos = vi.hoisted(() => ({ list: [] as unknown[] }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: null, repos: repos.list }) }));

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
});

const OBY = { repo: 'deskree-inc/oby', defaultBranch: 'dev', listed: true, check: null, noCheck: false, setup: 'npm ci' };
const OBI = { repo: 'obi-ai-bot/obi', defaultBranch: 'main', listed: true, check: null, noCheck: false, setup: null };
const GITHUBS = { repo: 'octo/elsewhere', defaultBranch: 'main', listed: false, check: null, noCheck: false, setup: null };

type Answer = { status: number; text: string };
function obid(refuse?: Answer) {
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const confirmed: string[] = [];
  const ask = (setup: string | null) => ({ id: 'confirm-1', kind: 'confirm', title: `Run \`${setup}\`?`, text: '', job: '', options: ['Confirm', 'Cancel'], confirm: { action: 'set_setup', target: OBI.repo, payload: { setup, was: null } } });
  const call = vi.fn(async (method: string, path: string, body?: unknown): Promise<Answer> => {
    calls.push({ method, path, body });
    if (method === 'POST' && path === '/v1/repos/setup') {
      if (refuse) return refuse;
      const { setup } = body as { setup: string | null };
      return { status: 200, text: JSON.stringify({ repo: OBI, ask: ask(setup) }) };
    }
    if (method === 'GET' && path === '/v1/repos') return { status: 200, text: JSON.stringify({ repos: [OBY, { ...OBI, setup: 'pnpm install' }] }) };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  // the Confirm, which main signs with this Mac's key for the click that made it
  const confirm = vi.fn(async (item: string): Promise<Answer> => {
    confirmed.push(item);
    return { status: 200, text: JSON.stringify({ ...ask('pnpm install'), done: '2026-11-19T12:00:00Z' }) };
  });
  return { app: { obid: { call, confirm } }, calls, confirmed };
}

async function show(app: object) {
  (window as { app?: unknown }).app = app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { default: RepositoriesSettings } = await import('../RepositoriesSettings');
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<RepositoriesSettings />));
  return host;
}
const group = (host: HTMLElement, name: string) =>
  [...host.querySelectorAll('section.set-group')].find((g) => g.querySelector('.set-head')?.textContent === name) as HTMLElement | undefined;
const button = (el: HTMLElement, text: string) => [...el.querySelectorAll('button')].find((b) => b.textContent === text) as HTMLButtonElement | undefined;
// what a person types, as React hears it
async function type(input: HTMLInputElement, text: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

describe('Settings › Repositories', () => {
  it('SETUP-4: each listed repository has its Setup command, with a hint that it runs on this Mac as the owner', async () => {
    repos.list = [OBY, OBI, GITHUBS];
    const host = await show(obid().app);
    const field = (name: string) => group(host, name)!.querySelector('input') as HTMLInputElement;
    expect(field('deskree-inc/oby').value).toBe('npm ci');
    expect(field('obi-ai-bot/obi').value).toBe('');
    expect(field('obi-ai-bot/obi').getAttribute('aria-label')).toBe('obi-ai-bot/obi setup command');
    // GitHub's repositories obid doesn't list as its own aren't here
    expect(group(host, 'octo/elsewhere')).toBeUndefined();
    expect(group(host, 'obi-ai-bot/obi')!.textContent).toContain('Oby runs it on this Mac, as your user, in each new task’s worktree before its agent starts.');
    // Save shows only for a change
    expect(button(group(host, 'obi-ai-bot/obi')!, 'Save')).toBeUndefined();
  });

  it('SETUP-4: Save asks obid for it and confirms that ask at once, for the same click; obid’s refusal shows', async () => {
    repos.list = [OBY, OBI];
    const { app, calls, confirmed } = obid();
    const host = await show(app);
    const obi = group(host, 'obi-ai-bot/obi')!;
    await type(obi.querySelector('input')!, ' pnpm install ');
    await act(async () => { button(obi, 'Save')!.click(); });
    expect(calls.find((c) => c.path === '/v1/repos/setup')).toEqual({ method: 'POST', path: '/v1/repos/setup', body: { repo: 'obi-ai-bot/obi', setup: 'pnpm install' } });
    expect(confirmed).toEqual(['confirm-1']);
    expect(calls.at(-1)).toEqual({ method: 'GET', path: '/v1/repos', body: undefined });

    // an empty field takes it out: null
    await act(async () => root!.unmount());
    root = null;
    vi.resetModules();
    const out = obid();
    const again = await show(out.app);
    const oby = group(again, 'deskree-inc/oby')!;
    await type(oby.querySelector('input')!, '');
    await act(async () => { button(oby, 'Save')!.click(); });
    expect(out.calls.find((c) => c.path === '/v1/repos/setup')?.body).toEqual({ repo: 'deskree-inc/oby', setup: null });

    // refused: obid's words under the field, and nothing confirmed
    await act(async () => root!.unmount());
    root = null;
    vi.resetModules();
    const no = obid({ status: 422, text: '{"error":"setup is one line of up to 200 characters, or null to take it out."}' });
    const refused = await show(no.app);
    const target = group(refused, 'obi-ai-bot/obi')!;
    await type(target.querySelector('input')!, 'make');
    await act(async () => { button(target, 'Save')!.click(); });
    expect(target.querySelector('[role="alert"]')?.textContent).toBe('setup is one line of up to 200 characters, or null to take it out.');
    expect(no.confirmed).toEqual([]);
  });
});
