// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { CheckRun, GitHubWork, GitHubWorkItem, Job } from '../obid';

// GitHub on the task (docs/contracts/github-work.md, GITHUBWORK-30 and 31; docs/contracts/tasks.md,
// TASKS-34): Fix CI on the owner's own red pull request's checks, one click one task; and the chip
// of the issue a task was started from.
const openExternal = vi.fn();
type Answer = { status: number; text: string };
const calls: [string, string][] = [];
let answer: (method: string, path: string) => Promise<Answer> = async () => ({ status: 404, text: '{"error":"Not found."}' });
let live: typeof import('../live');
let PrChecks: typeof import('../PrChecks');
let TaskCard: typeof import('../TaskCard');
let GroupView: typeof import('../GroupView');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const call = (method: string, path: string) => { calls.push([method, path]); return answer(method, path); };
  window.app = { obid: { call }, openExternal } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  PrChecks = await import('../PrChecks');
  TaskCard = await import('../TaskCard');
  GroupView = await import('../GroupView');
}, 120_000);
afterAll(() => vi.unstubAllGlobals());

let root: Root | undefined;
let host: HTMLElement;
async function render(node: React.ReactNode) {
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(node));
}
afterEach(async () => {
  await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = '';
  openExternal.mockReset(); calls.length = 0;
});
const flush = () => act(async () => { for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0)); });

const RED: CheckRun[] = [
  { name: 'build', state: 'SUCCESS', url: 'https://github.com/acme/demo-app/actions/runs/1' },
  { name: 'test', state: 'FAILURE', url: 'https://github.com/acme/demo-app/actions/runs/2' },
];
const pr = (more: Partial<GitHubWorkItem> = {}): GitHubWorkItem => ({
  id: 'f1', list: 'mine', repo: 'acme/demo-app', number: 812, title: 'Retry jitter', url: 'https://github.com/acme/demo-app/pull/812',
  author: 'me', updated: '2026-11-19T10:00:00Z', draft: false, checks: 'FAILURE', reviewDecision: null, fork: false, board: null,
  known: true, job: null, checkRuns: RED, ...more,
});
const feed = (mine: GitHubWorkItem[], more: Partial<GitHubWork> = {}): GitHubWork => ({ signedIn: true, error: null, reviews: [], assigned: [], mine, ...more });
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Fix CI demo-app #812', agent: 'claude', account: 'a1', repo: 'acme/demo-app', branch: 'fix-login', base: 'fix-login', issue: null,
  kind: 'fix', status: 'running', step: 'Working', summary: '', created: '2026-11-19T10:00:00Z', finished: null, commits: 0, published: null,
  rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, title: 'Fix CI demo-app #812', archived: null, ...more,
} as Job);
const fixButton = () => host.querySelector<HTMLButtonElement>('.pr-checks-line .pr-checks-fix');
const options = () => [...host.querySelectorAll('.tc-options .tc-option')].map((b) => b.textContent);

describe('GITHUBWORK-30: Fix CI on the red check of your own pull request', () => {
  it('githubwork_30_fix_ci_is_on_the_red_checks_line_of_your_own_pull_request', async () => {
    const [own] = live.workOf(feed([pr()]));
    expect(own.fixCi).toBe(true);
    expect(own.action).toBeUndefined();  // one start, on the checks: no Fix it beside it
    // anyone else's, a fork's, one in a repository Oby doesn't work in, a draft, a green one: no Fix CI
    const none = live.workOf(feed([pr({ id: 'f2', fork: true }), pr({ id: 'f3', known: false }), pr({ id: 'f4', draft: true }),
      pr({ id: 'f5', checks: 'SUCCESS', checkRuns: [RED[0]] })], { reviews: [pr({ id: 'f6', list: 'reviews' })] }));
    expect(none.map((g) => [g.id, !!g.fixCi])).toEqual([['f6', false], ['f2', false], ['f3', false], ['f4', false], ['f5', false]]);
    // red with no check of its listed as failed: its start stays the card's Fix it
    const [unlisted] = live.workOf(feed([pr({ checkRuns: [] })]));
    expect([unlisted.fixCi, unlisted.action]).toEqual([undefined, 'Fix it']);

    await render(<GroupView.GitHubCard item={own} />);
    expect(fixButton()?.textContent).toBe('Fix CI');
    expect(host.querySelector('.pr-checks-line')?.textContent).toBe('1 of 2 failedFix CI');
    expect(options()).toEqual(['Open on GitHub', 'Later']);
  });

  it('githubwork_30_no_fix_ci_without_a_failed_check_or_off_your_own', async () => {
    await render(<PrChecks.default runs={[RED[0]]} fix={{ onFix: vi.fn() }} />);
    expect(fixButton()).toBeNull();
    await act(async () => root?.unmount());
    await render(<PrChecks.default runs={RED} />);
    expect(fixButton()).toBeNull();
  });

  it('githubwork_30_fix_ci_starts_the_feeds_fix_ci_task', async () => {
    const s = live.liveState();
    s.github = feed([pr()]);
    answer = async () => ({ status: 201, text: JSON.stringify(job()) });
    await render(<GroupView.GitHubCard item={live.workOf(s.github)[0]} />);
    await act(async () => fixButton()!.click());
    await flush();
    expect(calls).toEqual([['POST', '/v1/github/work/f1/start']]);
  });
});

describe('GITHUBWORK-31: one click starts one task', () => {
  it('githubwork_31_a_second_click_shows_the_task_not_a_new_one', async () => {
    const s = live.liveState();
    s.github = feed([pr({ id: 'g1' })]);
    let go!: () => void;
    const held = new Promise<void>((done) => { go = done; });
    answer = async () => { await held; return { status: 201, text: JSON.stringify(job({ id: 'j7' })) }; };
    const onOpenTask = vi.fn();
    await render(<GroupView.GitHubCard item={live.workOf(s.github)[0]} onOpenTask={onOpenTask} />);
    // while its start is on its way: disabled, and another click starts nothing
    await act(async () => { fixButton()!.click(); fixButton()!.click(); });
    expect(fixButton()!.disabled).toBe(true);
    expect(await live.fixCi('g1')).toBeUndefined();
    expect(calls).toEqual([['POST', '/v1/github/work/g1/start']]);
    go();
    await flush();
    expect(fixButton()!.disabled).toBe(false);
    expect(onOpenTask).not.toHaveBeenCalled();
    // its task is on it now: a click opens it, and obid isn't asked again
    await act(async () => fixButton()!.click());
    await flush();
    expect(onOpenTask).toHaveBeenCalledWith('j7');
    expect(calls).toHaveLength(1);
  });

  it('githubwork_31_a_task_the_feed_has_on_it_opens', async () => {
    const s = live.liveState();
    s.jobs = { ...s.jobs, j8: job({ id: 'j8' }) };
    s.github = feed([pr({ id: 'g2', job: 'j8' })]);
    expect(await live.fixCi('g2')).toBe('j8');
    // obid answers a running task on it instead of a new one: that one opens
    s.github = feed([pr({ id: 'g3' })]);
    answer = async () => ({ status: 200, text: JSON.stringify(job({ id: 'j8' })) });
    expect(await live.fixCi('g3')).toBe('j8');
    expect(calls).toEqual([['POST', '/v1/github/work/g3/start']]);
    // once that task ended, a click starts a new one (a new red head)
    const now = live.liveState();  // a job taken makes a new state
    now.jobs = { ...now.jobs, j8: job({ id: 'j8', status: 'done' }) };
    answer = async () => ({ status: 201, text: JSON.stringify(job({ id: 'j9' })) });
    expect(await live.fixCi('g3')).toBeUndefined();
    expect(calls).toEqual([['POST', '/v1/github/work/g3/start'], ['POST', '/v1/github/work/g3/start']]);
  });
});

describe('TASKS-34: a task shows the issue it came from', () => {
  const issueUrl = 'https://github.com/acme/demo-app/issues/140';
  const assigned = pr({ id: 'a1', list: 'assigned', number: 140, title: 'Add a line to the docs', url: issueUrl, checks: null, checkRuns: undefined, job: 'j2' });
  const taskOf = (j: Job, github: GitHubWork | null = feed([], { assigned: [assigned] })) => live.taskOf({ ...live.liveState(), github, jobs: { [j.id]: j } }, j);

  it('tasks_34_a_task_from_an_issue_shows_its_chip_and_opens_it', async () => {
    const t = taskOf(job({ id: 'j2', issue: 140, issueUrl }));
    expect(t.job.issue).toEqual({ number: 140, url: issueUrl, title: 'Add a line to the docs' });
    const onOpen = vi.fn();
    await render(<TaskCard.default task={t} onOpen={onOpen} />);
    const chip = host.querySelector<HTMLButtonElement>('.tc-issue')!;
    expect(chip.textContent).toBe('#140 · Add a line to the docs');
    await act(async () => chip.click());
    expect(openExternal).toHaveBeenCalledWith(issueUrl);
    expect(onOpen).not.toHaveBeenCalled();  // the chip opens the issue, not the task's page
    // an issue the feed doesn't list: its number alone
    expect(taskOf(job({ id: 'j3', issue: 141, issueUrl: 'https://github.com/acme/demo-app/issues/141' })).job.issue)
      .toEqual({ number: 141, url: 'https://github.com/acme/demo-app/issues/141' });
  });

  it('tasks_34_a_task_with_no_issue_shows_nothing', async () => {
    // no issue; an older obid's job with no issueUrl; a page that isn't GitHub's
    for (const j of [job(), job({ issue: 140 }), job({ issue: 140, issueUrl: 'https://example.com/issues/140' })]) {
      expect(taskOf(j).job.issue).toBeUndefined();
    }
    await render(<TaskCard.default task={taskOf(job())} />);
    expect(host.querySelector('.tc-issue')).toBeNull();
  });
});
