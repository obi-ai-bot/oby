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

describe('BYPASS-6: a task’s Mode menu', () => {
  it('bypass_6_changes_the_jobs_mode_through_obid', async () => {
    call.mockResolvedValue(answered({ id: 'j1', mode: 'bypass' }));
    await live.changeMode('j1', 'bypass');
    expect(call).toHaveBeenCalledWith('POST', '/v1/jobs/j1/mode', { mode: 'bypass' });
    expect(live.liveState().jobs.j1.mode).toBe('bypass');
  });
});
