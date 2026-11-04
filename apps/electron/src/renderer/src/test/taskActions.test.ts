import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TRY_AGAIN, owners } from '../../../shared/words';

// The task's menu wired to obid (docs/contracts/tasks.md, TASKS-1 to TASKS-3, TASKS-10): what the
// app sends obid through main's bridge (window.app.obid), and what the task does with the answer.

// hold: obid answers only once it's settled (an event can come meanwhile)
type Answer = { status: number; body?: unknown; hold?: Promise<unknown> };
const calls: { method: string; path: string; body?: unknown }[] = [];
// obid's event stream, as main passes it on (window.app.obid.follow): fed by the test
let feed: (text: string) => void = () => {};
let answers: Record<string, Answer> = {};
const job = (more: Record<string, unknown> = {}) => ({
  id: 'j1', request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'done', step: 'Done', summary: '', created: '2026-10-30T10:00:00Z', finished: '2026-10-30T10:20:00Z',
  commits: 1, published: null, rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null,
  title: 'Fix it', archived: null, ...more,
});

function memory(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); },
  };
}
const flush = () => new Promise((r) => setTimeout(r, 0));

async function load(withObid: boolean) {
  vi.resetModules();
  calls.length = 0;
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', memory());
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const a = answers[`${method} ${path}`] ?? { status: 404, body: { error: 'Not found.' } };
    await a.hold;
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  // Confirm on a confirm item: main signs it with this Mac's device key (main/device.ts)
  const confirm = async (item: string) => {
    calls.push({ method: 'CONFIRM', path: item });
    await answers[`CONFIRM ${item}`]?.hold;
    return { status: 200, text: JSON.stringify(answers[`CONFIRM ${item}`]?.body ?? {}) };
  };
  const follow = (onChunk: (text: string) => void) => { feed = onChunk; return () => {}; };
  vi.stubGlobal('window', withObid ? { app: { obid: { call, confirm, follow } } } : {});
  const tasks = await import('../tasks');
  const live = await import('../live');
  return { tasks, live };
}
// a job obid has, as the app got it (renaming it to its own title brings it in)
async function seed(t: Awaited<ReturnType<typeof load>>['tasks'], more: Record<string, unknown> = {}) {
  answers['PATCH /v1/jobs/j1'] = { status: 200, body: job(more) };
  t.renameTask('j1', 'Fix it');
  await flush();
  calls.length = 0;
}

// Following obid's events, with obid having `jobs` and nothing else; then an event, as obid sends it
async function following(live: Awaited<ReturnType<typeof load>>['live'], jobs: unknown[]) {
  answers['GET /v1/status'] = { status: 200, body: { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0 } };
  answers['GET /v1/jobs?limit=100'] = { status: 200, body: jobs };
  answers['GET /v1/inbox'] = { status: 200, body: [] };
  answers['GET /v1/conversations?limit=200'] = { status: 200, body: [] };
  live.connect();
  feed('');
  for (let i = 0; i < 5; i++) await flush();
  expect(live.liveState().connection).toBe('live');
  calls.length = 0;
  return (name: string, data: unknown) => feed(`event: ${name}\ndata: ${JSON.stringify(data)}\n\n`);
}
// an answer of obid's held until the test lets it go
function held() {
  let go!: () => void;
  const hold = new Promise<void>((done) => { go = done; });
  return { hold, go };
}

describe('the task menu on obid', () => {
  // job()'s dates are fixed, and a done task older than 3 days leaves Home for Archive by itself:
  // the clock stays just after them, or these tests expire (TASKS-2 did on 2026-11-02, #1839)
  beforeEach(() => { answers = {}; vi.setSystemTime('2026-10-30T11:00:00Z'); });
  afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

  it('TASKS-1: Rename sends PATCH /v1/jobs/{id}, shows at once, and goes back with obid\'s words when refused', async () => {
    const { tasks, live } = await load(true);
    await seed(tasks);
    answers['PATCH /v1/jobs/j1'] = { status: 200, body: job({ title: 'Vault tests' }) };
    tasks.renameTask('j1', '  Vault tests ');
    expect(tasks.dev!.get()[0].title).toBe('Vault tests');
    await flush();
    expect(calls).toEqual([{ method: 'PATCH', path: '/v1/jobs/j1', body: { title: 'Vault tests' } }]);
    expect(tasks.dev!.get()[0].title).toBe('Vault tests');

    // the same name again (a blur with nothing changed): obid isn't told
    calls.length = 0;
    tasks.renameTask('j1', 'Vault tests ');
    await flush();
    expect(calls).toEqual([]);

    answers['PATCH /v1/jobs/j1'] = { status: 422, body: { error: 'A title is one line of 1 to 60 characters.' } };
    tasks.renameTask('j1', 'x'.repeat(61));
    await flush();
    expect(tasks.dev!.get()[0].title).toBe('Vault tests');
    expect(live.liveState().problem).toBe('A title is one line of 1 to 60 characters.');
  });

  it('TASKS-2: Archive sends POST /v1/jobs/{id}/archive and the task moves to Archive; a 409 leaves it, in obid\'s words', async () => {
    const { tasks, live } = await load(true);
    await seed(tasks);
    answers['POST /v1/jobs/j1/archive'] = { status: 409, body: { error: 'Dismiss it first, or publish it.' } };
    tasks.remove('j1');
    await flush();
    expect(calls).toEqual([{ method: 'POST', path: '/v1/jobs/j1/archive', body: { archived: true } }]);
    expect(live.liveState().problem).toBe('Dismiss it first, or publish it.');
    expect(tasks.tasksWith('done').map((t) => t.id)).toEqual(['j1']);

    answers['POST /v1/jobs/j1/archive'] = { status: 200, body: job({ archived: '2026-10-30T11:00:00Z' }) };
    tasks.remove('j1');
    await flush();
    expect(tasks.tasksWith('done')).toEqual([]);
    expect(tasks.archivedTasks().map((a) => a.id)).toEqual(['j1']);
  });

  it('TASKS-3: Delete sends POST /v1/jobs/{id}/delete; a 409 leaves the task, in obid\'s words; once deleted it\'s gone', async () => {
    const { tasks, live } = await load(true);
    await seed(tasks);
    answers['POST /v1/jobs/j1/delete'] = { status: 409, body: { error: 'Its pull request is still open: close or merge it first, or archive the job.' } };
    tasks.deleteTask('j1');
    await flush();
    expect(calls).toEqual([{ method: 'POST', path: '/v1/jobs/j1/delete', body: undefined }]);
    expect(live.liveState().problem).toBe('Its pull request is still open: close or merge it first, or archive the job.');
    expect(tasks.dev!.get().map((t) => t.id)).toEqual(['j1']);

    answers['POST /v1/jobs/j1/delete'] = { status: 200, body: { deleted: true, ask: null } };
    tasks.deleteTask('j1');
    await flush();
    expect(tasks.dev!.get()).toEqual([]);
  });

  it('TASKS-3: a job obid no longer has (404) is gone too', async () => {
    const { tasks } = await load(true);
    await seed(tasks);
    tasks.deleteTask('j1');
    await flush();
    expect(tasks.dev!.get()).toEqual([]);
  });

  it('TASKS-11: with commits that aren\'t published, Delete asks first on the Confirm card, and only its Confirm deletes', async () => {
    const { tasks, live } = await load(true);
    await seed(tasks, { commits: 2 });
    const ask = {
      id: 'confirm-1', kind: 'confirm', job: 'j1', title: 'Delete Fix it (its worktree and local branch go; 2 commits that aren’t published are lost)',
      text: 'Deleting a job can’t be undone: it happens only if you confirm.', options: ['Confirm', 'Cancel'], details: null,
      created: '2026-10-30T11:00:00Z', state: 'open', later: null, headline: null, recommended: null,
      confirm: { action: 'delete_job', target: 'j1', payload: { snapshot: {} } },
    };
    answers['POST /v1/jobs/j1/delete'] = { status: 200, body: { deleted: false, ask } };
    tasks.deleteTask('j1');
    await flush();
    // still there, waiting on the owner: obid's Confirm on its card, naming the commits lost
    const [task] = tasks.dev!.get();
    expect(task.id).toBe('j1');
    expect(task.status).toBe('needs');
    expect(task.job.ask).toMatchObject({ kind: 'confirm', headline: ask.title, options: ['Confirm', 'Cancel'] });

    // the owner's click: the signed Confirm of that item, nothing else (obid deletes it, and its
    // event takes the task away)
    calls.length = 0;
    answers['CONFIRM confirm-1'] = { status: 200, body: { ...ask, state: 'done' } };
    tasks.answer('j1', 'Confirm');
    await flush();
    expect(calls).toEqual([{ method: 'CONFIRM', path: 'confirm-1' }]);
    expect(live.liveState().inbox).toEqual([]);
  });

  it('TASKS-9: the session loads page after page, then only what came since, by n, one fetch at a time', async () => {
    const { live } = await load(true);
    const tool = (n: number, state = 'done') => ({ n, at: '2026-10-30T10:00:00Z', kind: 'tool', tool: 'Read', summary: `Reading f${n}.rs`, state });
    // obid's first page stops at entry 2; next says where the rest starts
    answers['GET /v1/jobs/j1/transcript'] = { status: 200, body: { next: 2, entries: [tool(1), tool(2)] } };
    answers['GET /v1/jobs/j1/transcript?after=2'] = { status: 200, body: { next: null, entries: [tool(3, 'running')] } };
    await live.loadSession('j1');
    expect(calls.map((c) => c.path)).toEqual(['/v1/jobs/j1/transcript', '/v1/jobs/j1/transcript?after=2']);
    expect(live.sessionNow('j1')!.map((e) => e.n)).toEqual([1, 2, 3]);

    // the next event: after the highest n; entry 3 ended, so it comes again, in place
    calls.length = 0;
    answers['GET /v1/jobs/j1/transcript?after=3'] = { status: 200, body: { next: null, entries: [tool(3), tool(4)] } };
    const first = live.loadSession('j1');
    // two more events while that fetch is out: one more fetch, not two
    void live.loadSession('j1');
    void live.loadSession('j1');
    answers['GET /v1/jobs/j1/transcript?after=4'] = { status: 200, body: { next: null, entries: [] } };
    await first;
    await flush();
    expect(calls.map((c) => c.path)).toEqual(['/v1/jobs/j1/transcript?after=3', '/v1/jobs/j1/transcript?after=4']);
    const session = live.sessionNow('j1')!;
    expect(session.map((e) => e.n).sort()).toEqual([1, 2, 3, 4]);
    expect(session.find((e) => e.n === 3)!.state).toBe('done');
  });

  it('LIVETASK-3: a followed session takes obid\'s events as they come, and fetches what it missed', async () => {
    const { live } = await load(true);
    const send = await following(live, [job({ status: 'running' })]);
    const entry = (n: number, more = {}) => ({ n, at: '2026-10-30T10:00:00Z', kind: 'tool', tool: 'Read', summary: `Reading f${n}.rs`, state: 'running', ...more });
    // nobody's looking: nothing kept
    send('transcript', { job: 'j1', entry: entry(1) });
    expect(live.sessionNow('j1')).toBeUndefined();
    answers['GET /v1/jobs/j1/transcript'] = { status: 200, body: { next: null, entries: [entry(1), { n: 2, at: '2026-10-30T10:00:00Z', kind: 'said', summary: 'One', text: 'One\n' }] } };
    await live.loadSession('j1');
    calls.length = 0;
    // an entry that came, one that ended, and a line the agent wrote: no fetch for any of them
    send('transcript', { job: 'j1', entry: entry(3) });
    send('transcript', { job: 'j1', entry: entry(1, { state: 'done', seconds: 1 }) });
    send('transcript', { job: 'j1', n: 2, append: 'Two\n' });
    expect(calls).toEqual([]);
    expect(live.sessionNow('j1')!.find((e) => e.n === 1)!.state).toBe('done');
    expect(live.sessionNow('j1')!.find((e) => e.n === 2)!.text).toBe('One\nTwo\n');
    // 4 and 5 never reached here; 6 did. The next fetch starts before the hole, so they come
    send('transcript', { job: 'j1', entry: entry(6) });
    answers['GET /v1/jobs/j1/transcript?after=3'] = { status: 200, body: { next: null, entries: [entry(4), entry(5), entry(6, { state: 'done' })] } };
    await live.loadSession('j1');
    expect(calls.map((c) => c.path)).toEqual(['/v1/jobs/j1/transcript?after=3']);
    expect(live.sessionNow('j1')!.map((e) => e.n).sort()).toEqual([1, 2, 3, 4, 5, 6]);
    // deleted: its session goes too
    send('job', { id: 'j1', deleted: true });
    expect(live.sessionNow('j1')).toBeUndefined();
  });

  it('LIVETASK-3: an event that comes while the first read is on its way is kept', async () => {
    const { live } = await load(true);
    const entry = (n: number) => ({ n, at: '2026-10-30T10:00:00Z', kind: 'tool', tool: 'Read', summary: `Reading f${n}.rs`, state: 'done' });
    answers['GET /v1/jobs/j1/transcript'] = { status: 200, body: { next: null, entries: [entry(1)] } };
    await live.loadSession('j1');
    answers['GET /v1/status'] = { status: 200, body: { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0 } };
    answers['GET /v1/jobs?limit=100'] = { status: 200, body: [job({ status: 'running' })] };
    answers['GET /v1/inbox'] = { status: 200, body: [] };
    answers['GET /v1/conversations?limit=200'] = { status: 200, body: [] };
    live.connect();
    // obid's first text holds it: the read of everything else starts only then
    feed(`event: transcript\ndata: ${JSON.stringify({ job: 'j1', entry: entry(2) })}\n\n`);
    for (let i = 0; i < 5; i++) await flush();
    expect(live.liveState().connection).toBe('live');
    expect(live.sessionNow('j1')!.map((e) => e.n).sort()).toEqual([1, 2]);
  });

  it('LIVETASK-11: a line opens from obid\'s entry, its output, diff and all', async () => {
    const { live } = await load(true);
    answers['GET /v1/jobs/j1/transcript/7'] = { status: 200, body: { n: 7, kind: 'tool', detail: { input: { command: 'cargo test' }, output: { text: 'FAILED', lines: 9, exit: 101 } } } };
    expect(await live.entryDetail('j1', 7)).toMatchObject({ input: 'cargo test', output: 'FAILED', lines: 9, exit: 101 });
    expect(calls.map((c) => [c.method, c.path])).toEqual([['GET', '/v1/jobs/j1/transcript/7']]);
    // obid hasn't got it: says why, opens nothing
    expect(await live.entryDetail('j1', 8)).toBeUndefined();
    expect(live.liveState().problem).toBe('Not found.');
  });

  it('LIVETASK-12: why it stopped is obid\'s reason, on its task and on the card that asks, before the agent\'s last words', async () => {
    const { live, tasks } = await load(true);
    await following(live, [job({ id: 'j1', status: 'stopped', reason: 'Stopped by you.', summary: 'I was reading.' }), job({ id: 'j2', status: 'failed', reason: 'The login failed.\nThe log', summary: 'I was reading.', dismissed: null })]);
    const all = tasks.tasksOf('tasks');
    expect(all.find((t) => t.id === 'j1')!.job.reason).toBe('Stopped by you.');
    const failed = all.find((t) => t.id === 'j2')!;
    expect(failed.job.reason).toBe('The login failed.\nThe log');
    expect(failed.job.ask?.text).toBe('The login failed.');
  });

  it('LIVETASK-15: a deleted job fetches nothing more', async () => {
    const { live } = await load(true);
    const send = await following(live, [job({ status: 'running' })]);
    answers['GET /v1/jobs/j1/transcript'] = { status: 200, body: { next: null, entries: [] } };
    await live.loadSession('j1');
    send('job', { id: 'j1', deleted: true });
    calls.length = 0;
    await live.loadSession('j1');
    expect(calls).toEqual([]);
    expect(live.liveState().problem).toBeNull();
  });

  it('LIVETASK-7: telling the agent is obid\'s own message; a finished one\'s follow-up is what the pane then follows', async () => {
    const { live } = await load(true);
    answers['POST /v1/jobs/j1/messages'] = { status: 200, body: job({ status: 'running' }) };
    expect(await live.tellAgent('j1', 'Use the other file.')).toBeUndefined();
    expect(calls.map((c) => [c.method, c.path])).toEqual([['POST', '/v1/jobs/j1/messages']]);
    expect(calls[0].body).toMatchObject({ text: 'Use the other file.' });
    answers['POST /v1/jobs/j1/messages'] = { status: 200, body: job({ id: 'j2', followUpOf: 'j1', status: 'queued' }) };
    answers['GET /v1/jobs/j1'] = { status: 200, body: job() };
    expect(await live.tellAgent('j1', 'Now the docs.')).toBe('j2');
  });

  it('TASKS-10: on the mock (npm run web), the menu works in place and calls nothing', async () => {
    const { tasks } = await load(false);
    const id = tasks.addTask('Fix the login bug')!;
    tasks.renameTask(id, 'Login bug');
    expect(tasks.dev!.get()[0].title).toBe('Login bug');
    tasks.deleteTask(id);
    expect(tasks.dev!.get()).toEqual([]);
    expect(calls).toEqual([]);
  });

  it('TASKCONV-4: words on a task\'s page go to obid with conversation: true, and the task comes back in the conversation obid made', async () => {
    const { tasks } = await load(true);
    await seed(tasks);
    answers['POST /v1/jobs/j1/messages'] = { status: 201, body: job({ id: 'j2', status: 'queued', followUpOf: 'j1', conversation: 'c1' }) };
    answers['GET /v1/jobs/j1'] = { status: 200, body: job({ conversation: 'c1' }) };
    await tasks.tell('j1', 'why wasn\'t this merged?');
    expect(calls[0]).toEqual({ method: 'POST', path: '/v1/jobs/j1/messages', body: { text: 'why wasn\'t this merged?', conversation: true } });
    expect(tasks.conversationOfTask('j1')).toBe('c1');
    expect(tasks.conversationOfTask('j2')).toBe('c1');
  });
});

// What the owner does takes obid's answer over the state as it is when the answer comes, so an
// event obid sent while the request was out isn't undone until a reload (the side panel's deleted
// task that stayed, #1707)
describe('a turn\'s answer, as the plan\'s answer and its events land (#1788)', () => {
  beforeEach(() => { answers = {}; });
  afterEach(() => { vi.unstubAllGlobals(); });

  const convo = { id: 'c1', title: 'Fixes', pinned: false, created: '2026-11-01T12:00:00Z', lastActivity: '2026-11-01T12:00:00Z', cursor: '' };
  const turn = (id: string, who: string, more: Record<string, unknown> = {}) => ({ id, who, text: id, at: `2026-11-01T12:0${id.at(-1)}:00Z`, jobs: [], conversation: 'c1', ...more });
  // the id of words the app sent, once it has, after `before` such calls (it sends them in its own time)
  const wordsSent = (before: number) => vi.waitFor(() => {
    const sent = calls.filter((c) => c.path === '/v1/plans');
    expect(sent.length).toBeGreaterThan(before);
    return (sent.at(-1)!.body as { id: string }).id;
  }, { timeout: 5000 });
  // words sent in a conversation, their plan's answer held; a job they started, and what obid has so far
  async function sentWords(said = 'Fix it') {
    const { live } = await load(true);
    vi.stubGlobal('matchMedia', () => ({ matches: true }));
    const event = await following(live, []);
    event('conversation', convo);
    const plan = held();
    answers['POST /v1/plans'] = { status: 200, body: { kind: 'none', jobs: [], conversation: 'c1' }, hold: plan.hold };
    live.ask(said, 'c1');
    // (the words' turn has the id the app sent them with)
    const sentAs = await wordsSent(0);
    event('turn', turn('u1', 'user', { id: sentAs }));
    event('job', job({ conversation: 'c1', turn: sentAs, status: 'running', finished: null }));
    const shown = () => live.chatOf(live.liveState(), convo);
    const under = (id: string) => shown().said.find((m) => m.text === id)?.jobs ?? [];
    return { live, event, plan, shown, under, sentAs };
  }
  const landed = async (go: () => void) => { go(); for (let i = 0; i < 3; i++) await flush(); };

  it('Stop: the plan answers none, so no answer is coming and the card shows under the words', async () => {
    const { plan, shown, under } = await sentWords();
    expect([shown().typing, under('u1')]).toEqual([true, []]);
    await landed(plan.go);
    expect([shown().typing, under('u1')]).toEqual([false, ['j1']]);
  });

  it('a reply whose turn lands after the plan\'s answer: Oby is still answering and the card never shows under the words', async () => {
    const { event, plan, shown, under } = await sentWords();
    answers['POST /v1/plans'].body = { kind: 'reply', text: 'On it', jobs: [], conversation: 'c1' };
    await landed(plan.go);
    expect([shown().typing, under('u1')]).toEqual([true, []]);
    event('turn', turn('o2', 'oby'));
    expect([shown().typing, under('u1'), under('o2')]).toEqual([false, [], ['j1']]);
  });

  it('a question too has its turn; the same words again answered none are answered in their turn', async () => {
    for (const body of [{ kind: 'question', said: 'Fix it', question: { text: 'Which?', options: [], recommended: null }, jobs: [], conversation: 'c1' }, { kind: 'none', repeated: true, jobs: [], conversation: 'c1' }]) {
      const { plan, shown, under } = await sentWords();
      answers['POST /v1/plans'].body = body;
      await landed(plan.go);
      expect([shown().typing, under('u1')]).toEqual([true, []]);
    }
  });

  it('a job\'s report during the turn neither ends it nor counts as the answer', async () => {
    const { live, event, shown, under, sentAs } = await sentWords();
    event('delta', { turn: sentAs, conversation: 'c1', text: 'On it' });
    event('turn', turn('r2', 'oby', { ended: true, jobs: ['j0'] }));
    expect([shown().typing, under('u1'), live.liveState().words.c1]).toEqual([true, [], 'On it']);
  });

  it('words stopped on another device: the job\'s report ends what only events said was under way', async () => {
    const { live } = await load(true);
    vi.stubGlobal('matchMedia', () => ({ matches: true }));
    const event = await following(live, []);
    event('conversation', convo);
    event('turn', turn('u1', 'user'));
    event('job', job({ conversation: 'c1', turn: 'u1', status: 'running', finished: null }));
    event('step', { turn: 'u1', conversation: 'c1', index: 0, text: 'Reading', state: 'running' });
    const shown = () => live.chatOf(live.liveState(), convo);
    expect([shown().typing, shown().step, shown().said[0].jobs]).toEqual([true, 'Reading', undefined]);
    event('turn', turn('r2', 'oby', { ended: true, jobs: ['j1'] }));
    expect([shown().typing, shown().said[0].jobs]).toEqual([false, ['j1']]);
  });

  it('the next words after a Stop never hide the card, before or after their turn lands', async () => {
    const { live, event, plan, under, sentAs } = await sentWords();
    event('step', { turn: sentAs, conversation: 'c1', index: 0, text: 'Reading', state: 'running' });
    await landed(plan.go);
    expect(under('u1')).toEqual(['j1']);
    const next = held();
    answers['POST /v1/plans'] = { status: 200, body: { kind: 'reply', text: 'Ok', jobs: [], conversation: 'c1' }, hold: next.hold };
    const before = calls.filter((c) => c.path === '/v1/plans').length;
    live.ask('And this', 'c1');
    const sentAs2 = await wordsSent(before);
    expect(under('u1')).toEqual(['j1']);
    event('turn', turn('u2', 'user', { id: sentAs2 }));
    expect(under('u1')).toEqual(['j1']);
  });

  it('a supervisor\'s turn after a Stop: the card stays under the words throughout, and after its answer', async () => {
    const { event, plan, shown, under } = await sentWords();
    await landed(plan.go);
    event('step', { turn: 'supervisor-turn', conversation: 'c1', index: 0, text: 'Reading', state: 'running' });
    expect([shown().typing, under('u1')]).toEqual([true, ['j1']]);
    event('turn', turn('o3', 'oby'));
    expect([shown().typing, under('u1'), under('o3')]).toEqual([false, ['j1'], []]);
  });

  it('a bare stop answers none while the words it stops are still on their way: their turn is still answering', async () => {
    const { plan, shown, under, live } = await sentWords();
    const stop = held();
    answers['POST /v1/plans'] = { status: 200, body: { kind: 'none', jobs: [], conversation: 'c1' }, hold: stop.hold };
    live.ask('stop', 'c1');
    await flush();
    await landed(stop.go);
    expect([shown().typing, under('u1')]).toEqual([true, []]);
    await landed(plan.go);
    expect([shown().typing, under('u1')]).toEqual([false, ['j1']]);
  });

  it('words already showing stop with a none, and no streaming line stays', async () => {
    const { event, plan, shown, under, sentAs } = await sentWords();
    vi.stubGlobal('matchMedia', () => ({ matches: false }));
    event('delta', { turn: sentAs, conversation: 'c1', text: 'On it ' });
    await landed(plan.go);
    expect([shown().said.some((m) => m.streaming), under('u1')]).toEqual([false, ['j1']]);
  });

  it('an answer a fetch brought before its event: its event still ends the turn', async () => {
    const { live, event, plan, shown, under, sentAs } = await sentWords();
    answers['POST /v1/plans'].body = { kind: 'reply', text: 'On it', jobs: [], conversation: 'c1' };
    answers['GET /v1/conversation?id=c1&limit=100'] = { status: 200, body: [turn('u1', 'user', { id: sentAs }), turn('o2', 'oby')] };
    await landed(plan.go);
    await live.openConversation('c1');
    await flush();
    expect(shown().typing).toBe(true);
    event('turn', turn('o2', 'oby'));
    expect([shown().typing, under('o2')]).toEqual([false, ['j1']]);
  });

  it('a send that fails: no answer is coming, and the card shows', async () => {
    const { plan, shown, under } = await sentWords();
    Object.assign(answers['POST /v1/plans'], { status: 422, body: { error: 'No.' } });
    await landed(plan.go);
    expect([shown().typing, under('u1')]).toEqual([false, ['j1']]);
  });
});

describe('obid\'s answers over the events that came meanwhile', () => {
  beforeEach(() => { answers = {}; });
  afterEach(() => { vi.unstubAllGlobals(); });

  const confirmItem = {
    id: 'confirm-1', kind: 'confirm', job: 'j1', title: 'Delete Fix it (its worktree and local branch go; work that isn’t committed is lost)',
    text: 'Deleting a job can’t be undone: it happens only if you confirm.', options: ['Confirm', 'Cancel'], details: null,
    created: '2026-10-30T11:00:00Z', state: 'open', later: null, headline: null, recommended: null,
    confirm: { action: 'delete_job', target: 'j1', payload: { snapshot: {} } },
  };

  it('TASKS-11: a task deleted on its Confirm leaves the side panel when its event comes, before the Confirm\'s answer', async () => {
    const { tasks, live } = await load(true);
    const event = await following(live, [job()]);
    answers['POST /v1/jobs/j1/delete'] = { status: 200, body: { deleted: false, ask: confirmItem } };
    tasks.deleteTask('j1');
    await flush();
    expect(tasks.tasksWith('needs').map((t) => t.id)).toEqual(['j1']);

    // obid deletes it on the owner's Confirm: its `deleted` event, then the answer
    const { hold, go } = held();
    answers['CONFIRM confirm-1'] = { status: 200, body: { ...confirmItem, state: 'done', answer: 'Confirm' }, hold };
    tasks.answer('j1', 'Confirm');
    await flush();
    event('job', { id: 'j1', deleted: true });
    go();
    for (let i = 0; i < 3; i++) await flush();
    expect(tasks.dev!.get()).toEqual([]);
  });

  it('an agent\'s question answered: the task shows what its job did meanwhile, not the job as it was asked', async () => {
    const { tasks, live } = await load(true);
    const question = { ...confirmItem, id: 'q-1', kind: 'agentQuestion', title: 'Which branch?', text: 'Which branch?', options: ['dev', 'main'], confirm: null };
    const event = await following(live, [job({ status: 'running', step: 'Waiting for your answer' })]);
    event('inbox', question);
    expect(tasks.dev!.get()[0].status).toBe('needs');

    const { hold, go } = held();
    answers['POST /v1/inbox/q-1/answer'] = { status: 200, body: { ...question, state: 'done', answer: 'dev' }, hold };
    tasks.answer('j1', 'dev');
    await flush();
    event('job', job({ status: 'running', step: 'Working on dev' }));
    go();
    for (let i = 0; i < 3; i++) await flush();
    expect(tasks.dev!.get()[0].job.step).toBe('Working on dev');
  });

  it('an offer dispatched: the task it starts stays, though its event came before the answer', async () => {
    const { tasks, live } = await load(true);
    const offer = { ...confirmItem, id: 's-1', kind: 'start', job: '', title: 'Fix it', confirm: null, conversation: 'c1', offer: { id: 'o1', what: 'Fix it', task: 'Fix it', untrusted: false } };
    const event = await following(live, []);
    event('inbox', offer);

    const { hold, go } = held();
    answers['POST /v1/inbox/s-1/answer'] = { status: 200, body: { ...offer, state: 'done', answer: 'Start' }, hold };
    const answered = live.answerOffer('o1', 'Dispatch');
    await flush();
    event('job', job({ status: 'queued', step: 'Getting ready' }));
    go();
    await answered;
    expect(tasks.dev!.get().map((t) => t.id)).toEqual(['j1']);
  });

  it('back after the stream was away: a task deleted meanwhile (on the iPhone) leaves, the older ones a conversation fetched stay', async () => {
    const { tasks, live } = await load(true);
    await following(live, [job({ id: 'j2', created: '2026-10-30T11:00:00Z' }), job()]);
    // one older than obid's latest 100, which an opened conversation fetched
    answers['GET /v1/conversation?id=c1&limit=100'] = { status: 200, body: [{ id: 't1', who: 'oby', text: 'Done.', at: '2026-10-01T10:00:00Z', jobs: ['j0'], conversation: 'c1' }] };
    answers['GET /v1/jobs/j0'] = { status: 200, body: job({ id: 'j0', created: '2026-10-01T10:00:00Z' }) };
    await live.openConversation('c1');
    for (let i = 0; i < 3; i++) await flush();
    expect(tasks.dev!.get().map((t) => t.id)).toEqual(['j2', 'j1', 'j0']);

    // j1 deleted while away; obid's 100 newest are j2 and 99 after it
    const latest = [job({ id: 'j2', created: '2026-10-30T11:00:00Z' }), ...Array.from({ length: 99 }, (_, i) => job({ id: `k${i}`, created: `2026-10-29T10:${String(i % 60).padStart(2, '0')}:00Z` }))];
    await following(live, latest);
    expect(tasks.dev!.get().map((t) => t.id).filter((id) => !id.startsWith('k'))).toEqual(['j2', 'j0']);
    // fewer than 100: that's all of them
    await following(live, [job({ id: 'j2', created: '2026-10-30T11:00:00Z' })]);
    expect(tasks.dev!.get().map((t) => t.id)).toEqual(['j2']);
  });

  // each answer obid gives as the whole job, crossed by the job's newer event (its revision on, #1709)
  const asJobs: [string, (live: Awaited<ReturnType<typeof load>>['live']) => Promise<unknown>][] = [
    ['POST /v1/jobs/j1/stop', (l) => l.stopJob('j1')],
    ['POST /v1/jobs/j1/mode', (l) => l.changeMode('j1', 'plan')],
    ['POST /v1/jobs/j1/archive', (l) => l.archiveJob('j1')],
    ['PATCH /v1/jobs/j1', (l) => l.renameJob('j1', 'Vault tests')],
    ['POST /v1/github/work/w1/start', (l) => l.startGitHubWork('w1')],
    ['POST /v1/jobs/j1/messages', (l) => l.tellJob('j1', 'And the docs')],
    ['POST /v1/jobs/j1/dismiss', (l) => l.answerJob('j1', 'Stop here')],
  ];

  // #1836: Sangam's failed card, on a job with no inbox item
  describe('a failed task\'s card', () => {
    const failed = async () => {
      const { tasks, live } = await load(true);
      const event = await following(live, [job({ status: 'failed', step: 'Failed', summary: 'It broke', session: 's1', revision: 1 })]);
      calls.length = 0;
      return { tasks, live, event };
    };
    const ask = (t: Awaited<ReturnType<typeof load>>['tasks']) => t.dev!.get().find((x) => x.id === 'j1')!.job.ask;

    describe('TASKS-14, #1795: Oby\'s own question about it, whose lone Done only settles it', () => {
      const question = {
        id: 'orchestrator-1', kind: 'question', job: 'j1', title: 'Fix it needs your decision', text: 'Oby looked at it. Oby recommends: Rebase it.', options: ['Done'], details: null,
        created: '2026-10-30T11:00:00Z', state: 'open', later: null, headline: null, recommended: null,
      };
      const stalled = { ...question, id: 'supervisor-1', title: 'Fix it stalled' };
      const asked = async (item: Record<string, unknown> = question) => {
        const it = await failed();
        it.event('job', job({ status: 'interrupted', step: 'Stopped', session: 's1', reason: 'Stopped at a limit', revision: 2 }));
        it.event('inbox', item);
        answers['POST /v1/jobs/j1/messages'] = { status: 200, body: job({ status: 'running', step: 'Working', session: 's1', revision: 3 }) };
        answers['POST /v1/inbox/orchestrator-1/answer'] = { status: 200, body: { ...question, state: 'done', answer: 'Done' } };
        answers['POST /v1/jobs/j1/dismiss'] = { status: 200, body: job({ status: 'interrupted', step: 'Stopped', session: 's1', dismissed: '2026-10-30T12:00:00Z', revision: 3 }) };
        calls.length = 0;
        return it;
      };
      const sent = () => calls.map((c) => [c.path, c.body]);
      const WAYS = ['Give it another round', 'Tell it something', 'Stop here'];

      it('offers the way on, with no button looking recommended, and no Done that only settles it', async () => {
        const { tasks } = await asked();
        expect(ask(tasks)).toMatchObject({ kind: 'question', options: WAYS, recommended: -1 });
      });
      it('stays as obid asked on a task that didn\'t stop, or one its follow-up carries on', async () => {
        const { tasks, event } = await asked();
        event('job', job({ status: 'running', step: 'Working', session: 's1', revision: 3 }));
        expect(ask(tasks)!.options).toEqual(['Done']);
        event('job', job({ status: 'interrupted', step: 'Stopped', session: 's1', revision: 4 }));
        event('job', job({ id: 'j2', followUpOf: 'j1', status: 'running', revision: 1 }));
        expect(ask(tasks)!.options).toEqual(['Done']);
      });
      it('Give it another round tells its agent, in words a click on it signs, then settles the question', async () => {
        const { tasks, live } = await asked();
        await live.answerJob('j1', 'Give it another round');
        expect(sent()).toEqual([
          ['/v1/jobs/j1/messages', { text: TRY_AGAIN }],
          ['/v1/inbox/orchestrator-1/answer', { answer: 'Done' }],
        ]);
        expect(owners('POST', calls[0].path, calls[0].body, ['Give it another round'], [])).toEqual([0]);
        expect(ask(tasks)).toBeUndefined();
      });
      it('Tell it something sends the owner\'s words to the task, never as the question\'s answer', async () => {
        const { live } = await asked();
        await live.answerJob('j1', 'Tell it something', 'Rebase it first');
        expect(sent()).toEqual([
          ['/v1/jobs/j1/messages', { text: 'Rebase it first' }],
          ['/v1/inbox/orchestrator-1/answer', { answer: 'Done' }],
        ]);
        expect(owners('POST', calls[0].path, calls[0].body, ['Rebase it first'], [])).toEqual([0]);
      });
      it('tasks_17_tell_it_what_to_do_messages_the_job_then_settles_done', async () => {
        // a task still running: Done alone only settles it; the box steers as the task page's box
        const { tasks, live, event } = await asked();
        event('job', job({ status: 'running', step: 'Working', session: 's1', revision: 3 }));
        expect(ask(tasks)).toMatchObject({ options: ['Done'], steer: true, fullText: true });
        await live.answerJob('j1', 'Tell it what to do', 'Rebase onto main, then merge');
        expect(sent()).toEqual([
          ['/v1/jobs/j1/messages', { text: 'Rebase onto main, then merge', conversation: true }],
          ['/v1/inbox/orchestrator-1/answer', { answer: 'Done' }],
        ]);
        expect(owners('POST', calls[0].path, calls[0].body, ['Rebase onto main, then merge'], [])).toEqual([0]);
        expect(ask(tasks)).toBeUndefined();
      });
      it('Stop here lets the task go and sends no Done, which obid would refuse: it settled the question itself', async () => {
        const { tasks, live } = await asked();
        answers['POST /v1/inbox/orchestrator-1/answer'] = { status: 409, body: { error: 'That item is done already.' } };
        await live.answerJob('j1', 'Stop here');
        expect(sent().map(([path]) => path)).toEqual(['/v1/jobs/j1/dismiss']);
        expect(live.liveState().problem).toBeNull();
        expect(ask(tasks)).toBeUndefined();
      });
      it('a task its limit stopped: its limit card decides, so no round is offered, nor sent by a card drawn before it (#1563)', async () => {
        const { tasks, live, event } = await asked();
        expect(ask(tasks)!.options).toEqual(WAYS);
        event('job', job({ status: 'interrupted', step: 'Stopped', session: 's1', accountLimit: { decision: 'd1', state: 'pending', resetAt: null }, revision: 3 }));
        expect(ask(tasks)!.options).toEqual(['Done']);
        await live.answerJob('j1', 'Give it another round');
        expect(sent()).toEqual([]);
      });
      it('refused, it says why and is still asked', async () => {
        const { tasks, live } = await asked();
        answers['POST /v1/jobs/j1/messages'] = { status: 409, body: { error: 'It can\'t go on from here.' } };
        await live.answerJob('j1', 'Give it another round');
        expect(sent().map(([path]) => path)).toEqual(['/v1/jobs/j1/messages']);
        expect(live.liveState().problem).toBe('It can\'t go on from here.');
        expect(ask(tasks)!.options).toEqual(WAYS);
      });
      it('a stalled task\'s question refuses an answer: none is sent, and it means nothing once the task goes on', async () => {
        const { tasks, live, event } = await asked(stalled);
        expect(ask(tasks)!.options).toEqual(WAYS);
        await live.answerJob('j1', 'Give it another round');
        expect(sent().map(([path]) => path)).toEqual(['/v1/jobs/j1/messages']);
        expect(live.liveState().problem).toBeNull();
        expect(ask(tasks)).toBeUndefined();
        event('job', job({ status: 'done', step: 'Done', session: 's1', revision: 4 }));
        expect(ask(tasks)).toBeUndefined();
      });
      it('past a safety limit: the Confirm obid makes shows over the question, and going on settles it', async () => {
        const { tasks, live, event } = await asked();
        answers['POST /v1/jobs/j1/messages'] = { status: 422, body: { error: 'It ran past its safety limit: Confirm to go on.' } };
        const confirm = { ...question, id: 'confirm-1', kind: 'confirm', title: 'Carry on Fix it', text: 'Past its limit', options: ['Confirm', 'Cancel'], confirm: { action: 'retry_job', target: 'j1', payload: null } };
        await live.answerJob('j1', 'Give it another round');
        event('inbox', confirm);
        expect(ask(tasks)!.options).toEqual(['Confirm', 'Cancel']);
        expect(sent().map(([path]) => path)).toEqual(['/v1/jobs/j1/messages']);

        calls.length = 0;
        answers['CONFIRM confirm-1'] = { status: 200, body: { ...confirm, state: 'done', answer: 'Confirm' } };
        await live.answerJob('j1', 'Confirm');
        expect(sent().map(([path]) => path)).toEqual(['confirm-1', '/v1/inbox/orchestrator-1/answer']);
      });
    });

    it('offers another round and stopping, with another round first', async () => {
      const { tasks } = await failed();
      expect(ask(tasks)).toMatchObject({ kind: 'failed', options: ['Give it another round', 'Stop here'], recommended: 0 });
    });
    it('Give it another round tells its agent obid\'s Try-again words', async () => {
      const { live } = await failed();
      answers['POST /v1/jobs/j1/messages'] = { status: 200, body: job({ status: 'running', step: 'Working', revision: 2 }) };
      await live.answerJob('j1', 'Give it another round');
      expect(calls.at(-1)).toMatchObject({ method: 'POST', path: '/v1/jobs/j1/messages' });
      expect(JSON.stringify(calls.at(-1)!.body)).toContain('Try again, picking up where you left off.');
      // a click on it signs them: the job stays the owner's own, not "words no app of the owner's signed"
      expect(owners('POST', calls.at(-1)!.path, calls.at(-1)!.body, ['Give it another round'], [])).toEqual([0]);
      expect(live.liveState().jobs.j1.status).toBe('running');
    });
    it('a second failure asks again', async () => {
      const { tasks, live, event } = await failed();
      answers['POST /v1/jobs/j1/messages'] = { status: 200, body: job({ status: 'running', step: 'Working', session: 's1', revision: 2 }) };
      await live.answerJob('j1', 'Give it another round');
      expect(ask(tasks)).toBeUndefined();
      event('job', job({ status: 'failed', step: 'Failed', summary: 'It broke again', session: 's1', revision: 3 }));
      expect(ask(tasks)).toMatchObject({ options: ['Give it another round', 'Stop here'] });
      expect(tasks.dev!.get().find((x) => x.id === 'j1')!.status).toBe('needs');
    });
    it('one that never started asks again what it was asked: it has no session to pick up', async () => {
      const { live, event } = await failed();
      event('job', job({ status: 'failed', step: 'Failed', session: null, moves: [], title: 'Rebase 391', request: 'yes, do that', revision: 2 }));
      answers['POST /v1/jobs/j1/messages'] = { status: 200, body: job({ id: 'j2', followUpOf: 'j1', status: 'running', revision: 1 }) };
      answers['GET /v1/jobs/j1'] = { status: 200, body: job({ status: 'failed', session: null, revision: 2 }) };
      await live.answerJob('j1', 'Give it another round');
      const told = calls.find((c) => c.path === '/v1/jobs/j1/messages')!;
      expect(JSON.stringify(told.body)).toContain('Rebase 391');
      expect(JSON.stringify(told.body)).toContain('yes, do that');
      expect(JSON.stringify(told.body)).not.toContain('Try again');
    });
    it('one moved to another account picks up with the Try-again words', async () => {
      const { live, event } = await failed();
      event('job', job({ status: 'failed', step: 'Failed', session: null, moves: [{ from: 'a' }], revision: 2 }));
      answers['POST /v1/jobs/j1/messages'] = { status: 200, body: job({ status: 'running', revision: 3 }) };
      await live.answerJob('j1', 'Give it another round');
      expect(JSON.stringify(calls.find((c) => c.path === '/v1/jobs/j1/messages')!.body)).toContain('Try again');
    });
    it('one its follow-up carries on asks nothing', async () => {
      const { tasks, event } = await failed();
      event('job', job({ id: 'j2', followUpOf: 'j1', status: 'running', revision: 1 }));
      expect(ask(tasks)).toBeUndefined();
    });
    it('a refused round says why, and leaves the choice', async () => {
      const { tasks, live } = await failed();
      answers['POST /v1/jobs/j1/messages'] = { status: 409, body: { error: 'It can\'t go on from here.' } };
      await live.answerJob('j1', 'Give it another round');
      expect(live.liveState().problem).toBe('It can\'t go on from here.');
      expect(live.liveState().answered.j1).toBeUndefined();
      expect(ask(tasks)!.options).toContain('Stop here');
    });
  });
  it.each(asJobs)('%s: an answer older than the job\'s event keeps the event\'s job, and never brings back a deleted one', async (route, act) => {
    const { tasks, live } = await load(true);
    const event = await following(live, [job({ status: 'running', step: 'Working', revision: 1 })]);
    let { hold, go } = held();
    answers[route] = { status: 200, body: job({ status: 'running', step: 'Working', revision: 2 }), hold };
    let acted = act(live);
    await flush();
    event('job', job({ status: 'done', step: 'Done', revision: 3 }));
    go();
    await acted;
    expect(live.liveState().jobs.j1.status).toBe('done');

    // deleted (on the iPhone) while the answer was out
    ({ hold, go } = held());
    answers[route] = { status: 200, body: job({ revision: 4 }), hold };
    acted = act(live);
    await flush();
    event('job', { id: 'j1', deleted: true });
    go();
    await acted;
    expect(tasks.dev!.get()).toEqual([]);
  });

  it('a job with no revision (obid downgraded) is taken as it comes, not held off by one from before (#1709)', async () => {
    const { live } = await load(true);
    const event = await following(live, [job({ status: 'running', step: 'Working', revision: 5 })]);
    event('job', job({ status: 'done', step: 'Done' }));
    expect(live.liveState().jobs.j1.status).toBe('done');
  });

  it('back after obid restarted with a lower revision: obid\'s list wins, but a newer answer that came while it was read stays (#1709)', async () => {
    const { live } = await load(true);
    await following(live, [job({ status: 'running', step: 'Working', revision: 5 })]);
    // obid restarted before its last save (its revision went back): its list is the job now
    await following(live, [job({ status: 'done', step: 'Done', revision: 2 })]);
    expect(live.liveState().jobs.j1.status).toBe('done');

    // a rename's answer, newer than the list read meanwhile
    const { hold, go } = held();
    answers['GET /v1/jobs?limit=100'] = { status: 200, body: [job({ revision: 3 })], hold };
    answers['PATCH /v1/jobs/j1'] = { status: 200, body: job({ title: 'Vault tests', revision: 4 }) };
    live.connect();
    feed('');
    await flush();
    await live.renameJob('j1', 'Vault tests');
    go();
    for (let i = 0; i < 5; i++) await flush();
    expect(live.liveState().connection).toBe('live');
    expect(live.liveState().jobs.j1.title).toBe('Vault tests');
  });

  it('a conversation opened after its task was deleted doesn\'t fetch the task back (#1709)', async () => {
    const { tasks, live } = await load(true);
    const event = await following(live, [job()]);
    event('job', { id: 'j1', deleted: true });
    answers['GET /v1/conversation?id=c1&limit=100'] = { status: 200, body: [{ id: 't1', who: 'oby', text: 'Done.', at: '2026-10-30T10:20:00Z', jobs: ['j1'], conversation: 'c1' }] };
    answers['GET /v1/jobs/j1'] = { status: 200, body: job() };
    await live.openConversation('c1');
    for (let i = 0; i < 3; i++) await flush();
    expect(tasks.dev!.get()).toEqual([]);
  });

  it('words that start a conversation: the turns other conversations got while its own were read stay', async () => {
    const { live } = await load(true);
    const event = await following(live, []);
    const turn = (id: string, conversation: string, who = 'user') => ({ id, who, text: id, at: '2026-11-01T10:00:00Z', jobs: [], conversation });
    answers['POST /v1/plans'] = { status: 200, body: { kind: 'reply', text: 'Hi.', jobs: [], conversation: 'c1' } };
    const { hold, go } = held();
    answers['GET /v1/conversation?id=c1&limit=100'] = { status: 200, body: [turn('t1', 'c1'), turn('t2', 'c1', 'oby')], hold };
    live.ask('Hello');
    for (let i = 0; i < 3; i++) await flush();
    event('turn', turn('t3', 'c2'));
    go();
    for (let i = 0; i < 3; i++) await flush();
    expect(Object.keys(live.liveState().turns).sort()).toEqual(['c1', 'c2']);
  });
});
