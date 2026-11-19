// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Job } from '../obid';

// A task's local servers (docs/contracts/task-servers.md, TASKSERVERS-7 and 8): the line shows each
// port obid lists on the job, and Open asks main, which opens http://localhost:<port> only for a
// port obid lists on that job now.

const the = vi.hoisted(() => ({ jobs: {} as Record<string, unknown>, asked: [] as string[], input: true }));
vi.mock('electron', () => ({ shell: { openExternal: async () => {} }, ipcMain: { handle() {}, on() {} } }));
vi.mock('../../../main/obid', () => ({
  obid: async (method: string, path: string) => {
    the.asked.push(`${method} ${path}`);
    const id = decodeURIComponent(path.split('/').at(-1)!);
    return id in the.jobs ? { status: 200, text: JSON.stringify(the.jobs[id]) } : { status: 404, text: '{"error":"There’s no such job."}' };
  },
}));
vi.mock('../../../main/words', () => ({ takeInput: () => the.input }));

const call = vi.fn();
let live: typeof import('../live');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
}, 120_000);
afterAll(() => vi.unstubAllGlobals());

const job = (more: Partial<Job> = {}) => ({
  id: 'j1', repo: 'o/app', title: 'Run the app', request: 'Run the app', agent: 'claude', account: 'stub', branch: 'oby/run', base: 'main',
  issue: null, kind: 'build', status: 'running', step: '', summary: '', created: new Date().toISOString(), finished: null, commits: null, published: null,
  rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null, conversation: null, ...more,
}) as Job;
const SERVERS = [{ port: 3000, process: 'node', firstSeen: '2026-11-19T10:00:00Z' }, { port: 5173, process: 'vite', firstSeen: '2026-11-19T10:00:05Z' }];

let root: Root | undefined;
let host: HTMLElement;
const opened = vi.fn(async (_job: string, _port: number) => true);
beforeEach(() => {
  window.app = { obid: { call }, openTaskServer: opened } as never;
  host = document.createElement('div');
  document.body.append(host);
});
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; opened.mockClear(); });

async function line(ports?: number[]) {
  const { default: TaskServers } = await import('../TaskServers');
  root = createRoot(host);
  await act(async () => root!.render(<TaskServers job="j1" ports={ports} />));
}

describe('TASKSERVERS-7: the task shows its local servers', () => {
  it('taskservers_7_each_port_obid_lists_shows_localhost_and_open', async () => {
    const task = live.taskOf(live.liveState(), job({ servers: SERVERS }));
    expect(task.job.servers).toEqual([3000, 5173]);
    await line(task.job.servers);
    expect(host.textContent).toBe('localhost:3000 · Openlocalhost:5173 · Open');
    expect([...host.querySelectorAll('button')].map((b) => b.getAttribute('aria-label'))).toEqual(['Open localhost:3000', 'Open localhost:5173']);
  });

  it('taskservers_7_a_job_with_none_shows_no_line', async () => {
    expect(live.taskOf(live.liveState(), job()).job.servers).toBeUndefined();
    expect(live.taskOf(live.liveState(), job({ servers: [] })).job.servers).toBeUndefined();
    await line(undefined);
    expect(host.innerHTML).toBe('');
  });
});

describe('TASKSERVERS-8: Open asks main with the job and the port, and main opens only what obid lists', () => {
  beforeEach(() => { the.jobs = { j1: { id: 'j1', servers: SERVERS }, j2: { id: 'j2' } }; the.asked = []; the.input = true; });

  it('taskservers_8_open_sends_only_the_jobs_id_and_the_port', async () => {
    await line([3000, 5173]);
    await act(async () => host.querySelectorAll('button')[1].click());
    expect(opened).toHaveBeenCalledWith('j1', 5173);
  });

  it('taskservers_8_main_opens_localhost_on_a_port_obid_lists_for_that_job', async () => {
    const { openServer } = await import('../../../main/servers');
    const urls: string[] = [];
    const open = async (url: string) => { urls.push(url); };
    expect(await openServer('j1', 5173, open)).toBe(true);
    expect(urls).toEqual(['http://localhost:5173']);
    expect(the.asked).toEqual(['GET /v1/jobs/j1']);
  });

  it('taskservers_8_a_port_obid_doesnt_list_another_job_or_no_job_opens_nothing', async () => {
    const { openServer } = await import('../../../main/servers');
    const urls: string[] = [];
    const open = async (url: string) => { urls.push(url); };
    expect(await openServer('j1', 8080, open)).toBe(false);  // not one obid lists
    expect(await openServer('j2', 3000, open)).toBe(false);  // another job's: it lists none
    expect(await openServer('j9', 3000, open)).toBe(false);  // a job obid doesn't have
    expect(await openServer('../j1', 3000, open)).toBe(false);  // not a job's id: obid isn't asked
    expect(await openServer('j1', 0, open)).toBe(false);
    expect(await openServer('j1', 70000, open)).toBe(false);
    expect(await openServer('j1', 3000.5, open)).toBe(false);
    expect(urls).toEqual([]);
    expect(the.asked).toEqual(['GET /v1/jobs/j1', 'GET /v1/jobs/j2', 'GET /v1/jobs/j9']);
  });

  it('taskservers_8_the_page_alone_opens_nothing_without_a_click_main_saw', async () => {
    const handlers = new Map<string, (...args: unknown[]) => unknown>();
    vi.doMock('../../../main/ipc', () => ({ ipc: () => ({ handle: (name: string, _d: unknown, f: (...args: unknown[]) => unknown) => handlers.set(name, f) }) }));
    vi.resetModules();
    const { servers } = await import('../../../main/servers');
    servers(() => true);
    the.input = false;
    expect(await handlers.get('open-task-server')!({}, 'j1', 3000)).toBe(false);
    expect(the.asked).toEqual([]);
    vi.doUnmock('../../../main/ipc');
  });
});
