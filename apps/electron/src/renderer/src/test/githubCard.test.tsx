// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { InboxItem } from '../obid';

// A GitHub offer's card on Home (docs/contracts/github-work.md, GITHUBWORK-10): a click anywhere on
// it but its own buttons and links does what its start button does (Fix CI, Review it, Fix it),
// once; a card whose work already has its conversation opens that instead.
const call = vi.fn();
let live: typeof import('../live');
let tasks: typeof import('../tasks');
let AskStack: typeof import('../AskStack');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call }, openExternal: vi.fn() } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  tasks = await import('../tasks');
  AskStack = await import('../AskStack');
}, 120_000);
afterAll(() => vi.unstubAllGlobals());

const OFFER = {
  id: 'github-ci-f1-abc', kind: 'github', job: '', title: 'CI failed: obi #812', text: 'Checks failed on your pull request #812 in obi-ai-bot/obi.',
  options: ['Fix CI', 'Dismiss'], details: null, created: '2026-11-17T10:00:00Z', state: 'open', later: null, headline: null, recommended: null,
  github: { list: 'mine', repo: 'obi-ai-bot/obi', number: 812, author: 'me', url: 'https://github.com/obi-ai-bot/obi/pull/812' },
} as InboxItem;

let root: Root | undefined;
let host: HTMLElement;
const onOpen = vi.fn(), onOpenTalk = vi.fn();
async function render(asks: import('../tasks').Asking[], inbox: InboxItem[] = [OFFER]) {
  live.liveState().inbox = inbox;  // what obid has: an answer finds its item there (answerJob)
  tasks.replaceTasks([], { asks, work: [] });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<AskStack.default active onOpen={onOpen} onOpenTalk={onOpenTalk} />));
}
afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  document.body.innerHTML = '';
  call.mockReset();
  onOpen.mockReset();
  onOpenTalk.mockReset();
  vi.useRealTimers();
});
const answers = () => call.mock.calls.filter(([method, path]) => method === 'POST' && String(path).endsWith('/answer'));
// a click lands only once the card has shown (one click, one answer: AskBlock's half second)
const shown = async () => act(async () => { vi.advanceTimersByTime(600); });

describe('GITHUBWORK-10: a GitHub offer’s card starts its work when clicked', () => {
  it('githubwork_10_clicking_the_card_does_what_fix_ci_does', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...OFFER, state: 'done', answer: 'Fix CI', job: 'j1' }) });
    await render([{ id: OFFER.id, title: '#812 Retry jitter', url: OFFER.github!.url, ask: live.homeAsk(OFFER) }]);
    await shown();
    const card = host.querySelector<HTMLElement>('.ask-card')!;
    expect(card.classList).toContain('is-startable');
    await act(async () => card.querySelector<HTMLElement>('.gh-title')!.click());
    expect(answers()).toHaveLength(1);
    expect(answers()[0][1]).toBe(`/v1/inbox/${OFFER.id}/answer`);
    expect(answers()[0][2]).toMatchObject({ answer: 'Fix CI' });
  });

  it('githubwork_10_its_own_buttons_and_links_keep_their_own_click', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...OFFER, state: 'done', answer: 'Dismiss' }) });
    await render([{ id: OFFER.id, title: '#812 Retry jitter', url: OFFER.github!.url, ask: live.homeAsk(OFFER) }]);
    await shown();
    // Open on GitHub opens its page and answers nothing; Dismiss answers Dismiss, once
    await act(async () => host.querySelector<HTMLElement>('.gh-open')!.click());
    expect(answers()).toHaveLength(0);
    const dismiss = [...host.querySelectorAll('button')].find((b) => b.textContent === 'Dismiss')!;
    await act(async () => dismiss.click());
    expect(answers().map((a) => a[2].answer)).toEqual(['Dismiss']);
  });

  it('githubwork_10_a_card_with_its_conversation_opens_it', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    await render([{ id: OFFER.id, title: '#812 Retry jitter', url: OFFER.github!.url, talk: 'c1', ask: live.homeAsk(OFFER) }]);
    await shown();
    await act(async () => host.querySelector<HTMLElement>('.gh-title')!.click());
    expect(onOpenTalk).toHaveBeenCalledWith('c1');
    expect(answers()).toHaveLength(0);
  });

  it('githubwork_10_another_kind_of_card_isnt_started_by_a_click', async () => {
    vi.useFakeTimers({ toFake: ['performance', 'setTimeout', 'Date'] });
    const confirm = { ...OFFER, id: 'confirm-1', kind: 'confirm', github: null, title: 'Delete it?', options: ['Confirm', 'Cancel'] } as InboxItem;
    await render([{ id: confirm.id, title: 'Delete it?', ask: live.homeAsk(confirm) }], [confirm]);
    await shown();
    expect(host.querySelector('.ask-card')!.classList).not.toContain('is-startable');
    await act(async () => host.querySelector<HTMLElement>('.tc-ask-head')!.click());
    expect(answers()).toHaveLength(0);
  });
});
