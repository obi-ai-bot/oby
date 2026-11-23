// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { InboxItem, Job } from '../obid';
import type { Pending } from '../DecisionBar';

// The drawer over the message box (docs/contracts/live-task.md, LIVETASK-24 and LIVETASK-26): Later
// takes a decision off this drawer only. obid's bridge (window.app.obid) is stood in for, so a call
// to it would show; the clock is vitest's.
const call = vi.fn();
let live: typeof import('../live');
let bar: typeof import('../DecisionBar');
let nested: typeof import('../nested');
beforeAll(async () => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date('2026-11-17T12:00:00Z'));
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call } } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  bar = await import('../DecisionBar');
  nested = await import('../nested');
}, 120_000);
afterAll(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

const at = (minutesAgo: number) => new Date(Date.now() - minutesAgo * 60_000).toISOString();
const job = (id: string, title: string, more: Partial<Job> = {}) =>
  ({
    id, repo: 'acme/oby-scratch', title, request: title, agent: 'claude', account: 'a1', branch: `oby/${id}`, base: 'main', issue: null, kind: 'work',
    status: 'done', step: '', summary: '', created: at(30), finished: null, commits: 1, published: 'https://github.com/acme/oby-scratch/pull/4',
    rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null, conversation: 'c1', ...more,
  }) as Job;
const item = (id: string, kind: string, jobId: string, more: Partial<InboxItem> = {}): InboxItem => ({
  id, kind, job: jobId, title: id, text: '', options: [], details: null, created: at(10), state: 'open', later: null, headline: null,
  recommended: null, triage: 'needs_you', ...more,
});
const merge = item('merge-j1', 'merge', 'j1', {
  title: 'Ready to merge: Divide', options: ['Merge', 'Leave it open'],
  // as obid writes it (merge.rs, MERGING-21): what it asks, then each hold on a line of its own
  text: 'Merge oby-scratch #4 into main?\nNo check ran on its head (this repository has none).\nIts base is main, which Oby merges into only on your word.',
});
// as obid writes it (sphere/inbox.rs, permission_item; docs/api/inbox.md): its title the task's, its text the permission's own
const permission = item('j2-permission-p1', 'permission', 'j2', { title: 'Run the tests', text: 'Run npm test', options: ['Allow once', 'Deny'] });

let root: Root | undefined;
let host: HTMLElement;
async function show(pending: Pending[]) {
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<bar.default pending={pending} />));
}
const close = async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; };
afterEach(async () => { await close(); call.mockReset(); });
const head = () => host.querySelector('.tc-ask-head')?.textContent;
const button = (label: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === label)!;

// both tasks of conversation c1 as the app reads them from obid
function tasks() {
  const s = live.liveState();
  s.jobs = { j1: job('j1', 'Divide'), j2: job('j2', 'Run the tests', { status: 'running', permission: { id: 'p1', tool: 'Bash', title: 'Run', detail: 'npm test', rule: null, always: null } }) };
  s.inbox = [merge, permission];
  s.answering = new Set();
  s.answered = {};
  return [live.taskOf(s, s.jobs.j1), live.taskOf(s, s.jobs.j2)];
}

describe('a postponed decision is still reachable', () => {
  it('LIVETASK-26: Later takes it off the drawer only: no obid call, the task still needs you, and opening the view again shows it', async () => {
    const [divide, run] = tasks();
    const pending = [divide, run].map((t) => ({ key: t.id, task: t, from: t.title }));
    await show(pending);
    expect(head()).toBe('Merge oby-scratch #4 into main? · Divide');
    expect(host.querySelector('.decision-count')!.textContent).toBe('1 of 2');
    // the merge card's holds are behind Details, not on the drawer's line
    expect(host.textContent).not.toContain('No check ran');
    await act(async () => button('Details').click());
    expect(host.textContent).toContain('No check ran on its head');

    await act(async () => button('Later').click());
    // the next one shows; the first is off this drawer, and nothing went to obid
    expect(head()).toBe('Let Claude Code run a command? · Run the tests');
    expect(host.querySelector('.decision-count')).toBeNull();
    expect(call).not.toHaveBeenCalled();
    // it is still the task's decision: its card, its nested row and its conversation's line say so
    const [still] = tasks();
    expect(still.status).toBe('needs');
    expect(still.answer).toBeUndefined();
    expect(nested.kidState(still)).toBe('needs');
    expect(nested.kidLine(still)).toBe('is ready to merge');
    expect(nested.kidsLine([still])).toBe('Claude Code is ready to merge');
    // and Waiting for you lists it
    expect(live.waitingOf(live.liveState()).map((i) => i.id)).toContain('merge-j1');

    // opened again (the task, or its conversation), it shows again
    await close();
    await show(pending);
    expect(head()).toBe('Merge oby-scratch #4 into main? · Divide');
    expect(host.querySelector('.decision-count')!.textContent).toBe('1 of 2');
    // the next one's arrow
    await act(async () => host.querySelector<HTMLButtonElement>('.decision-next')!.click());
    expect(head()).toBe('Let Claude Code run a command? · Run the tests');
    expect(host.querySelector('.decision-count')!.textContent).toBe('2 of 2');
    expect(call).not.toHaveBeenCalled();
  });

  it('LIVETASK-26: a decision put off in obid (another device, an earlier build) still waits with its buttons', async () => {
    const s = live.liveState();
    tasks();
    s.inbox = [{ ...merge, state: 'later', later: at(-60) }, permission];
    const divide = live.taskOf(s, s.jobs.j1);
    expect(divide.status).toBe('needs');
    await show([{ key: divide.id, task: divide }]);
    expect(head()).toBe('Merge oby-scratch #4 into main?');
    expect(button('Merge')).toBeTruthy();
    expect(button('Later')).toBeTruthy();
    expect(live.waitingOf(s).map((i) => i.id)).toContain('merge-j1');
  });
});
