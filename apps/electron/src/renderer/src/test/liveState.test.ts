import { describe, expect, it } from 'vitest';
import { answer, answerStatus, answerTurns, apply, synced, type ObidEvent, type ObidState, type Read } from '../../../shared/live-state';
import { parseEvent, type Account, type Conversation, type Event, type GitHubWork, type InboxItem, type Job, type Status, type Turn } from '../../../shared/obid-wire';

// The one rule what obid has reaches the app's state by (docs/contracts/live-state.md, LIVE-1 to LIVE-9)
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'running', step: 'Working', summary: '', created: '2026-10-30T10:00:00Z', finished: null,
  commits: 0, published: null, rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null,
  title: 'Fix it', ...more,
});
const empty: ObidState = { jobs: {}, gone: new Set(), status: null, github: null, inbox: [], conversations: [], turns: {}, goneConversations: new Set(), settledInbox: {} };
const holding = (j: Job): ObidState => ({ ...empty, jobs: { [j.id]: j } });
const turn = (more: Partial<Turn> = {}): Turn => ({ id: 't1', who: 'oby', text: 'Done.', at: '2026-10-30T10:00:00Z', jobs: [], conversation: 'c1', ...more });
const convo = (more: Partial<Conversation> = {}): Conversation => ({
  id: 'c1', title: 'Fix it', pinned: false, created: '2026-10-30T10:00:00Z', lastActivity: '2026-10-30T10:00:00Z', cursor: '', ...more,
});
const item = (more: Partial<InboxItem> = {}): InboxItem => ({
  id: 'i1', kind: 'question', job: 'j1', title: 'Which one?', text: 'Which one?', options: ['This', 'That'], details: null,
  created: '2026-10-30T10:00:00Z', state: 'open', later: null, headline: null, recommended: null, ...more,
});
const status = (accounts: Account[] = []): Status => ({ runner: { version: '0.2.4', host: 'mac' }, accounts, models: {}, maxRunning: 6, running: 0 });
const work = (signedIn = true): GitHubWork => ({ signedIn, error: null, reviews: [], assigned: [], mine: [] });
const read = (more: Partial<Read> = {}): Read => ({ jobs: [], conversations: [], complete: true, inbox: [], status: null, github: null, ...more });

describe('live state', () => {
  it('LIVE-1: an older job never replaces a newer one, from an event or an answer', () => {
    const held = holding(job({ revision: 5, step: 'Checking' }));
    const older = job({ revision: 4, step: 'Working' });
    expect(apply(held, { kind: 'job', job: older }).jobs.j1).toMatchObject({ revision: 5, step: 'Checking' });
    expect(answer(held, older).jobs.j1).toMatchObject({ revision: 5, step: 'Checking' });
    // the newer one still comes in, both ways
    expect(apply(held, { kind: 'job', job: job({ revision: 6 }) }).jobs.j1.revision).toBe(6);
    expect(answer(held, job({ revision: 6 })).jobs.j1.revision).toBe(6);
  });

  it('LIVE-2: a deleted job never comes back', () => {
    const deleted = parseEvent('job', JSON.stringify({ id: 'j1', deleted: true }));
    expect(deleted).toEqual({ kind: 'jobGone', id: 'j1' });
    const s = apply(holding(job({ revision: 5 })), deleted as Extract<Event, { kind: 'jobGone' }>);
    expect(s.jobs.j1).toBeUndefined();
    expect(answer(s, job({ revision: 9 })).jobs.j1).toBeUndefined();
    expect(apply(s, { kind: 'job', job: job({ revision: 9 }) }).jobs.j1).toBeUndefined();
    expect(apply(s, { kind: 'job', job: job() }).jobs.j1).toBeUndefined();
  });

  it('LIVE-3: a job with no revision is taken as it comes', () => {
    const plain = job({ step: 'From an older obid' });
    expect(apply(holding(job({ revision: 5 })), { kind: 'job', job: plain }).jobs.j1).toBe(plain);
    expect(answer(holding(job({ revision: 5 })), plain).jobs.j1).toBe(plain);
    expect(apply(empty, { kind: 'job', job: plain }).jobs.j1).toBe(plain);
    expect(answer(empty, plain).jobs.j1).toBe(plain);
  });

  it('LIVE-4: the same events from the same state give the same state, and the state they started from is unchanged', () => {
    const start: ObidState = {
      ...empty, jobs: { j1: job({ revision: 2 }), j2: job({ id: 'j2', revision: 1 }) }, gone: new Set(['j9']), status: status(),
      conversations: [convo({ revision: 2 }), convo({ id: 'c2' })], turns: { c1: [turn({ revision: 1 })], c2: [turn({ id: 't2', conversation: 'c2' })] },
      inbox: [item({ revision: 1 })], goneConversations: new Set(['c9']), settledInbox: { i2: 3 },
    };
    const before = structuredClone(start);
    const events: ObidEvent[] = [
      { kind: 'conversation', conversation: convo({ revision: 3, title: 'Fix it now' }) },
      { kind: 'conversation', conversation: convo({ revision: 1 }) },
      { kind: 'conversationGone', id: 'c2' },
      { kind: 'turn', turn: turn({ revision: 2, text: 'Done, with tests.' }) },
      { kind: 'turnAsks', turn: turn({ id: 't3', at: '2026-10-30T09:00:00Z' }) },
      { kind: 'turn', turn: turn({ id: 't2', conversation: 'c2' }) },
      { kind: 'inbox', item: item({ revision: 2, state: 'done' }) },
      { kind: 'inbox', item: item({ id: 'i2', revision: 2 }) },
      { kind: 'accounts', accounts: [] },
      { kind: 'github', work: work() },
      { kind: 'job', job: job({ revision: 3, step: 'Checking' }) },
      { kind: 'job', job: job({ revision: 1 }) },
      { kind: 'jobGone', id: 'j2' },
      { kind: 'job', job: job({ id: 'j2', revision: 7 }) },
      { kind: 'job', job: job({ id: 'j3' }) },
      { kind: 'job', job: job({ id: 'j9', revision: 1 }) },
    ];
    const once = events.reduce(apply, start), twice = events.reduce(apply, start);
    expect(twice).toEqual(once);
    expect(start).toEqual(before);
    expect(Object.keys(once.jobs).sort()).toEqual(['j1', 'j3']);
    expect(once.jobs.j1).toMatchObject({ revision: 3, step: 'Checking' });
    expect([...once.gone].sort()).toEqual(['j2', 'j9']);
    expect(once.conversations).toEqual([convo({ revision: 3, title: 'Fix it now' })]);
    expect(once.turns).toEqual({ c1: [turn({ id: 't3', at: '2026-10-30T09:00:00Z' }), turn({ revision: 2, text: 'Done, with tests.' })] });
    expect([...once.goneConversations].sort()).toEqual(['c2', 'c9']);
    expect(once.inbox).toEqual([]);
    expect(once.settledInbox).toEqual({ i1: 2, i2: 3 });
    expect(once.github).toEqual(work());
  });

  it('LIVE-5: an older turn from GET /v1/conversation never replaces a newer one its event brought', () => {
    const held = apply(empty, { kind: 'turn', turn: turn({ revision: 3, text: 'Done, with tests.' }) });
    const older = turn({ revision: 2, text: 'Done.' });
    expect(answerTurns(held, 'c1', [older]).turns.c1).toEqual([turn({ revision: 3, text: 'Done, with tests.' })]);
    expect(apply(held, { kind: 'turn', turn: older }).turns.c1).toEqual([turn({ revision: 3, text: 'Done, with tests.' })]);
    // the newer one still comes in, both ways; one with no conversation of its own goes under the one asked for
    expect(answerTurns(held, 'c1', [turn({ revision: 4, text: 'Merged.' })]).turns.c1).toEqual([turn({ revision: 4, text: 'Merged.' })]);
    expect(apply(held, { kind: 'turn', turn: turn({ revision: 4 }) }).turns.c1).toEqual([turn({ revision: 4 })]);
    const { conversation: _, ...bare } = turn({ revision: 4, text: 'Merged.' });
    expect(answerTurns(held, 'c1', [bare]).turns.c1).toEqual([bare]);
  });

  it('LIVE-6: an older answer of the Mac\'s doesn\'t undo the iPhone\'s newer one on an inbox item', () => {
    const macs = item({ revision: 3, state: 'open' });
    const open = apply(empty, { kind: 'inbox', item: item({ revision: 2 }) });
    // the iPhone's answer settled it, or put it off with its answer, or changed it: each at revision 4
    for (const iPhones of [item({ revision: 4, state: 'done' }), item({ revision: 4, state: 'later', later: '2026-10-30T11:00:00Z' }), item({ revision: 4, text: 'Which one, now?' })]) {
      const held = apply(open, { kind: 'inbox', item: iPhones });
      expect(apply(held, { kind: 'inbox', item: macs })).toEqual(held);
      expect(held.inbox).toEqual(iPhones.state === 'done' ? [] : [iPhones]);
    }
    const settled = apply(open, { kind: 'inbox', item: item({ revision: 4, state: 'done' }) });
    expect(settled.settledInbox).toEqual({ i1: 4 });
    // a newer one still comes in, and takes the mark away
    const reopened = apply(settled, { kind: 'inbox', item: item({ revision: 5 }) });
    expect(reopened.inbox).toEqual([item({ revision: 5 })]);
    expect(reopened.settledInbox).toEqual({});
  });

  it('LIVE-7: a deleted conversation never comes back, nor its turns', () => {
    const start = answerTurns(apply(empty, { kind: 'conversation', conversation: convo({ revision: 2 }) }), 'c1', [turn({ revision: 1 })]);
    const deleted = parseEvent('conversation', JSON.stringify({ id: 'c1', deleted: true, revision: 3 }));
    expect(deleted).toEqual({ kind: 'conversationGone', id: 'c1' });
    const s = apply(start, deleted as Extract<Event, { kind: 'conversationGone' }>);
    expect(s.conversations).toEqual([]);
    expect(s.turns.c1).toBeUndefined();
    for (const c of [convo({ revision: 9 }), convo()]) expect(apply(s, { kind: 'conversation', conversation: c }).conversations).toEqual([]);
    const after = synced(s, s, read({ conversations: [convo({ revision: 9 })], complete: true }));
    expect(after.conversations).toEqual([]);
    expect(after.turns.c1).toBeUndefined();
    expect(apply(after, { kind: 'turn', turn: turn({ revision: 5 }) }).turns.c1).toBeUndefined();
    expect(apply(after, { kind: 'turnAsks', turn: turn() }).turns.c1).toBeUndefined();
    expect(answerTurns(after, 'c1', [turn({ revision: 5 })]).turns.c1).toBeUndefined();
  });

  it('LIVE-8: back after a reconnect, obid\'s lists and the events that came while they were read give obid\'s newest copy of each', () => {
    const before: ObidState = {
      ...empty, status: status(),
      conversations: [convo({ id: 'cA', revision: 5 }), convo({ id: 'cB', revision: 7 }), convo({ id: 'cC', revision: 1 })],
      turns: { cA: [turn({ id: 'tA', conversation: 'cA', revision: 1 })] },
      inbox: [item({ id: 'iA', revision: 2 })], settledInbox: { iX: 9 },
    };
    // while the lists were read, an answer brought newer copies of cA and iA
    const held = apply(apply(before, { kind: 'conversation', conversation: convo({ id: 'cA', revision: 6, title: 'Renamed' }) }),
      { kind: 'inbox', item: item({ id: 'iA', revision: 3, state: 'later', later: '2026-10-30T11:00:00Z' }) });
    // obid's lists hold those at a lower revision, and cB lower than its unchanged copy (obid restarted)
    const listed = read({
      conversations: [convo({ id: 'cA', revision: 4 }), convo({ id: 'cB', revision: 2, title: 'After the restart' }), convo({ id: 'cC', revision: 1 })],
      inbox: [item({ id: 'iA', revision: 1 }), item({ id: 'iX', revision: 2 })], status: status(), github: work(),
    });
    const pending: ObidEvent[] = [
      { kind: 'turn', turn: turn({ id: 'tA', conversation: 'cA', revision: 2, text: 'Done, with tests.' }) },
      { kind: 'conversationGone', id: 'cC' },
    ];
    expect(pending.reduce(apply, synced(held, before, listed))).toEqual({
      jobs: {}, gone: new Set(), status: status(), github: work(),
      conversations: [convo({ id: 'cA', revision: 6, title: 'Renamed' }), convo({ id: 'cB', revision: 2, title: 'After the restart' })],
      turns: { cA: [turn({ id: 'tA', conversation: 'cA', revision: 2, text: 'Done, with tests.' })] },
      inbox: [item({ id: 'iA', revision: 3, state: 'later', later: '2026-10-30T11:00:00Z' }), item({ id: 'iX', revision: 2 })],
      goneConversations: new Set(['cC']), settledInbox: {},
    });
  });

  it('LIVE-9: a conversation, a turn or an inbox item with no revision is taken as it comes, in the order it comes', () => {
    const held = answerTurns({ ...empty, conversations: [convo({ revision: 5 })], inbox: [item({ revision: 5 })] }, 'c1', [turn({ revision: 5 })]);
    const c = convo({ title: 'From an older obid' }), t = turn({ text: 'From an older obid' }), i = item({ text: 'From an older obid' });
    for (const s of [held, empty]) {
      expect(apply(s, { kind: 'conversation', conversation: c }).conversations[0]).toBe(c);
      expect(apply(s, { kind: 'turn', turn: t }).turns.c1).toEqual([t]);
      expect(answerTurns(s, 'c1', [t]).turns.c1[0]).toBe(t);
      expect(apply(s, { kind: 'inbox', item: i }).inbox[0]).toBe(i);
    }
    // of two, the last that came
    const then = (title: string) => ({ c: convo({ title }), t: turn({ text: title }), i: item({ text: title }) });
    const first = then('First'), second = then('Second');
    const both = [first, second].reduce((s, x) => [
      { kind: 'conversation', conversation: x.c }, { kind: 'turn', turn: x.t }, { kind: 'inbox', item: x.i },
    ].reduce((s, e) => apply(s, e as ObidEvent), s), held);
    expect([both.conversations, both.turns.c1, both.inbox]).toEqual([[second.c], [second.t], [second.i]]);
    // over a settled mark too
    const settled = apply(held, { kind: 'inbox', item: item({ revision: 6, state: 'done' }) });
    expect(apply(settled, { kind: 'inbox', item: i }).inbox).toEqual([i]);
    // and a status, which has none
    expect(answerStatus(held, status()).status).toEqual(status());
  });
});
