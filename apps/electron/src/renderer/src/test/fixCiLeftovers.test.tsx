// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { InboxItem, Job } from '../obid';

// What Fix CI shows (docs/contracts/github-work.md, GITHUBWORK-10; docs/contracts/task-conversations.md,
// TASKCONV-32): Waiting for you's GitHub card starts its work when clicked, as Home's does, and a
// task's card names the branch its commits went to.
const call = vi.fn();
let live: typeof import('../live');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call }, openExternal: vi.fn() } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
}, 120_000);
afterAll(() => vi.unstubAllGlobals());

const OFFER = {
  id: 'github-ci-f1-abc', kind: 'github', job: '', title: 'CI failed: obi #812', text: 'Checks failed on your pull request #812 in obi-ai-bot/obi.',
  options: ['Fix CI', 'Dismiss'], details: null, created: new Date(Date.now() - 60_000).toISOString(), state: 'open', later: null, headline: null,
  recommended: null, triage: 'needs_you',
  github: { list: 'mine', repo: 'obi-ai-bot/obi', number: 812, author: 'me', url: 'https://github.com/obi-ai-bot/obi/pull/812' },
} as InboxItem;
const job = (more: Partial<Job> = {}) => ({
  id: 'j1', repo: 'obi-ai-bot/obi', title: 'Fix CI obi #812', request: 'Fix CI', agent: 'codex', account: 'codex', branch: 'oby/fix-812', base: 'fix-login',
  issue: null, kind: 'fix', status: 'done', step: '', summary: '', created: new Date().toISOString(), finished: null, commits: 1, published: null,
  rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null, conversation: null, ...more,
}) as Job;

let root: Root | undefined;
let host: HTMLElement;
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; call.mockReset(); vi.useRealTimers(); });
const answers = () => call.mock.calls.filter(([method, path]) => method === 'POST' && String(path).endsWith('/answer'));

describe('GITHUBWORK-10: Waiting for you’s GitHub card starts its work when clicked', () => {
  it('githubwork_10_waiting_for_yous_card_does_what_fix_ci_does', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...OFFER, state: 'done', answer: 'Fix CI', job: 'j1' }) });
    const s = live.liveState();
    s.jobs = {};
    s.inbox = [OFFER];
    const { default: WaitingView } = await import('../WaitingView');
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<WaitingView leaving={false} onClose={() => {}} onGone={() => {}} onOpenTask={() => {}} onOpenTalk={() => {}} />));
    await act(async () => { vi.advanceTimersByTime(600); });
    const row = host.querySelector<HTMLElement>('.waiting-row')!;
    expect(row.classList).toContain('is-startable');
    // its own buttons keep theirs: Dismiss is Dismiss
    await act(async () => row.querySelector<HTMLElement>('.waiting-title')!.click());
    expect(answers()).toHaveLength(1);
    expect(answers()[0][1]).toBe(`/v1/inbox/${OFFER.id}/answer`);
    expect(answers()[0][2]).toMatchObject({ answer: 'Fix CI' });
  }, 60_000);
});

describe('TASKCONV-32: a task’s card names the branch its commits went to', () => {
  it('taskconv_32_a_push_onto_the_pull_requests_branch_names_that_branch', () => {
    const s = live.liveState();
    const pushed = job({ outgoing: [{ action: 'push', id: 'abc', branch: 'fix-login', date: new Date().toISOString() }] } as Partial<Job>);
    expect(live.taskOf(s, pushed).job.branch).toBe('fix-login');
    // nothing pushed yet: its own branch
    expect(live.taskOf(s, job()).job.branch).toBe('oby/fix-812');
  });
});
