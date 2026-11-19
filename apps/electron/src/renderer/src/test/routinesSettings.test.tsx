// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Settings › Routines (docs/contracts/routines.md, ROUTINES-18 to 21): the real page, with main's
// bridge (window.app.obid.call) standing in for obid's routines.

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
});

const now = Date.now();
const at = (hours: number) => new Date(now + hours * 3_600_000).toISOString();
const routine = (more: Record<string, unknown>) => ({
  id: 'a1b2c3d4', name: 'Review open PRs', repo: 'obi-ai-bot/obi', kind: 'review', task: 'Review them.', agent: 'auto', account: 'auto',
  mode: null, check: null, rounds: 1, schedule: { every: 'weekdays', at: '09:00' }, on: null, zone: 'America/Sao_Paulo', when: 'Weekdays at 9:00',
  enabled: true, created: '2026-11-01T12:00:00Z', createdBy: 'chat', conversation: null, nextRun: at(20), failuresInARow: 0, graceMinutes: 1440,
  lastRun: { scheduled: at(-4), at: at(-4), outcome: 'started', reason: null, job: 'j1', catchUp: false, ended: 'done' }, ...more,
});
const REVIEW = routine({});
const NIGHTLY = routine({
  id: 'b2c3d4e5', name: 'Nightly check', repo: 'deskree-inc/oby', when: 'Every day at 2:00',
  lastRun: { scheduled: at(-30), at: at(-30), outcome: 'skipped', reason: 'The previous run was still going.', job: null, catchUp: false, ended: null },
});
const FAILING = routine({
  id: 'c3d4e5f6', name: 'Release notes', when: 'Fridays at 16:00', enabled: false, nextRun: null, failuresInARow: 3,
  lastRun: { scheduled: at(-50), at: at(-50), outcome: 'started', reason: null, job: 'j3', catchUp: false, ended: 'failed' },
});
const ON_GITHUB = routine({
  id: 'd4e5f6a7', name: 'Review requests', when: 'When your review is requested in obi-ai-bot/obi', schedule: null, nextRun: null, lastRun: null,
  on: { github: { repo: 'obi-ai-bot/obi', event: 'reviewRequested', label: null } },
});

type Answer = { status: number; text: string };
// obid as main's bridge reaches it: its routines, and what each change answers (refused: an answer)
function obid(list: Record<string, unknown>[], refuse: Record<string, Answer> = {}) {
  let routines = list;
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown): Promise<Answer> => {
    calls.push({ method, path, body });
    const refused = refuse[`${method} ${path}`];
    if (refused) return refused;
    if (method === 'GET' && path === '/v1/routines') return { status: 200, text: JSON.stringify(routines) };
    const id = path.split('/')[3];
    const r = routines.find((x) => x.id === id);
    if (!r) return { status: 404, text: '{"error":"No such routine."}' };
    if (method === 'PATCH') {
      const changed = { ...r, ...(body as object), nextRun: (body as { enabled: boolean }).enabled ? at(20) : null };
      routines = routines.map((x) => (x.id === id ? changed : x));
      return { status: 200, text: JSON.stringify(changed) };
    }
    if (method === 'POST' && path.endsWith('/run')) {
      const ran = { ...r, lastRun: { scheduled: at(0), at: at(0), outcome: 'started', reason: null, job: 'j9', catchUp: false, ended: null } };
      routines = routines.map((x) => (x.id === id ? ran : x));
      return { status: 201, text: JSON.stringify({ routine: ran, job: { id: 'j9' } }) };
    }
    if (method === 'DELETE') {
      routines = routines.filter((x) => x.id !== id);
      return { status: 204, text: '' };
    }
    return { status: 404, text: '{"error":"Not found."}' };
  });
  return { app: { obid: { call } }, calls };
}

async function show(app: object) {
  (window as { app?: unknown }).app = app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { default: RoutinesSettings } = await import('../RoutinesSettings');
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<RoutinesSettings />));
  await act(async () => {});
  return host;
}
const group = (host: HTMLElement, name: string) =>
  [...host.querySelectorAll('section.set-group')].find((g) => g.querySelector('.set-head')?.textContent === name) as HTMLElement;
const button = (el: HTMLElement, text: string) => [...el.querySelectorAll('button')].find((b) => b.textContent === text) as HTMLButtonElement | undefined;
const click = (b: HTMLElement | undefined) => act(async () => { b!.click(); });

describe('Settings › Routines', () => {
  it('ROUTINES-18: lists every routine with its schedule in words, its repository and next run, and how its last run went', async () => {
    const { app } = obid([REVIEW, NIGHTLY, FAILING, ON_GITHUB]);
    const host = await show(app);
    const review = group(host, 'Review open PRs');
    expect(review.textContent).toContain('Weekdays at 9:00');
    expect(review.textContent).toMatch(/obi-ai-bot\/obi · Next (today|tomorrow) at /);
    expect(review.textContent).toMatch(/Last runDone · started today at /);
    expect(group(host, 'Nightly check').textContent).toMatch(/Skipped (yesterday|today) at .*: The previous run was still going/);
    const failing = group(host, 'Release notes');
    expect(failing.textContent).toContain('obi-ai-bot/obi · Paused');
    expect(failing.querySelector('.set-problem')?.textContent).toMatch(/^Failed · started /);
    // one on GitHub events: no next run, no runs, and no Run now
    const github = group(host, 'Review requests');
    expect(github.textContent).toContain('Runs when its event comes');
    expect(github.textContent).not.toContain('Last run');
    expect(button(github, 'Run now')).toBeUndefined();
    // none: the page says so, and how one is made
    await act(async () => root!.unmount());
    root = null;
    vi.resetModules();
    const empty = await show(obid([]).app);
    expect(empty.textContent).toContain('No routines yet');
  });

  it('ROUTINES-19: Pause and Resume change only whether it runs (PATCH enabled)', async () => {
    const { app, calls } = obid([REVIEW]);
    const host = await show(app);
    await click(button(group(host, 'Review open PRs'), 'Pause'));
    expect(calls.at(-1)).toEqual({ method: 'PATCH', path: '/v1/routines/a1b2c3d4', body: { enabled: false } });
    expect(group(host, 'Review open PRs').textContent).toContain('obi-ai-bot/obi · Paused');
    await click(button(group(host, 'Review open PRs'), 'Resume'));
    expect(calls.at(-1)).toEqual({ method: 'PATCH', path: '/v1/routines/a1b2c3d4', body: { enabled: true } });
    expect(group(host, 'Review open PRs').textContent).toMatch(/Next (today|tomorrow) at /);
  });

  it('ROUTINES-20: Run now starts a run now, and obid’s refusal shows in its words', async () => {
    const { app, calls } = obid([REVIEW, NIGHTLY], { 'POST /v1/routines/b2c3d4e5/run': { status: 409, text: '{"error":"Its previous run is still going."}' } });
    const host = await show(app);
    await click(button(group(host, 'Review open PRs'), 'Run now'));
    expect(calls.at(-1)).toEqual({ method: 'POST', path: '/v1/routines/a1b2c3d4/run', body: undefined });
    expect(group(host, 'Review open PRs').textContent).toMatch(/Last runRunning · started today at /);
    await click(button(group(host, 'Nightly check'), 'Run now'));
    expect(group(host, 'Nightly check').querySelector('[role="alert"]')?.textContent).toBe('Its previous run is still going.');
  });

  it('ROUTINES-21: Delete asks first in the page; Cancel deletes nothing, Delete takes it away', async () => {
    const { app, calls } = obid([REVIEW, NIGHTLY]);
    const host = await show(app);
    const before = calls.length;
    await click(button(group(host, 'Review open PRs'), 'Delete'));
    // asked here, never by the window's confirm() (the app has none): nothing sent yet
    expect(calls.length).toBe(before);
    const asked = group(host, 'Review open PRs').querySelector('[role="group"]') as HTMLElement;
    expect(asked.getAttribute('aria-label')).toBe('Delete Review open PRs?');
    expect(asked.textContent).toContain('The tasks its runs started stay.');
    await click(button(asked, 'Cancel'));
    expect(calls.length).toBe(before);
    expect(button(group(host, 'Review open PRs'), 'Pause')).toBeDefined();
    await click(button(group(host, 'Review open PRs'), 'Delete'));
    await click(button(group(host, 'Review open PRs').querySelector('[role="group"]') as HTMLElement, 'Delete'));
    expect(calls.at(-1)).toEqual({ method: 'DELETE', path: '/v1/routines/a1b2c3d4', body: undefined });
    expect(group(host, 'Review open PRs')).toBeUndefined();
    expect(group(host, 'Nightly check')).toBeDefined();
  });

  it('ROUTINES-18: Settings lists Routines and Repositories among its pages', async () => {
    (window as { app?: unknown }).app = obid([]).app;
    const { SETTINGS_PAGES } = await import('../SettingsView');
    expect(SETTINGS_PAGES.map((p) => p.id)).toEqual(expect.arrayContaining(['routines', 'repositories']));
  });
});
