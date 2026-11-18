// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { CheckRun, InboxItem } from '../obid';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

// Home's GitHub card, lean (docs/contracts/github-work.md, GITHUBWORK-20 to 27): one line says what
// happened, then only the failed checks, then the start button, Later and Dismiss, with Open on GitHub
// a quiet link; the stack behind it peeks out under it, never through it.
const stackCss = readFileSync(join(__dirname, '../askStack.css'), 'utf8');
const call = vi.fn();
const openExternal = vi.fn();
let live: typeof import('../live');
let tasks: typeof import('../tasks');
let AskStack: typeof import('../AskStack');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call }, openExternal } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  tasks = await import('../tasks');
  AskStack = await import('../AskStack');
}, 120_000);
afterAll(() => vi.unstubAllGlobals());

const base = { job: '', details: null, created: '2026-11-17T10:00:00Z', state: 'open', later: null, headline: null, recommended: null };
const PR = 'https://github.com/acme/demo-app/pull/26';
const CI = {
  ...base, id: 'github-ci-f1-abc', kind: 'github', title: 'CI failed: demo-app #26', text: 'Checks failed on your pull request #26 in acme/demo-app.',
  options: ['Fix CI', 'Dismiss'], github: { list: 'mine', repo: 'acme/demo-app', number: 26, author: 'me', url: PR },
} as InboxItem;
const REVIEW = {
  ...base, id: 'github-r1', kind: 'github', title: 'Review requested: other-app #12',
  text: 'Your review is requested on pull request #12 in acme/other-app. Oby doesn’t work in acme/other-app yet.',
  options: ['Review it', 'Dismiss'], github: { list: 'reviews', repo: 'acme/other-app', number: 12, author: 'dana', url: 'https://github.com/acme/other-app/pull/12' },
} as InboxItem;
const ISSUE = {
  ...base, id: 'github-i1', kind: 'github', title: 'Assigned to you: demo-app #7', text: 'Issue #7 in acme/demo-app is assigned to you.',
  options: ['Fix it', 'Dismiss'], github: { list: 'assigned', repo: 'acme/demo-app', number: 7, author: 'lee', url: 'https://github.com/acme/demo-app/issues/7' },
} as InboxItem;
const run = (name: string, state: string): CheckRun => ({ name, state, url: `https://github.com/acme/demo-app/actions/runs/${name.length}` });

type Asking = import('../tasks').Asking;
const card = (i: InboxItem, title: string, more: Partial<Asking> & { checks?: CheckRun[] } = {}): Asking => {
  const { checks, ...rest } = more;
  return {
    id: i.id, title, url: i.github!.url, who: i.github!.list === 'reviews' ? i.github!.author ?? undefined : undefined,
    ask: { ...live.homeAsk(i), ...(checks ? { checks } : {}) }, ...rest,
  } as Asking;
};

let root: Root | undefined;
let host: HTMLElement;
const onOpenTalk = vi.fn();
async function render(asks: Asking[], inbox: InboxItem[]) {
  live.liveState().inbox = inbox;
  tasks.replaceTasks([], { asks, work: [] });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<AskStack.default active onOpen={vi.fn()} onOpenTalk={onOpenTalk} />));
}
afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  document.body.innerHTML = '';
  call.mockReset();
  openExternal.mockReset();
  onOpenTalk.mockReset();
  vi.useRealTimers();
});
const answers = () => call.mock.calls.filter(([method, path]) => method === 'POST' && String(path).endsWith('/answer'));
const shown = async () => act(async () => { vi.advanceTimersByTime(600); });
const top = () => host.querySelector<HTMLElement>('.ask-card.is-up')!;
const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
const buttons = (el: Element) => [...el.querySelectorAll('button')].map((b) => b.textContent);
const click = async (el: Element | null | undefined) => act(async () => (el as HTMLElement).click());

describe('GITHUBWORK-20: the stack peeks out under its top card, never through it', () => {
  it('githubwork_20_the_top_card_is_opaque_and_borderless_over_the_cards_behind_it', () => {
    // another card's room.css styles .ask-card too (a border, a see-through background): on Home's
    // stack the top card's own background and no border win, so the next card can't show through
    const rule = stackCss.match(/\.asks > \.ask-card\s*\{([^}]*)\}/)?.[1] ?? '';
    expect(rule).toMatch(/background:\s*var\(--chip\)/);
    expect(rule).toMatch(/border:\s*0/);
    expect(rule).toMatch(/border-radius:\s*22px/);
    // the cards behind it: only their bottom edge, under it, never a box of their own over it
    const peek = stackCss.match(/\.asks > \.ask-peek\s*\{([^}]*)\}/)?.[1] ?? '';
    expect(peek).toMatch(/top:\s*auto/);
    expect(peek).toMatch(/height:\s*\d+px/);
  });
});

describe('GITHUBWORK-21: one line says what happened, once', () => {
  it('githubwork_21_a_ci_failed_card_is_one_line_with_its_kind_title_and_repository', async () => {
    await render([card(CI, '#26 feat: add cube', { checks: [run('pytest', 'FAILURE')] })], [CI]);
    const line = top().querySelector('.gh-line')!;
    expect(text(line.querySelector('.gh-kind'))).toBe('CI failed');
    expect(text(line.querySelector('.gh-title'))).toBe('#26 feat: add cube');
    expect(text(line.querySelector('.gh-meta'))).toBe('· demo-app');
    // the whole line, and the title once: no "Start it?", no headline or sentence saying it again
    const all = text(top());
    expect(all.split('#26 feat: add cube')).toHaveLength(2);
    expect(all).not.toContain('Start it?');
    expect(all).not.toContain('CI failed: demo-app #26');
    expect(all).not.toContain('Checks failed on your pull request');
  });

  it('githubwork_21_a_long_title_keeps_to_its_line_and_shows_whole_on_hover', async () => {
    const long = '#31 refactor: move the renderer’s settings store into its own module and migrate every caller to it';
    await render([card(CI, long)], [CI]);
    const title = top().querySelector<HTMLElement>('.gh-title')!;
    expect(title.title).toBe(long);
    expect(stackCss).toMatch(/\.gh-title\s*\{[^}]*text-overflow:\s*ellipsis/);
    expect(stackCss).toMatch(/\.gh-line\s*\{[^}]*white-space:\s*nowrap|\.gh-title\s*\{[^}]*white-space:\s*nowrap/);
  });
});

describe('GITHUBWORK-22: only the failed checks, one line each', () => {
  it('githubwork_22_each_failed_check_is_a_line_and_nothing_counts_them', async () => {
    const checks = [run('pytest', 'FAILURE'), run('e2e', 'PENDING'), run('lint', 'FAILURE'), run('typecheck', 'SUCCESS')];
    await render([card(CI, '#26 feat: add cube', { checks })], [CI]);
    const rows = [...top().querySelectorAll('.gh-checks li')].map((li) => text(li.querySelector('.gh-check')));
    expect(rows).toEqual(['pytest failed', 'lint failed']);
    expect(text(top())).not.toMatch(/\d+ of \d+ failed/);
    expect(top().querySelector('.pr-checks')).toBeNull();
  });

  it('githubwork_22_a_checks_details_opens_its_page_and_starts_nothing', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    await render([card(CI, '#26 feat: add cube', { checks: [run('pytest', 'FAILURE')] })], [CI]);
    await shown();
    await click([...top().querySelectorAll('.gh-checks button')].find((b) => b.textContent === 'Details'));
    expect(openExternal).toHaveBeenCalledWith(run('pytest', 'FAILURE').url);
    expect(answers()).toHaveLength(0);
  });

  it('githubwork_22_past_three_failed_the_rest_are_one_line_to_github', async () => {
    const checks = ['a', 'bb', 'ccc', 'dddd', 'eeeee'].map((n) => run(n, 'FAILURE'));
    await render([card(CI, '#26 feat: add cube', { checks })], [CI]);
    expect(top().querySelectorAll('.gh-checks li .gh-check')).toHaveLength(3);
    const more = top().querySelector('.gh-checks-more')!;
    expect(text(more)).toBe('2 more failed');
    await click(more);
    expect(openExternal).toHaveBeenCalledWith(`${PR}/checks`);
  });

  it('githubwork_22_no_failed_check_known_no_lines', async () => {
    await render([card(CI, '#26 feat: add cube')], [CI]);
    expect(top().querySelector('.gh-checks')).toBeNull();
  });
});

describe('GITHUBWORK-23: its start button, Later and Dismiss; Open on GitHub a quiet link', () => {
  it('githubwork_23_fix_ci_leads_then_later_and_dismiss', async () => {
    await render([card(CI, '#26 feat: add cube')], [CI]);
    const row = top().querySelector('.tc-options')!;
    expect(buttons(row)).toEqual(['Fix CI', 'Later', 'Dismiss']);
    expect(row.querySelector('.is-primary')?.textContent).toBe('Fix CI');
    const open = top().querySelector<HTMLElement>('.gh-open')!;
    expect(text(open)).toBe('Open on GitHub');
    expect(open.classList).not.toContain('tc-option');
  });

  it('githubwork_23_open_on_github_opens_its_page_and_starts_nothing', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    await render([card(CI, '#26 feat: add cube')], [CI]);
    await shown();
    await click(top().querySelector('.gh-open'));
    expect(openExternal).toHaveBeenCalledWith(PR);
    expect(answers()).toHaveLength(0);
  });

  it('githubwork_23_later_puts_it_off_and_dismiss_answers_dismiss', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...CI, state: 'later' }) });
    await render([card(CI, '#26 feat: add cube'), card(ISSUE, '#7 Settings page crashes')], [CI, ISSUE]);
    await shown();
    await click([...top().querySelectorAll('button')].find((b) => b.textContent === 'Later'));
    expect(answers()[0][1]).toBe(`/v1/inbox/${CI.id}/answer`);
    expect(answers()[0][2]).toHaveProperty('later');
    await shown();
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...ISSUE, state: 'done', answer: 'Dismiss' }) });
    await click([...top().querySelectorAll('button')].find((b) => b.textContent === 'Dismiss'));
    expect(answers()[1][1]).toBe(`/v1/inbox/${ISSUE.id}/answer`);
    expect(answers()[1][2]).toMatchObject({ answer: 'Dismiss' });
  });
});

describe('GITHUBWORK-24: the pager only when there is more than one card', () => {
  it('githubwork_24_one_card_has_no_pager', async () => {
    await render([card(CI, '#26 feat: add cube')], [CI]);
    expect(top().querySelector('.ask-nav')).toBeNull();
    expect(text(top())).not.toContain('1/1');
  });

  it('githubwork_24_several_cards_page_round_on_the_line', async () => {
    await render([card(CI, '#26 feat: add cube'), card(REVIEW, '#12 Add a health endpoint')], [CI, REVIEW]);
    const nav = top().querySelector('.gh-line .ask-nav')!;
    expect(text(nav)).toBe('1/2');
    await click(nav.querySelector('[aria-label="Next"]'));
    expect(text(top().querySelector('.gh-title'))).toBe('#12 Add a health endpoint');
  });
});

describe('GITHUBWORK-25: a review request and an assigned issue, the same lean card', () => {
  it('githubwork_25_a_review_request_says_who_and_adds_only_what_the_line_doesnt', async () => {
    await render([card(REVIEW, '#12 Add a health endpoint')], [REVIEW]);
    expect(text(top().querySelector('.gh-kind'))).toBe('Review requested');
    expect(text(top().querySelector('.gh-meta'))).toBe('· dana · other-app');
    // obid's first sentence says the line again; what follows it is news, and stays
    expect(text(top().querySelector('.gh-note'))).toBe('Oby doesn’t work in acme/other-app yet.');
    expect(text(top())).not.toContain('Your review is requested');
    expect(buttons(top().querySelector('.tc-options')!)).toEqual(['Review it', 'Later', 'Dismiss']);
  });

  it('githubwork_25_an_assigned_issue_is_its_line_and_fix_it', async () => {
    await render([card(ISSUE, '#7 Settings page crashes')], [ISSUE]);
    expect(text(top().querySelector('.gh-line .gh-kind'))).toBe('Assigned to you');
    expect(text(top().querySelector('.gh-meta'))).toBe('· demo-app');
    expect(top().querySelector('.gh-note')).toBeNull();
    expect(text(top())).not.toContain('is assigned to you.');
    expect(buttons(top().querySelector('.tc-options')!)).toEqual(['Fix it', 'Later', 'Dismiss']);
  });
});

describe('GITHUBWORK-26: a click on the lean card still starts its work (GITHUBWORK-10)', () => {
  it('githubwork_26_a_click_on_its_line_answers_fix_ci_once', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...CI, state: 'done', answer: 'Fix CI', job: 'j1' }) });
    await render([card(CI, '#26 feat: add cube', { checks: [run('pytest', 'FAILURE')] })], [CI]);
    await shown();
    expect(top().classList).toContain('is-startable');
    await click(top().querySelector('.gh-line .gh-title'));
    expect(answers().map((a) => a[2].answer)).toEqual(['Fix CI']);
  });
});

describe('GITHUBWORK-27: its work already running, the card opens its conversation', () => {
  it('githubwork_27_the_card_and_its_open_conversation_link_open_it', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    await render([card(CI, '#26 feat: add cube', { talk: 'c1' })], [CI]);
    await shown();
    await click(top().querySelector('.gh-title'));
    expect(onOpenTalk).toHaveBeenLastCalledWith('c1');
    onOpenTalk.mockReset();
    const link = [...top().querySelectorAll<HTMLElement>('.gh-open')].find((b) => text(b) === 'Open conversation')!;
    await click(link);
    expect(onOpenTalk).toHaveBeenCalledWith('c1');
    expect(answers()).toHaveLength(0);
  });
});
