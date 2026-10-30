import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// The task's menu wired to obid (docs/contracts/tasks.md, TASKS-1 to TASKS-3, TASKS-10): what the
// app sends obid through main's bridge (window.app.obid), and what the task does with the answer.

type Answer = { status: number; body?: unknown };
const calls: { method: string; path: string; body?: unknown }[] = [];
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
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  vi.stubGlobal('window', withObid ? { app: { obid: { call } } } : {});
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

  it('TASKS-3: Delete sends DELETE /v1/jobs/{id}; a 409 leaves the task, in obid\'s words; once deleted it\'s gone', async () => {
    const { tasks, live } = await load(true);
    await seed(tasks);
    answers['DELETE /v1/jobs/j1'] = { status: 409, body: { error: 'Its pull request is still open: close or merge it first, or archive the job.' } };
    tasks.deleteTask('j1');
    await flush();
    expect(calls).toEqual([{ method: 'DELETE', path: '/v1/jobs/j1', body: undefined }]);
    expect(live.liveState().problem).toBe('Its pull request is still open: close or merge it first, or archive the job.');
    expect(tasks.dev!.get().map((t) => t.id)).toEqual(['j1']);

    answers['DELETE /v1/jobs/j1'] = { status: 204 };
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
});
