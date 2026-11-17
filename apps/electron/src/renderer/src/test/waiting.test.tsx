// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { InboxItem, Job } from '../obid';

// Waiting for you (docs/contracts/home.md, WAITING-1 to 3): one list across the profile's projects
// of what needs the owner's word, over obid's inbox. obid answers through the bridge main gives the
// page (window.app.obid), stood in for here; the clock is vitest's.
const call = vi.fn();
let live: typeof import('../live');
let words: typeof import('../../../shared/words');
beforeAll(async () => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date('2026-11-15T12:00:00Z'));
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('app', { obid: { call } });
  window.app = { obid: { call } } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  words = await import('../../../shared/words');
});
afterAll(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

const at = (minutesAgo: number) => new Date(Date.now() - minutesAgo * 60_000).toISOString();
const item = (id: string, kind: string, job: string, created: string, more: Partial<InboxItem> = {}): InboxItem => ({
  id, kind, job, title: id, text: '', options: [], details: null, created, state: 'open', later: null, headline: null,
  recommended: null, triage: 'needs_you', ...more,
});
const job = (id: string, repo: string, title: string, more: Partial<Job> = {}) =>
  ({
    id, repo, title, request: title, agent: 'claude', account: 'a1', branch: `oby/${id}`, base: 'dev', issue: null, kind: 'work', status: 'done',
    step: '', summary: '', created: at(60), finished: null, commits: 1, published: null, rounds: 1, round: 1, check: null, model: null,
    dismissed: null, permission: null, pipeline: null, conversation: null, ...more,
  }) as Job;

const review = (agent: string, commit: string) => ({ verdict: 'approved', job: `r-${agent}`, agent, account: 'a1', round: 1, date: at(45), commit });

// three that need you in two projects, one that doesn't, and one put off
function fill() {
  const s = live.liveState();
  s.jobs = {
    j1: job('j1', 'obi-ai-bot/oby', 'Fix the live panel’s queue order'),
    j2: job('j2', 'deskree-inc/tetrix', 'Retry jitter for the collector', {
      summary: 'Backoff gets ±20% jitter so a fleet doesn’t retry at once.',
      // two tools approved the head obid pushed; an approval of an older head doesn't count
      outgoing: [{ action: 'push', id: 'old', branch: 'oby/j2', date: at(70) }, { action: 'push', id: 'head', branch: 'oby/j2', date: at(50) }],
      approvals: [review('codex', 'head'), review('cursor', 'old')],
      review: review('claude', 'head'),
      diff: { files: 2, added: 21, removed: 4 },
      pipeline: { number: 6, total: 6, stages: [
        { id: 'check', title: 'Check', state: 'done', detail: 'npm test' },
        { id: 'review', title: 'Review', state: 'done', detail: 'Approved by Oby' },
      ] },
    }),
    j3: job('j3', 'obi-ai-bot/oby', 'Fix the iOS signing step', { status: 'running', permission: { id: 'p1', tool: 'Bash', title: 'Run', detail: 'xcodebuild -exportArchive', rule: null, always: null } }),
  };
  s.inbox = [
    item('merge-j1', 'merge', 'j1', at(12), { title: 'Ready to merge: Fix the live panel’s queue order', text: 'Merge oby #2341 into dev?', options: ['Merge', 'Leave it open'], triage: undefined }),
    item('j3-permission-p1', 'permission', 'j3', at(5), { text: 'Run xcodebuild', options: ['Allow', 'Deny'] }),
    item('merge-j2', 'merge', 'j2', at(40), { title: 'Ready to merge: Retry jitter for the collector', text: 'Merge tetrix-collectors #412 into main?', options: ['Merge', 'Leave it open'] }),
    item('fyi-1', 'github', '', at(90), { triage: 'fyi' }),
    item('later-1', 'question', '', at(80), { later: at(-60) }),
  ];
  s.answering = new Set();
  s.answered = {};
}

let root: Root | undefined;
let host: HTMLElement;
async function show(node: React.ReactNode) {
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(node));
}
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; call.mockReset(); });

async function waitingPage() {
  const { default: WaitingView } = await import('../WaitingView');
  await show(<WaitingView leaving={false} onClose={() => {}} onGone={() => {}} onOpenTask={() => {}} onOpenTalk={() => {}} />);
}
const rows = () => [...host.querySelectorAll('.waiting-row')].map((r) => r.getAttribute('aria-label'));

describe('Waiting for you', () => {
  it('waiting_1_one_list_longest_first', async () => {
    fill();
    await waitingPage();
    expect(rows()).toEqual(['Retry jitter for the collector', 'Fix the live panel’s queue order', 'Fix the iOS signing step']);
    expect(host.querySelector('.waiting-counts')?.textContent).toBe('2 to merge · 1 permission');
    expect(host.textContent).toContain('tetrix · 40m');
    // a permission says what it asks in place of how long (screen 17)
    expect(host.textContent).toContain('oby · asks to run a command');
    // a merge's row: Oby's summary, its check and who approved it
    const first = host.querySelector('.waiting-row')!;
    expect(first.querySelector('.waiting-summary')?.textContent).toBe('Backoff gets ±20% jitter so a fleet doesn’t retry at once.');
    const tags = [...first.querySelectorAll('.waiting-tag')];
    expect(tags.map((t) => t.textContent)).toEqual(['Check passed', 'Approved', 'Approved', '+21 −4 · 2 files']);
    expect(tags.map((t) => t.getAttribute('aria-label')).filter(Boolean)).toEqual(['Approved by Claude Code', 'Approved by Codex']);
    // All projects narrows to one
    const select = host.querySelector('select')!;
    expect([...select.options].map((o) => o.textContent)).toEqual(['All projects', 'tetrix', 'oby']);
    await act(async () => { select.value = 'obi-ai-bot/oby'; select.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(rows()).toEqual(['Fix the live panel’s queue order', 'Fix the iOS signing step']);
  });

  it('waiting_2_merge_is_signed_and_leaves', async () => {
    fill();
    await waitingPage();
    call.mockResolvedValue({ status: 200, text: JSON.stringify(item('merge-j2', 'merge', 'j2', at(40), { state: 'done' })) });
    const merge = [...host.querySelectorAll('.waiting-row')][0].querySelector<HTMLButtonElement>('button.is-primary')!;
    expect(merge.textContent).toBe('Merge');
    await act(async () => merge.click());
    // the merge card's own answer, in the shape main signs by the clicked button's label
    expect(call).toHaveBeenCalledWith('POST', '/v1/inbox/merge-j2/answer', { answer: 'Merge' });
    expect(words.owners('POST', '/v1/inbox/merge-j2/answer', { answer: 'Merge' }, [merge.textContent!], [])).toEqual([0]);
    expect(rows()).toEqual(['Fix the live panel’s queue order', 'Fix the iOS signing step']);
  });

  it('waiting_2_allow_once_answers_the_permission_signed', async () => {
    fill();
    await waitingPage();
    call.mockResolvedValue({ status: 200, text: JSON.stringify(live.liveState().jobs.j3) });
    const allow = [...host.querySelectorAll('button')].find((b) => b.textContent === 'Allow once')!;
    await act(async () => allow.click());
    expect(call).toHaveBeenCalledWith('POST', '/v1/jobs/j3/permission', { id: 'p1', decision: 'allow' });
    // main signs it on the label held, Allow once standing for allow
    expect(words.owners('POST', '/v1/jobs/j3/permission', { id: 'p1', decision: 'allow' }, ['Allow once'], [])).toEqual([0]);
    expect(words.owners('POST', '/v1/jobs/j3/permission', { id: 'p1', decision: 'deny' }, ['Allow once'], [])).toBeNull();
  });

  it('waiting_2_another_obys_question_is_shown_quoted_and_answered_in_words', async () => {
    fill();
    const s = live.liveState();
    // as obid makes it (sphere/inbox.rs, question_item): Decline and Decline-and-remember its options,
    // the question and its choices quoted under obi.quoted, never buttons
    s.inbox = [item('obi-q1', 'obiRequest', '', at(3), {
      title: 'Question from dima', text: 'dima’s Obi (dima-obi) asks a question, quoted below. Answer it in your own words, or say no.',
      options: ['Decline', 'Decline, and remember for this repo'],
      obi: { type: 'question', fromOwner: 'dima', repos: [], quoted: { text: 'Ship it today?', choices: ['Yes', 'No'] } },
    })];
    let clock = 0;
    const now = vi.spyOn(performance, 'now').mockImplementation(() => clock);
    await waitingPage();
    clock += 1000;
    const row = host.querySelector('.waiting-row')!;
    expect(row.querySelector('blockquote')?.textContent).toBe('Ship it today?');
    expect(row.textContent).toContain('Their choices: Yes · No');
    const buttons = () => [...row.querySelectorAll('button')];
    expect(buttons().map((b) => b.textContent)).toEqual(['Answer', 'Decline', 'Later']);
    await act(async () => buttons()[0].click());
    const box = row.querySelector<HTMLInputElement>('input[aria-label="Your answer"]')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(box, 'Yes');
      box.dispatchEvent(new Event('input', { bubbles: true }));
    });
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...s.inbox[0], state: 'done' }) });
    await act(async () => box.form!.requestSubmit());
    // the owner's own words, which main signs as typed
    expect(call).toHaveBeenCalledWith('POST', '/v1/inbox/obi-q1/answer', { answer: 'Yes' });
    expect(words.owners('POST', '/v1/inbox/obi-q1/answer', { answer: 'Yes' }, ['Yes'], [])).toEqual([0]);
    expect(rows()).toEqual([]);
    now.mockRestore();
  });

  it('waiting_2_another_obys_review_request_shows_all_it_quotes_before_its_answers', async () => {
    fill();
    const s = live.liveState();
    // as obid makes it (sphere/inbox.rs, review_request_item): its four answers, what that Oby wrote
    // (goal, summary, conditions) quoted under obi.quoted
    s.inbox = [item('obi-r1', 'obiRequest', '', at(3), {
      title: 'Review request from dima', text: 'dima’s Obi asks you to review acme/app at abc1234.',
      options: ['Accept', 'Accept, and remember for this repo', 'Decline', 'Decline, and remember for this repo'],
      obi: { type: 'review_request', fromOwner: 'dima', repo: 'acme/app', headSha: 'abc1234', quoted: {
        goal: 'Add retry jitter', summary: 'Backoff gets ±20% jitter.', acceptance: [{ text: 'Tests pass', note: 'run the soak test too' }],
      } },
    })];
    let clock = 0;
    const now = vi.spyOn(performance, 'now').mockImplementation(() => clock);
    await waitingPage();
    clock += 1000;
    const row = host.querySelector('.waiting-row')!;
    const quote = row.querySelector('blockquote')!;
    expect([...quote.querySelectorAll('p')].map((p) => p.textContent)).toEqual([
      'Add retry jitter', 'Backoff gets ±20% jitter.', 'Their condition: Tests pass · run the soak test too',
    ]);
    expect([...row.querySelectorAll('button')].map((b) => b.textContent)).toEqual([
      'Accept', 'Accept, and remember for this repo', 'Decline', 'Decline, and remember for this repo', 'Later',
    ]);
    call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...s.inbox[0], state: 'done' }) });
    await act(async () => row.querySelector<HTMLButtonElement>('button')!.click());
    expect(call).toHaveBeenCalledWith('POST', '/v1/inbox/obi-r1/answer', { answer: 'Accept' });
    expect(rows()).toEqual([]);
    now.mockRestore();
  });

  it('waiting_2_an_item_with_no_task_or_conversation_is_answered_here', async () => {
    fill();
    const s = live.liveState();
    // Oby's ask to share (sphere/inbox.rs, share_item): a Confirm with no job, its repositories to tick;
    // and one obid sorts as needing the owner with no task, conversation or answers of its own
    s.inbox = [
      item('share-1', 'confirm', '', at(9), {
        title: 'Can I tell people in your Obysphere how your tasks are going, with each one’s current step and latest report line, in oby?',
        text: 'Only what you tick.', options: ['Confirm', 'Cancel'],
        confirm: { action: 'share_status', target: '', payload: { offered: ['obi-ai-bot/oby'], repos: ['obi-ai-bot/oby'] } },
      }),
      item('bare-1', 'question', '', at(4), { title: 'Oby is stuck', text: 'Nothing to go on.' }),
    ];
    let clock = 0;
    const now = vi.spyOn(performance, 'now').mockImplementation(() => clock);
    await waitingPage();
    clock += 1000;
    const [share, bare] = [...host.querySelectorAll('.waiting-row')];
    expect(share.textContent).toContain('Only what you tick.');
    expect(share.querySelector('[aria-label="Repositories to tell about"]')).not.toBeNull();
    expect([...share.querySelectorAll('.tc-options button')].map((b) => b.textContent)).toEqual(['Confirm', 'Cancel', 'Later']);
    expect(bare.textContent).toContain('Nothing to go on.');
    expect([...bare.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['Later']);
    // Confirm: main's own signed Confirm of the card (obid.ts, api.confirm)
    const confirm = vi.fn().mockResolvedValue({ status: 200, text: JSON.stringify({ ...s.inbox[0], state: 'done' }) });
    (window.app!.obid as unknown as { confirm: typeof confirm }).confirm = confirm;
    await act(async () => share.querySelector<HTMLButtonElement>('.tc-options button')!.click());
    expect(confirm).toHaveBeenCalledWith('share-1');
    expect(rows()).toEqual(['Oby is stuck']);
    now.mockRestore();
  });

  it('waiting_3_count_matches', async () => {
    fill();
    const { default: Sidebar } = await import('../Sidebar');
    const { default: WaitingView } = await import('../WaitingView');
    const none = () => {};
    // the panel and the list it opens, side by side
    const panel = () => <>
      <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none}
        onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none}
        onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} onOpenWaiting={none} />
      <WaitingView leaving={false} onClose={none} onGone={none} onOpenTask={none} onOpenTalk={none} />
    </>;
    await show(panel());
    const row = () => [...host.querySelectorAll('button')].find((b) => b.textContent?.startsWith('Waiting for you'));
    expect(rows()).toHaveLength(3);
    expect(row()?.querySelector('.waiting-count')?.textContent).toBe('3');
    // nothing waiting: no row
    live.liveState().inbox = [];
    await act(async () => { root!.unmount(); });
    await show(panel());
    expect(row()).toBeUndefined();
    expect(rows()).toHaveLength(0);
  });
});
