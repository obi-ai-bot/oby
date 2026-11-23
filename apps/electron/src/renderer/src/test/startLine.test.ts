// @vitest-environment happy-dom
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Job } from '../obid';

// Oby's line that a task started, as a task's own thread opens with it (docs/contracts/live-task.md,
// LIVETASK-22), and Oby's line that a follow-up started (docs/contracts/task-conversations.md,
// TASKCONV-41, 43): the mode it runs in is named, and the account is labelled as one, so an
// account named like a mode (Claude Code's "Default") is never read as the mode.
let live: typeof import('../live');
let mock: typeof import('../mock');
beforeAll(async () => {
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call: vi.fn() }, openExternal: vi.fn() } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  mock = await import('../mock');
}, 120_000);
afterAll(() => vi.unstubAllGlobals());

const job = (more: Partial<Job> = {}) => ({
  id: 'j1', repo: 'well-prado/blok', title: 'Greeting fix', request: 'Fix the greeting', agent: 'claude', account: 'acc-1', branch: 'oby/greeting', base: 'main',
  issue: null, kind: 'fix', status: 'running', step: '', summary: '', created: new Date().toISOString(), finished: null, commits: null, published: null,
  rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null, conversation: null, mode: 'ask', ...more,
}) as Job;

describe('TASKCONV-41: a task’s own thread opens with its mode and its account named', () => {
  it('taskconv_41_the_threads_start_line_names_the_mode_and_the_account', () => {
    const s = live.liveState();
    s.status = { ...(s.status ?? {}), models: {}, accounts: [{ id: 'acc-1', agent: 'claude', name: 'Default', signedIn: true }] } as never;
    const thread = mock.threadOf(live.taskOf(s, job()).job);
    const oby = thread.filter((m) => m.who === 'oby').map((m) => m.text);
    expect(oby).toContain('On it. Claude Code has it in blok, in Ask mode, on the account Default.');
    expect(oby.join('\n')).not.toMatch(/ on Default\./);
    // the job's own mode, whichever it is
    const edits = mock.threadOf(live.taskOf(s, job({ mode: 'acceptEdits' })).job);
    expect(edits.map((m) => m.text)).toContain('On it. Claude Code has it in blok, in Accept edits mode, on the account Default.');
  });
});
