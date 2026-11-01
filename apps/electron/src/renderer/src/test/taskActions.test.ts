import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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
  beforeEach(() => { answers = {}; });
  afterEach(() => { vi.unstubAllGlobals(); });

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
    expect(live.liveState().sessions.j1.map((e) => e.n)).toEqual([1, 2, 3]);

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
    const session = live.liveState().sessions.j1;
    expect(session.map((e) => e.n).sort()).toEqual([1, 2, 3, 4]);
    expect(session.find((e) => e.n === 3)!.state).toBe('done');
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
    ['POST /v1/jobs/j1/dismiss', (l) => l.answerJob('j1', 'Dismiss')],
  ];
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
