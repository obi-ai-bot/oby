import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { InboxItem, Job } from '../obid';

// live.ts reads the page's window, address and storage as it loads; obid answers through the
// bridge main gives the page (window.app.obid), stood in for here
const call = vi.fn();
let live: typeof import('../live');
beforeAll(async () => {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', { app: { obid: { call } } });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
});
beforeEach(() => call.mockReset());

const item = (kind: string, more: Partial<InboxItem> = {}): InboxItem => ({
  id: 'j1-permission-p1', kind, job: 'j1', title: 'Fix the login', text: '', options: [], details: null,
  created: '2026-10-30T10:00:00Z', state: 'open', later: null, headline: null, recommended: null, ...more,
});
const answered = (body: unknown) => ({ status: 200, text: JSON.stringify(body) });

describe('BYPASS-5: the agent’s question is Sangam’s Question card', () => {
  it('bypass_5_shows_the_question_as_its_headline_with_the_agents_options_and_tell_it_something', () => {
    const ask = live.itemAsk(item('agentQuestion', { text: 'Which store?', options: ['SQLite', 'Postgres'], details: ['One file', ''] }));
    expect(ask).toEqual({ kind: 'question', headline: 'Which store?', text: '', options: ['SQLite', 'Postgres', 'Tell it something'] });
  });

  it('bypass_5_a_question_with_no_options_can_still_be_answered', () => {
    expect(live.itemAsk(item('agentQuestion', { text: 'What should the button say?' })).options).toEqual(['Tell it something']);
  });

  it('bypass_5_leaves_a_permission_a_permission', () => {
    const ask = live.itemAsk(item('permission', { text: 'Run npm install', options: ['Allow', 'Deny'] }), 'npm install');
    expect(ask.kind).toBe('permission');
    expect(ask.detail).toBe('npm install');
  });

  it('bypass_5_answers_by_the_inbox_item_never_the_jobs_lagging_permission', async () => {
    // the job's permission is still the call's first question; the inbox already has the second
    const s = live.liveState();
    s.jobs.j1 = { id: 'j1', status: 'running', permission: { id: 'p0' } } as unknown as Job;
    s.inbox = [item('agentQuestion', { id: 'j1-permission-p2', text: 'Which checks?', options: ['Unit'] })];
    call.mockResolvedValue(answered(item('agentQuestion', { id: 'j1-permission-p2', state: 'done' })));
    await live.answerJob('j1', 'Unit');
    expect(call).toHaveBeenCalledWith('POST', '/v1/inbox/j1-permission-p2/answer', { answer: 'Unit' });
    // Tell it something: the owner's own words are the answer
    live.liveState().inbox = [item('agentQuestion', { id: 'j1-permission-p3', text: 'Anything else?' })];
    await live.answerJob('j1', 'Tell it something', 'Keep the old flag');
    expect(call).toHaveBeenLastCalledWith('POST', '/v1/inbox/j1-permission-p3/answer', { answer: 'Keep the old flag' });
  });
});

describe('TASKS-14: obid’s own question says why under its title', () => {
  it('tasks_14_shows_obids_reason_and_recommendation_under_the_title', () => {
    const ask = live.itemAsk(item('question', { id: 'orchestrator-1', title: 'Fix the login needs your decision', text: 'Oby looked at it. Its pull request conflicts with main. Oby recommends: Rebase it.', options: ['Done'] }));
    expect(ask).toMatchObject({ kind: 'question', headline: 'Fix the login needs your decision', text: 'Oby looked at it. Its pull request conflicts with main. Oby recommends: Rebase it.', options: ['Done'] });
  });
});

describe('BYPASS-6: a task’s Mode menu', () => {
  it('bypass_6_changes_the_jobs_mode_through_obid', async () => {
    call.mockResolvedValue(answered({ id: 'j1', mode: 'bypass' }));
    await live.changeMode('j1', 'bypass');
    expect(call).toHaveBeenCalledWith('POST', '/v1/jobs/j1/mode', { mode: 'bypass' });
    expect(live.liveState().jobs.j1.mode).toBe('bypass');
  });
});

describe('MERGING-21: a merge card says why it stopped, never just its title (#1680)', () => {
  it('merging_21_leads_with_obids_reason_and_what_to_change_when_no_headline_came', () => {
    const ask = live.itemAsk(item('merge', {
      title: 'Not merged: Oby drive 15:36', options: ['Dismiss'],
      text: "Not merged: it conflicts with main. Ask Oby to rebase it onto main, or settle it on GitHub. Oby's review still asks for changes, so it's yours to merge.",
    }));
    expect(ask).toMatchObject({
      kind: 'merge', label: 'Not merged', headline: 'It conflicts with main.',
      text: "Ask Oby to rebase it onto main, or settle it on GitHub. Oby's review still asks for changes, so it's yours to merge.",
      options: ['Dismiss'],
    });
  });

  it('merging_21_a_ready_card_asks_its_headline_and_lists_its_holds_a_line_each', () => {
    const ask = live.itemAsk(item('merge', {
      title: 'Ready to merge: Resolve issue 153', options: ['Merge', 'Leave it open'],
      text: 'Merge tetrix-e2e #153 into main?\nNo check ran on its head (this repository has none).\nNobody has approved its head.',
    }));
    expect(ask).toMatchObject({
      kind: 'merge', label: 'Ready to merge', headline: 'Merge tetrix-e2e #153 into main?',
      text: 'No check ran on its head (this repository has none).\nNobody has approved its head.',
    });
    // a stop's reason leads, what to change under it, then each hold behind it
    expect(live.itemAsk(item('merge', {
      title: 'Not merged: Fix it', options: ['Dismiss'],
      text: "Not merged: it conflicts with main. Ask Oby to rebase it onto main.\nOby's review still asks for changes.",
    }))).toMatchObject({ headline: 'It conflicts with main.', text: "Ask Oby to rebase it onto main.\nOby's review still asks for changes." });
  });

  it('merging_21_keeps_the_models_headline_over_the_reason', () => {
    const ask = live.itemAsk(item('merge', { title: 'Ready to merge: Fix it', headline: 'Merge it into main?', text: 'A review asks for changes.', options: ['Merge'] }));
    expect([ask.label, ask.headline, ask.text]).toEqual(['Ready to merge', 'Merge it into main?', 'A review asks for changes.']);
    // a Ready to merge card is labelled so, never Not merged
    expect(live.itemAsk(item('merge', { title: 'Ready to merge: Fix it', text: 'Not merged yet: a review asks for changes. Its base is main.', options: ['Merge'] })))
      .toMatchObject({ label: 'Ready to merge', headline: 'A review asks for changes.', text: 'Its base is main.' });
  });
});

describe('TASKCONV-7: the Confirm Oby’s answer made shows under it, and on Home (#1680)', () => {
  const at = (m: number) => `2026-10-31T12:${String(m).padStart(2, '0')}:00Z`;
  const convo = { id: 'c1', title: 'Oby drive 15:36', pinned: false, created: at(0), lastActivity: at(49), cursor: '' };
  const merge = item('confirm', {
    id: 'confirm-1', job: '', title: 'Merge well-prado/tetrix-e2e#147 into main', text: 'Oby’s conversation planned this.',
    options: ['Confirm', 'Cancel'], created: at(49), conversation: 'c1',
  });
  const turns = [
    { id: 't1', who: 'user' as const, text: 'Merge it as it is.', at: at(48), jobs: [], conversation: 'c1' },
    { id: 't2', who: 'oby' as const, text: 'The merge of PR #147 into main waits for your OK.', at: at(49), jobs: [], conversation: 'c1' },
  ];

  it('taskconv_7_the_confirm_is_under_the_answer_that_made_it_one_click_away', () => {
    const chat = live.chatOf({ ...live.liveState(), inbox: [merge], turns: { c1: turns } }, convo);
    expect(chat.said.map((m) => m.text || m.confirm?.id)).toEqual(['Merge it as it is.', 'The merge of PR #147 into main waits for your OK.', 'confirm-1']);
    // what it confirms shows, though no model headline came
    expect(chat.said[2].confirm!.ask).toMatchObject({ kind: 'confirm', headline: 'Merge well-prado/tetrix-e2e#147 into main', text: 'Oby’s conversation planned this.', options: ['Confirm', 'Cancel'] });
  });

  it('taskconv_7_answered_on_home_it_leaves_the_conversation_too', () => {
    const s = live.liveState();
    expect(live.chatOf({ ...s, inbox: [merge], turns: { c1: turns }, answering: new Set(['confirm-1']) }, convo).said.some((m) => m.confirm)).toBe(false);
    expect(live.chatOf({ ...s, inbox: [], turns: { c1: turns } }, convo).said.some((m) => m.confirm)).toBe(false);
  });
});

describe('a task’s card is never lost: words more words followed keep what they started (#1783)', () => {
  const at = (m: number) => `2026-11-01T12:${String(m).padStart(2, '0')}:00Z`;
  const convo = { id: 'c1', title: 'Fixes', pinned: false, created: at(0), lastActivity: at(9), cursor: '' };
  const words = (id: string, m: number) => ({ id, who: 'user' as const, text: id, at: at(m), jobs: [], conversation: 'c1' });
  const oby = (id: string, m: number) => ({ id, who: 'oby' as const, text: id, at: at(m), jobs: [] as string[], conversation: 'c1' });
  const j = { id: 'j', conversation: 'c1', turn: 'u1', created: at(2), status: 'running', title: 'Fix it', summary: '', step: '', agent: 'claude', account: 'a', repo: 'o/r', branch: '', rounds: 1, round: 1 } as unknown as Job;
  const cards = (turns: unknown[]) =>
    Object.fromEntries(live.chatOf({ ...live.liveState(), jobs: { j }, turns: { c1: turns } } as never, convo).said.map((m) => [m.text, m.jobs ?? []]));

  it('stays_under_the_words_that_started_it_when_more_words_follow', () => {
    expect(cards([words('u1', 1), words('u2', 3), oby('o2', 4)])).toEqual({ u1: ['j'], u2: [], o2: [] });
  });

  it('the_last_words_wait_for_their_answer_so_the_card_never_jumps', () => {
    expect(cards([words('u1', 1)])).toEqual({ u1: [] });
  });

  it('still_goes_under_the_answer_and_not_again_under_its_report', () => {
    const report = { ...oby('r1', 5), jobs: ['j'], ended: true };
    expect(cards([words('u1', 1), oby('o1', 2), report])).toEqual({ u1: [], o1: ['j'], r1: [] });
  });
});
