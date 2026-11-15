import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { acting, actionOn, added, adopted, answered, awaited, awaitedIn, confirmed, failed, gone, settled, unacted, type Action, type Pending } from '../pending';
import type { Conversation, Turn } from '../../../shared/obid-wire';
import { liveBridge, type Listed } from './liveBridge';

// Words to Oby as a pending overlay keyed by their turn id (docs/contracts/live-state.md, LIVE-15 to
// LIVE-18): pending.ts's rules on their own, then the page through main's bridge.

const convo = (id: string): Conversation => ({ id, title: id, pinned: false, created: '2026-11-01T12:00:00Z', lastActivity: '2026-11-01T12:00:00Z', cursor: '' });
const turn = (id: string, who: 'user' | 'oby', conversation: string, more: Partial<Turn> = {}): Turn => ({ id, who, text: id, at: '2026-11-01T12:00:00Z', jobs: [], conversation, ...more });
const entry = (more: Partial<Pending> = {}) => ({ id: 't1', shown: 'new:1', conversation: null, said: 'Fix the login', files: [], ...more });
const answers = (t: Turn) => t.who === 'oby' && !t.ended;

describe('pending.ts', () => {
  it('an entry is added on its way, and nothing changes its input', () => {
    const none = {};
    const p = added(none, entry());
    expect(none).toEqual({});
    expect(p.t1).toEqual({ ...entry(), coming: false, over: false });
    expect([awaited(p, 't1'), awaitedIn(p, 'new:1', 'cA'), awaitedIn(p, 'c9', 'c9')]).toEqual([true, true, false]);
  });

  it('the plan\'s answer: coming keeps it, none takes it away unless it\'s the same words again', () => {
    const p = added({}, entry({ shown: 'c1', conversation: 'c1' }));
    expect(answered(p, 't1', { kind: 'reply', conversation: 'c1' }, { conversations: [] }).t1.coming).toBe(true);
    expect(answered(p, 't1', { kind: 'none', repeated: true }, { conversations: [] }).t1.coming).toBe(true);
    expect(answered(p, 't1', { kind: 'none' }, { conversations: [] })).toEqual({});
    expect(answered(p, 'other', { kind: 'none' }, { conversations: [] })).toBe(p);
  });

  it('words starting a conversation stay, over, until obid lists it; with none named they go', () => {
    const p = added({}, entry());
    const over = answered(p, 't1', { kind: 'none', conversation: 'cA' }, { conversations: [] });
    expect(over.t1).toMatchObject({ conversation: 'cA', over: true });
    expect(awaited(over, 't1')).toBe(false);
    expect(settled(over, { turns: {}, conversations: [] }, answers)).toBe(over);
    expect(settled(over, { turns: {}, conversations: [convo('cA')] }, answers)).toEqual({});
    expect(failed(p, 't1', { conversations: [] })).toEqual({});
  });

  it('settled: the entry learns its conversation from its own turn, and leaves at Oby\'s answer after it', () => {
    const p = added({}, entry({ shown: 'c1', conversation: null }));
    const mine = settled(p, { turns: { c1: [turn('o0', 'oby', 'c1'), turn('t1', 'user', 'c1')] }, conversations: [convo('c1')] }, answers);
    expect(mine.t1.conversation).toBe('c1');
    const report = settled(mine, { turns: { c1: [turn('t1', 'user', 'c1'), turn('r', 'oby', 'c1', { ended: true })] }, conversations: [convo('c1')] }, answers);
    expect(report).toBe(mine);
    expect(settled(mine, { turns: { c1: [turn('t1', 'user', 'c1'), turn('o1', 'oby', 'c1')] }, conversations: [convo('c1')] }, answers)).toEqual({});
  });

  it('gone: obid deleting a conversation takes the words to it', () => {
    const p = added(added({}, entry({ id: 'a', shown: 'c1', conversation: 'c1' })), entry({ id: 'b', shown: 'c2', conversation: 'c2' }));
    expect(Object.keys(gone(p, 'c1'))).toEqual(['b']);
    expect(gone(p, 'c3')).toBe(p);
  });

  it('adopted: only the entry\'s own conversation maps onto its app id, and onto no other', () => {
    const e = { ...entry(), coming: false, over: false };
    const alias = { cX: 'new:9' };
    expect(adopted(alias, e, 'cA')).toEqual({ cX: 'new:9', cA: 'new:1' });
    expect(adopted({ cB: 'new:1' }, e, 'cA')).toEqual({ cA: 'new:1' });
    expect(adopted(alias, undefined, 'cA')).toBe(alias);
    expect(adopted(alias, { ...e, shown: 'c1' }, 'cA')).toBe(alias);
  });

  it('actions: an entry stays while obid holds what it acts on at the same revision, and leaves once newer, gone or failed', () => {
    const item = { revision: 3 };
    const a: Action = { what: 'answer', in: 'inbox', on: 'i1', of: item, option: 'Allow' };
    const p = acting({}, a);
    expect(actionOn(p, 'answer', 'i1')).toBe(a);
    expect(confirmed(p, () => ({ revision: 3 }))).toBe(p);
    expect(confirmed(p, () => ({ revision: 4 }))).toEqual({});
    expect(confirmed(p, () => undefined)).toEqual({});
    // with no revision, only another copy is newer
    const old: Action = { ...a, of: {} };
    expect(confirmed(acting({}, old), () => old.of)).toEqual(acting({}, old));
    expect(confirmed(acting({}, old), () => ({}))).toEqual({});
    // a failed call takes its own entry, never a later click's
    const again = { ...a, option: 'Deny' };
    expect(unacted(acting(p, again), a)).toEqual(acting({}, again));
    expect(unacted(p, a)).toEqual({});
  });
});

// The page, through main's bridge: obid as the test answers it, each answer held until it lets it go
type Answer = { status: number; body?: unknown; hold?: Promise<unknown> };
type Call = { method: string; path: string; body?: unknown };
const calls: Call[] = [];
let plan: (n: number) => Answer = () => ({ status: 404 });
let routes: Record<string, Answer> = {};
let bridge = liveBridge();
const flush = () => new Promise((r) => setTimeout(r, 0));
const plans = () => calls.filter((c) => c.path === '/v1/plans');
function memory(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); },
  };
}
function held() {
  let go!: () => void;
  const hold = new Promise<void>((done) => { go = done; });
  return { hold, go };
}
async function load(conversations: Conversation[] = [], listed: Omit<Listed, 'conversations'> = {}) {
  vi.resetModules();
  calls.length = 0;
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', memory());
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const a: Answer = routes[`${method} ${path}`] ?? (path === '/v1/plans' ? plan(plans().length) : path.startsWith('/v1/conversation?') ? { status: 200, body: [] } : { status: 404, body: { error: 'Not found.' } });
    await a.hold;
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  bridge = liveBridge(call);
  vi.stubGlobal('window', { app: { obid: { call, confirm: async () => ({ status: 200, text: '{}' }), ...bridge.obid } } });
  const live = await import('../live');
  const chats = await import('../chats');
  const tasks = await import('../tasks');
  live.connect();
  bridge.sync({ ...listed, conversations });
  for (let i = 0; i < 5; i++) await flush();
  const rows = () => chats.dev!.get();
  const row = (id: string) => rows().find((c) => c.id === id);
  const sentAs = () => vi.waitFor(() => { expect(plans().length).toBeGreaterThan(0); return (plans()[0].body as { id: string }).id; }, { timeout: 5000 });
  const task = (id: string) => tasks.dev!.get().find((t) => t.id === id)!;
  return { live, rows, row, sentAs, task, asks: () => tasks.asksWaiting().map((a) => a.id) };
}

describe('words to Oby as a pending overlay', { timeout: 15_000 }, () => {
  beforeEach(() => { plan = () => ({ status: 404 }); });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('LIVE-15: obid\'s turn with the sent id adopts its conversation, whatever its text', async () => {
    const { hold, go } = held();
    plan = () => ({ status: 200, body: { kind: 'reply', text: 'On it', jobs: [], conversation: 'cA' }, hold });
    const { live, rows, row, sentAs } = await load();
    expect(live.ask('Fix the login')).toBe('new:1');
    const id = await sentAs();
    bridge.event('conversation', convo('cA'));
    bridge.event('turn', turn(id, 'user', 'cA', { text: 'In deskree-inc/oby: Fix the login' }));
    expect(live.realId('new:1')).toBe('cA');
    expect(row('new:1')!.said.map((m) => m.text)).toEqual(['In deskree-inc/oby: Fix the login']);
    expect(rows().filter((c) => c.id === 'cA')).toEqual([]);
    expect(live.liveState().pending[id]).toBeDefined();
    bridge.event('turn', turn('o1', 'oby', 'cA', { text: 'On it' }));
    expect(live.liveState().pending[id]).toBeUndefined();
    expect(row('new:1')!.said.map((m) => m.text)).toEqual(['In deskree-inc/oby: Fix the login', 'On it']);
    go();
    for (let i = 0; i < 5; i++) await flush();
    expect(live.realId('new:1')).toBe('cA');
  });

  it('LIVE-16: another device\'s words of the same text start their own conversation, never this app\'s', async () => {
    const { hold, go } = held();
    plan = () => ({ status: 200, body: { kind: 'reply', text: 'On it', jobs: [], conversation: 'cA' }, hold });
    const { live, rows, row, sentAs } = await load();
    live.ask('Fix the login');
    const id = await sentAs();
    bridge.event('conversation', convo('cB'));
    bridge.event('turn', turn('other', 'user', 'cB', { text: 'Fix the login' }));
    bridge.event('turn', turn('o-other', 'oby', 'cB', { text: 'For the other device' }));
    bridge.event('conversation', convo('cA'));
    bridge.event('turn', turn(id, 'user', 'cA', { text: 'Fix the login' }));
    expect(live.shownId('cB')).toBe('cB');
    expect(live.realId('new:1')).toBe('cA');
    expect(row('cB')!.said.map((m) => m.text)).toEqual(['Fix the login', 'For the other device']);
    expect(row('new:1')!.said.map((m) => m.text)).toEqual(['Fix the login']);
    expect(rows().filter((c) => c.id === 'cA')).toEqual([]);
    go();
    for (let i = 0; i < 5; i++) await flush();
  });

  it('LIVE-17: a send obid didn\'t answer goes again as the very same request, and its words are taken once', async () => {
    plan = (n) => (n === 1 ? { status: 0, body: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' } : { status: 200, body: { kind: 'none', repeated: true, conversation: 'c1', jobs: [] } });
    const { live } = await load([convo('c1')]);
    live.ask('Fix it', 'c1');
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    await flush();
    const id = (plans()[0].body as { id: string }).id;
    expect(live.liveState().pending[id]).toBeDefined();
    await vi.waitFor(() => expect(plans()).toHaveLength(2), { timeout: 3000 });
    expect(plans()[1].body).toEqual(plans()[0].body);
    for (let i = 0; i < 5; i++) await flush();
    expect(live.liveState().pending[id]).toMatchObject({ coming: true });
    bridge.event('turn', turn(id, 'user', 'c1', { text: 'Fix it' }));
    expect(live.liveState().turns.c1.filter((t) => t.who === 'user' && t.id === id)).toHaveLength(1);
    await new Promise((r) => setTimeout(r, 1200));
    expect(plans()).toHaveLength(2);
  });

  describe('LIVE-18: a reconnect never sends words again, and the entry stays until its own end', () => {
    async function sentThenReconnected(answer: Omit<Answer, 'hold'>) {
      const { hold, go } = held();
      plan = () => ({ ...answer, hold });
      const page = await load();
      page.live.ask('Fix the login');
      const id = await page.sentAs();
      bridge.send({ kind: 'connection', connection: 'down', problem: 'Oby’s engine stopped.' });
      bridge.sync({});
      for (let i = 0; i < 5; i++) await flush();
      expect(plans()).toHaveLength(1);
      expect(page.live.liveState().pending[id]).toBeDefined();
      expect(page.row('new:1')!.said.map((m) => m.text)).toEqual(['Fix the login']);
      return { ...page, id, go };
    }

    it('LIVE-18: it leaves once its turn and Oby\'s answer arrive', async () => {
      const { live, id, go } = await sentThenReconnected({ status: 200, body: { kind: 'reply', text: 'On it', jobs: [], conversation: 'cA' } });
      bridge.event('conversation', convo('cA'));
      bridge.event('turn', turn(id, 'user', 'cA', { text: 'Fix the login' }));
      expect(live.liveState().pending[id]).toBeDefined();
      bridge.event('turn', turn('o1', 'oby', 'cA'));
      expect(live.liveState().pending[id]).toBeUndefined();
      go();
      for (let i = 0; i < 5; i++) await flush();
      expect(plans()).toHaveLength(1);
    });

    it('LIVE-18: it leaves once the held answer is let go as a 422', async () => {
      const { live, row, id, go } = await sentThenReconnected({ status: 422, body: { error: 'No.' } });
      go();
      for (let i = 0; i < 5; i++) await flush();
      expect(live.liveState().pending[id]).toBeUndefined();
      expect(row('new:1')).toBeUndefined();
      expect(plans()).toHaveLength(1);
    });
  });
});

// Answers to cards and task actions as pending entries (LIVE-19 to LIVE-21): a job waiting on its
// permission, and its inbox item, as obid has them
const now = new Date().toISOString();
const job = (more: Record<string, unknown> = {}) => ({
  id: 'j1', request: 'Fix it', title: 'Fix it', agent: 'claude', account: 'a1', repo: 'o/r', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'running', step: 'Waiting on you', summary: '', created: now, finished: null, commits: 0, published: null,
  rounds: 1, round: 1, check: null, model: null, dismissed: null, pipeline: null, archived: null,
  permission: { id: 'p1', detail: 'rm -rf build' }, revision: 1, ...more,
});
const permission = (more: Record<string, unknown> = {}) => ({
  id: 'j1-permission-p1', kind: 'permission', job: 'j1', title: 'Fix it', text: 'Run rm -rf build?', options: ['Allow', 'Deny'], details: null,
  created: now, state: 'open', later: null, headline: null, recommended: null, revision: 1, ...more,
});

describe('answers to cards and task actions as pending entries', { timeout: 15_000 }, () => {
  beforeEach(() => { routes = {}; });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('LIVE-19: Allow on a job\'s permission card shows answered until obid\'s newer inbox event, then obid\'s state', async () => {
    const { hold, go } = held();
    routes['POST /v1/jobs/j1/permission'] = { status: 200, body: job({ permission: null, step: 'Working', revision: 2 }), hold };
    const { live, task } = await load([], { jobs: [job()], inbox: [permission()] });
    expect(task('j1')).toMatchObject({ status: 'needs', answer: undefined });
    const answering = live.answerJob('j1', 'Allow');
    await flush();
    expect(calls.at(-1)).toMatchObject({ method: 'POST', path: '/v1/jobs/j1/permission', body: { id: 'p1', decision: 'allow' } });
    expect(task('j1')).toMatchObject({ status: 'running', answer: 'Allow' });
    expect(task('j1').job.ask).toMatchObject({ kind: 'permission', detail: 'rm -rf build' });
    expect(Object.keys(live.liveState().actions)).toEqual(['answer:j1-permission-p1']);
    // the same revision changes nothing; obid's newer copy settles it
    bridge.event('inbox', permission());
    expect(task('j1').answer).toBe('Allow');
    bridge.event('inbox', permission({ state: 'done', revision: 2 }));
    expect(live.liveState().actions).toEqual({});
    expect(task('j1')).toMatchObject({ status: 'running', answer: undefined });
    expect(task('j1').job.ask).toBeUndefined();
    go();
    await answering;
    expect(task('j1').job.step).toBe('Working');
  });

  it('LIVE-20: a Stop that fails shows the job as obid has it, and the reason', async () => {
    const { hold, go } = held();
    routes['POST /v1/jobs/j1/stop'] = { status: 409, body: { error: 'It already ended.' }, hold };
    const { live, task } = await load([], { jobs: [job({ permission: null, step: 'Working' })] });
    const stopping = live.stopJob('j1');
    await flush();
    expect(Object.keys(live.liveState().actions)).toEqual(['stop:j1']);
    expect(task('j1').job).toMatchObject({ status: 'running', step: 'Working' });
    // obid's job moves on meanwhile; the card follows it
    bridge.event('job', job({ permission: null, step: 'Checking', revision: 2 }));
    go();
    await stopping;
    expect(live.liveState().actions).toEqual({});
    expect(task('j1').job).toMatchObject({ status: 'running', step: 'Checking' });
    expect(live.liveState().problem).toBe('It already ended.');
  });

  it('LIVE-21: obid\'s event before the call\'s answer: when the answer arrives, nothing changes on the card', async () => {
    const { hold, go } = held();
    routes['POST /v1/jobs/j1/permission'] = { status: 200, body: job({ permission: null, step: 'Allowed', revision: 2 }), hold };
    const { live, task } = await load([], { jobs: [job()], inbox: [permission()] });
    const answering = live.answerJob('j1', 'Allow');
    await flush();
    bridge.event('inbox', permission({ state: 'done', revision: 2 }));
    bridge.event('job', job({ permission: null, status: 'done', step: 'Done', finished: now, revision: 3 }));
    // the card as shown, but for its minutes ago, which move with the clock
    const shown = () => { const t = task('j1'); return JSON.stringify({ ...t, job: { ...t.job, created: 0, finished: 0 } }); };
    const card = shown();
    expect(task('j1')).toMatchObject({ answer: undefined, job: { status: 'done', step: 'Done' } });
    go();
    await answering;
    expect(shown()).toBe(card);
    expect(live.liveState().actions).toEqual({});
  });

  // another Oby's question, answered from Home's stack (QuestionCard.tsx)
  const question = (more: Record<string, unknown> = {}) => ({
    id: 'q1', kind: 'obiRequest', job: '', title: 'A question', text: 'dana asks', options: [], details: null, created: now, state: 'open',
    later: null, headline: null, recommended: null, obi: { type: 'question', fromOwner: 'dana', quoted: { text: 'Billing retries?' } }, revision: 1, ...more,
  });

  it('an answer to another Oby\'s question keeps its card while on its way, and leaves with obid\'s newer copy', async () => {
    const { hold, go } = held();
    routes['POST /v1/inbox/q1/answer'] = { status: 200, body: question({ state: 'done', revision: 2 }), hold };
    const { live, asks } = await load([], { inbox: [question()] });
    const answering = live.answerQuestion('q1', 'Yes, weekly.');
    await flush();
    expect(Object.keys(live.liveState().actions)).toEqual(['reply:q1']);
    expect(asks()).toEqual(['q1']);
    go();
    await answering;
    expect(live.liveState().actions).toEqual({});
    expect(asks()).toEqual([]);
  });

  it('an answer to another Oby\'s question obid refuses: its entry goes, the card stays, and the refusal is the caller\'s', async () => {
    routes['POST /v1/inbox/q1/answer'] = { status: 422, body: { error: 'It was called off.' } };
    const { live, asks } = await load([], { inbox: [question()] });
    await expect(live.answerQuestion('q1', 'Yes, weekly.')).rejects.toThrow('It was called off.');
    expect(live.liveState().actions).toEqual({});
    expect(asks()).toEqual(['q1']);
  });
});
