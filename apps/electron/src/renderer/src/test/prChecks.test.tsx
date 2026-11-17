// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { CheckRun, GitHubWork, InboxItem } from '../obid';

// A pull request's checks, each on its own line, failed first (docs/contracts/github-work.md,
// GITHUBWORK-9), as T3 Code's checks popover lists them: from obid's feed (checkRuns, GITHUBWORK-8)
// to the CI failed card and the feed's card.
const openExternal = vi.fn();
let live: typeof import('../live');
let PrChecks: typeof import('../PrChecks');
let TaskCard: typeof import('../TaskCard');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call: vi.fn() }, openExternal } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  PrChecks = await import('../PrChecks');
  TaskCard = await import('../TaskCard');
}, 120_000);
afterAll(() => vi.unstubAllGlobals());

const RUNS: CheckRun[] = [
  { name: 'build', state: 'SUCCESS', url: 'https://github.com/obi-ai-bot/obi/actions/runs/1' },
  { name: 'lint', state: 'PENDING', url: null },
  { name: 'test', state: 'FAILURE', url: 'https://github.com/obi-ai-bot/obi/actions/runs/2' },
  { name: 'docs', state: 'SUCCESS', url: 'https://github.com/obi-ai-bot/obi/actions/runs/3' },
  { name: 'ci/circleci', state: 'FAILURE', url: 'http://circleci.example/x' },
];

let root: Root | undefined;
let host: HTMLElement;
async function render(node: React.ReactNode) {
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(node));
}
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; openExternal.mockReset(); });
const rows = () => [...host.querySelectorAll('.pr-checks-list li')].map((li) => [li.querySelector('.pr-check-name')?.textContent, li.querySelector('.pr-check-state')?.textContent, !!li.querySelector('.pr-check-details')]);

describe('GITHUBWORK-9: a pull request’s checks are a list, failed first', () => {
  it('githubwork_9_failed_then_running_then_passed_each_with_its_state_and_details', async () => {
    expect(PrChecks.sortChecks(RUNS).map((r) => r.name)).toEqual(['test', 'ci/circleci', 'lint', 'build', 'docs']);
    expect(PrChecks.checksLine(RUNS)).toBe('2 of 5 failed, 1 running');
    expect(PrChecks.checksLine(RUNS.filter((r) => r.state !== 'FAILURE'))).toBe('1 of 3 running');
    expect(PrChecks.checksLine(RUNS.filter((r) => r.state === 'SUCCESS'))).toBe('2 passed');
    await render(<PrChecks.default runs={RUNS} />);
    expect(host.querySelector('.pr-checks-line')?.textContent).toBe('2 of 5 failed, 1 running');
    // while any failed or runs, the passed ones wait behind Show all; Details only for an https page
    expect(rows()).toEqual([['test', 'Failed', true], ['ci/circleci', 'Failed', false], ['lint', 'Running', false]]);
    const showAll = [...host.querySelectorAll('button')].find((b) => b.textContent === 'Show all 5')!;
    await act(async () => showAll.click());
    expect(rows().map((r) => r[0])).toEqual(['test', 'ci/circleci', 'lint', 'build', 'docs']);
    await act(async () => host.querySelector<HTMLButtonElement>('.pr-check-details')!.click());
    expect(openExternal).toHaveBeenCalledWith('https://github.com/obi-ai-bot/obi/actions/runs/2');
  });

  it('githubwork_9_all_passed_shows_them_all', async () => {
    await render(<PrChecks.default runs={RUNS.filter((r) => r.state === 'SUCCESS')} />);
    expect(rows().map((r) => r[0])).toEqual(['build', 'docs']);
    expect(host.textContent).not.toContain('Show all');
  });

  it('githubwork_9_the_ci_failed_card_lists_its_pull_requests_checks', async () => {
    const s = live.liveState();
    // obid's feed, and its CI failed offer for the same pull request (github_work.rs)
    s.github = { signedIn: true, error: null, reviews: [], assigned: [], mine: [{
      id: 'f1', list: 'mine', repo: 'obi-ai-bot/obi', number: 812, title: 'Retry jitter', url: 'https://github.com/obi-ai-bot/obi/pull/812',
      author: 'me', updated: '2026-11-17T10:00:00Z', draft: false, checks: 'FAILURE', reviewDecision: null, board: null, known: true, job: null, checkRuns: RUNS,
    }] } as GitHubWork;
    const offer = {
      id: 'github-ci-f1-abc', kind: 'github', job: '', title: 'CI failed: obi #812', text: 'Checks failed on your pull request #812 in obi-ai-bot/obi.',
      options: ['Fix CI', 'Dismiss'], details: null, created: '2026-11-17T10:00:00Z', state: 'open', later: null, headline: null, recommended: null,
      github: { list: 'mine', repo: 'obi-ai-bot/obi', number: 812, author: 'me', url: 'https://github.com/obi-ai-bot/obi/pull/812' },
    } as InboxItem;
    expect(live.prChecksOf(s, offer).checks?.map((r) => r.name)).toEqual(RUNS.map((r) => r.name));
    // another pull request's, or a feed without it: none
    expect(live.prChecksOf(s, { ...offer, github: { ...offer.github!, number: 9 } })).toEqual({});
    expect(live.prChecksOf({ github: null }, offer)).toEqual({});
    const ask = { ...live.homeAsk(offer), ...live.prChecksOf(s, offer) };
    await render(<TaskCard.AskBlock task={{ id: offer.id, job: { ask } }} />);
    expect(host.querySelector('.tc-ask-head')?.textContent).toBe('CI failed: obi #812');
    expect(rows().map((r) => r.slice(0, 2))).toEqual([['test', 'Failed'], ['ci/circleci', 'Failed'], ['lint', 'Running']]);
  });
});
