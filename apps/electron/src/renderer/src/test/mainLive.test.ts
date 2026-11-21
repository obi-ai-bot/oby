import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { unwired, type LiveMessage, type LiveSnapshot } from '../../../shared/live-wire';

// Main's one follower of obid's events (main/live.ts; docs/contracts/live-state.md, LIVE-10 and 11;
// docs/contracts/alerts.md, ALERTS-11): run for real over Electron's IPC and main/obid.ts stood in
// for, obid answering each read from here and its event stream ours to write to, under fake timers.

type Answer = { status: number; body?: unknown; hold?: Promise<unknown> };
const the = vi.hoisted(() => ({
  handlers: {} as Record<string, (e: unknown, ...args: unknown[]) => unknown>,
  // obid's answer to each read, by path
  answers: {} as Record<string, { status: number; body?: unknown; hold?: Promise<unknown> }>,
  // every read, and every event stream asked for, by path
  reads: [] as string[],
  streams: [] as string[],
  // how obid answers the next stream: open (ours to write to), or as given
  answer: 'open' as 'open' | { status: number; text: string; why?: 'waiting' } | Response,
  stream: null as ReadableStreamDefaultController<Uint8Array> | null,
  window: true,
  page: [] as unknown[],
}));
vi.mock('electron', () => ({ ipcMain: { on() {}, handle: (name: string, f: (e: unknown, ...args: unknown[]) => unknown) => { the.handlers[name] = f; } } }));
vi.mock('../../../main/obid', () => ({
  obid: async (_method: string, path: string) => {
    the.reads.push(path);
    const a = the.answers[path] ?? { status: 404, body: { error: 'Not found.' } };
    await a.hold;
    return { status: a.status, text: typeof a.body === 'string' ? a.body : JSON.stringify(a.body) };
  },
  send: async (_method: string, path: string) => {
    the.streams.push(path);
    if (the.answer !== 'open') return the.answer;
    return new Response(new ReadableStream<Uint8Array>({ start: (c) => { the.stream = c; } }));
  },
  takesKeys: async () => false,
  streamEnded: () => {},
  reason: (e: unknown) => ({ status: 0, text: String(e) }),
  chooseProfile: () => false, openedComputer: () => null, onComputer: () => {}, heardComputers: () => {},
  openedProfile: () => null,
}));

const job = (more: Record<string, unknown> = {}) => ({
  id: 'j1', request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'done', step: 'Done', summary: '', created: '2026-10-30T10:00:00Z', finished: '2026-10-30T10:20:00Z',
  commits: 1, published: null, rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null,
  title: 'Fix it', archived: null, ...more,
});
const turn = (id: string, more: Record<string, unknown> = {}) => ({ id, who: 'oby', text: id, at: '2026-10-30T10:00:00Z', jobs: [], conversation: 'c1', ...more });
const STATUS = { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0 };

// obid with these lists, and nothing else
function obidHas(jobs: unknown[] = [], more: Record<string, Answer> = {}) {
  Object.assign(the.answers, {
    '/v1/status': { status: 200, body: STATUS }, '/v1/jobs?limit=100': { status: 200, body: jobs }, '/v1/inbox': { status: 200, body: [] },
    '/v1/conversations?limit=200': { status: 200, body: [] }, '/v1/limit-decisions': { status: 200, body: [] }, ...more,
  });
}
// a macrotask: every microtask before it (the stream's text, obid's answers) has run
const settle = async () => { for (let i = 0; i < 5; i++) await new Promise((r) => setImmediate(r)); };
const event = (name: string, data: unknown) => the.stream!.enqueue(new TextEncoder().encode(`event: ${name}\ndata: ${JSON.stringify(data)}\n\n`));
// what main sent the window's page since `from`
const sent = (from = 0) => (the.page as LiveMessage[]).slice(from);
const snapshot = () => the.handlers['obid-snapshot']({}) as LiveSnapshot;
const load = (what: unknown) => the.handlers['obid-load']({}, what) as Promise<true | string>;
const jobsHeld = () => Object.keys(unwired(snapshot().state).jobs);

// Main's modules fresh, launched as index.ts launches them, and synced
async function launch() {
  vi.resetModules();
  const live = await import('../../../main/live');
  const page = { isDestroyed: () => false, webContents: { send: (channel: string, m: unknown) => { if (channel === 'obid-live') the.page.push(m); } } };
  live.living(() => true, () => (the.window ? page as never : null));
  await settle();
  return live;
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'] });
  Object.assign(the, { handlers: {}, answers: {}, reads: [], streams: [], answer: 'open', stream: null, window: true, page: [] });
});
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe('ALERTS-11', () => {
  it('alerts_11_a_window_that_opens_again_gets_mains_state_at_once_with_no_new_stream', async () => {
    obidHas([job()]);
    await launch();
    expect(snapshot()).toMatchObject({ connection: 'live', problem: null });
    const reads = the.reads.length, told = the.page.length;
    // the window closes; main still follows, and keeps what comes
    the.window = false;
    event('job', job({ id: 'j2' }));
    await settle();
    expect(the.page.length).toBe(told);
    // it opens again: its page asks, and main answers what it holds, at once
    the.window = true;
    const again = snapshot();
    expect(again).toMatchObject({ connection: 'live', problem: null, complete: true });
    expect(Object.keys(unwired(again.state).jobs)).toEqual(['j1', 'j2']);
    expect(again.seq).toBe(told + 1);
    await settle();
    expect(the.streams).toEqual(['/v1/events?alerts=1']);
    expect(the.reads.length).toBe(reads);
  });
});

describe('LIVE-10', () => {
  it('live_10_a_conversation_the_page_loads_goes_into_mains_state_and_the_window_gets_it', async () => {
    obidHas([job({ id: 'j1' })]);
    await launch();
    // a turn held at a higher revision than the read will bring
    event('turn', turn('t1', { text: 'newer', revision: 3 }));
    await settle();
    const from = the.page.length;
    the.answers['/v1/conversation?id=c1&limit=100'] = { status: 200, body: [turn('t1', { text: 'older', revision: 2, jobs: ['j1', 'j0'] }), turn('t2', { jobs: ['j9'] })] };
    the.answers['/v1/jobs/j0'] = { status: 200, body: job({ id: 'j0', created: '2026-10-01T10:00:00Z' }) };
    the.answers['/v1/jobs/j9'] = { status: 200, body: job({ id: 'j9' }) };
    // j9 deleted meanwhile: never read back
    event('job', { id: 'j9', deleted: true });
    await settle();
    expect(await load({ conversation: 'c1' })).toBe(true);
    expect(the.reads.filter((p) => p.startsWith('/v1/conversation?') || p.startsWith('/v1/jobs/'))).toEqual(['/v1/conversation?id=c1&limit=100', '/v1/jobs/j0']);
    const held = unwired(snapshot().state);
    expect(held.turns.c1.map((t) => [t.id, t.text])).toEqual([['t1', 'newer'], ['t2', 't2']]);
    expect(Object.keys(held.jobs).sort()).toEqual(['j0', 'j1']);
    const messages = sent(from).filter((m) => m.kind !== 'event' || m.event.kind !== 'jobGone');
    expect(messages.map((m) => (m.kind === 'event' ? `event ${m.event.kind}` : m.kind))).toEqual(['turns', 'event job']);
    expect(messages[0]).toMatchObject({ conversation: 'c1', turns: [{ id: 't1', text: 'older' }, { id: 't2' }] });
    expect(messages[1]).toMatchObject({ event: { job: { id: 'j0' } } });
  });

  it('live_10: a job, and the limit decisions, read for the page; what obid refuses says why', async () => {
    obidHas([job()]);
    await launch();
    const reads = the.reads.length;
    // held: nothing to read
    expect(await load({ job: 'j1' })).toBe(true);
    expect(the.reads.length).toBe(reads);
    the.answers['/v1/jobs/j%2F2'] = { status: 200, body: job({ id: 'j/2' }) };
    expect(await load({ job: 'j/2' })).toBe(true);
    expect(jobsHeld().sort()).toEqual(['j/2', 'j1']);
    the.answers['/v1/limit-decisions'] = { status: 200, body: [{ id: 'd1', revision: 1, jobs: [], options: [] }] };
    expect(await load({ limits: true })).toBe(true);
    expect(snapshot().limits).toEqual([{ id: 'd1', revision: 1, jobs: [], options: [] }]);
    expect(sent().at(-1)).toMatchObject({ kind: 'limits' });
    expect(await load({ conversation: 'gone' })).toBe('Not found.');
    the.answers['/v1/conversation?id=c2&limit=100'] = { status: 0, body: 'obid isn’t running.' };
    expect(await load({ conversation: 'c2' })).toBe('obid isn’t running.');
  });

  it('an item that comes as an event for a job main hasn’t got: main reads it and passes it on', async () => {
    obidHas();
    await launch();
    the.answers['/v1/jobs/old'] = { status: 200, body: job({ id: 'old' }) };
    const from = the.page.length;
    event('inbox', { id: 'i', kind: 'permission', job: 'old', title: 'Run this?', text: '', options: [], created: '2026-10-30T10:00:00Z', state: 'open' });
    await settle();
    expect(sent(from).map((m) => m.kind === 'event' && m.event.kind)).toEqual(['inbox', 'job']);
    expect(jobsHeld()).toEqual(['old']);
  });

  it('the events that came during the sync follow its snapshot, in order, and nothing of a session is kept', async () => {
    let go!: () => void;
    obidHas([job()], { '/v1/inbox': { status: 200, body: [], hold: new Promise<void>((done) => { go = done; }) } });
    await launch();
    event('job', job({ status: 'running', revision: 2 }));
    event('transcript', { job: 'j1', entry: { n: 1, at: '2026-10-30T10:00:00Z', kind: 'said', summary: 'Hi' } });
    await settle();
    expect(sent()).toEqual([]);
    go();
    await settle();
    expect(sent().map((m) => (m.kind === 'event' ? m.event.kind : m.kind))).toEqual(['snapshot', 'job', 'transcript']);
    expect(unwired(snapshot().state).jobs.j1.status).toBe('running');
    expect(JSON.stringify(snapshot().state)).not.toContain('Hi');
  });
});

describe('LIVE-11', () => {
  it('live_11_when_obids_stream_ends_the_window_shows_down_and_main_reconnects_with_the_same_back_off', async () => {
    obidHas([job()]);
    await launch();
    the.stream!.close();
    await settle();
    expect(sent().at(-1)).toMatchObject({ kind: 'connection', connection: 'down', problem: 'obid closed the event stream.' });
    expect(snapshot()).toMatchObject({ connection: 'down', problem: 'obid closed the event stream.' });
    // after a good sync: a second
    vi.advanceTimersByTime(999);
    expect(the.streams).toHaveLength(1);
    vi.advanceTimersByTime(1);
    await settle();
    expect(the.streams).toHaveLength(2);
    expect(snapshot().connection).toBe('live');

    // obid's reads fail: 1 s, then 2 s, then 4 s
    the.answers['/v1/status'] = { status: 503, body: { error: 'Oby’s engine didn’t start.' } };
    the.stream!.close();
    await settle();
    for (const wait of [1000, 2000, 4000]) {
      const n = the.streams.length;
      vi.advanceTimersByTime(wait - 1);
      await settle();
      expect(the.streams).toHaveLength(n);
      vi.advanceTimersByTime(1);
      await settle();
      expect(the.streams).toHaveLength(n + 1);
      expect(sent().at(-1)).toMatchObject({ kind: 'connection', connection: 'down', problem: 'Oby’s engine didn’t start.' });
    }
  });

  it('live_11: obid refusing the token, and a refusal in obid’s own words', async () => {
    the.answer = new Response('', { status: 401 });
    await launch();
    expect(sent().at(-1)).toMatchObject({ kind: 'connection', connection: 'down', problem: 'obid refused this Mac’s token (api-token).' });
    the.answer = new Response(JSON.stringify({ error: 'No.' }), { status: 500 });
    vi.advanceTimersByTime(1000);
    await settle();
    expect(sent().at(-1)).toMatchObject({ connection: 'down', problem: 'No.' });
    the.answer = new Response('nope', { status: 502 });
    vi.advanceTimersByTime(2000);
    await settle();
    expect(sent().at(-1)).toMatchObject({ connection: 'down', problem: 'obid answered 502.' });
  });

  it('live_11: while obid starts, before any status was read, main tries every second', async () => {
    the.answer = { status: 0, text: 'Oby’s engine is starting.', why: 'waiting' };
    await launch();
    expect(sent().at(-1)).toMatchObject({ kind: 'connection', connection: 'down', problem: 'Oby’s engine is starting.', why: 'waiting' });
    for (let n = 2; n <= 5; n++) {
      vi.advanceTimersByTime(1000);
      await settle();
      expect(the.streams).toHaveLength(n);
    }
  });
});

describe('the sync after a reconnect (moved from taskActions.test.ts)', () => {
  // obid's stream ends, and main opens it again with obid listing these
  async function back(jobs: unknown[], more: Record<string, Answer> = {}) {
    obidHas(jobs, more);
    the.stream!.close();
    await settle();
    vi.advanceTimersByTime(1000);
    await settle();
  }

  it('back after the stream was away: a task deleted meanwhile (on the iPhone) leaves, the older ones a conversation fetched stay', async () => {
    obidHas([job({ id: 'j2', created: '2026-10-30T11:00:00Z' }), job()]);
    await launch();
    // one older than obid's latest 100, which an opened conversation fetched
    the.answers['/v1/conversation?id=c1&limit=100'] = { status: 200, body: [turn('t1', { at: '2026-10-01T10:00:00Z', jobs: ['j0'] })] };
    the.answers['/v1/jobs/j0'] = { status: 200, body: job({ id: 'j0', created: '2026-10-01T10:00:00Z' }) };
    expect(await load({ conversation: 'c1' })).toBe(true);
    expect(jobsHeld().sort()).toEqual(['j0', 'j1', 'j2']);

    // j1 deleted while away; obid's 100 newest are j2 and 99 after it
    const latest = [job({ id: 'j2', created: '2026-10-30T11:00:00Z' }), ...Array.from({ length: 99 }, (_, i) => job({ id: `k${i}`, created: `2026-10-29T10:${String(i % 60).padStart(2, '0')}:00Z` }))];
    await back(latest);
    expect(snapshot().connection).toBe('live');
    expect(jobsHeld().filter((id) => !id.startsWith('k')).sort()).toEqual(['j0', 'j2']);
    // fewer than 100: that's all of them
    await back([job({ id: 'j2', created: '2026-10-30T11:00:00Z' })]);
    expect(jobsHeld()).toEqual(['j2']);
  });

  it('back after obid restarted with a lower revision: obid\'s list wins, but a newer answer that came while it was read stays (#1709)', async () => {
    obidHas([job({ status: 'running', step: 'Working', revision: 5 })]);
    await launch();
    // obid restarted before its last save (its revision went back): its list is the job now
    await back([job({ status: 'done', step: 'Done', revision: 2 })]);
    expect(unwired(snapshot().state).jobs.j1.status).toBe('done');

    // a job main hasn't got, which the page loads while obid's list is read: the load's answer is
    // newer than the list's copy
    await back([]);
    let go!: () => void;
    obidHas([job({ id: 'j3', revision: 3 })], { '/v1/jobs?limit=100': { status: 200, body: [job({ id: 'j3', revision: 3 })], hold: new Promise<void>((done) => { go = done; }) } });
    the.answers['/v1/jobs/j3'] = { status: 200, body: job({ id: 'j3', title: 'Vault tests', revision: 4 }) };
    the.stream!.close();
    await settle();
    vi.advanceTimersByTime(1000);
    await settle();
    expect(await load({ job: 'j3' })).toBe(true);
    go();
    await settle();
    expect(snapshot().connection).toBe('live');
    expect(unwired(snapshot().state).jobs.j3.title).toBe('Vault tests');
  });
});
