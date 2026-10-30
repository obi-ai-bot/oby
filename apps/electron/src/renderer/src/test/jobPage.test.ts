import { describe, expect, it } from 'vitest';
import { outgoingText, pageOf, prName, prUrl, sessionOf, span, stagesOf } from '../jobPage';
import type { Account, Job } from '../obid';

// The task page on obid's job data (docs/contracts/tasks.md, TASKS-4 to TASKS-9)
const NOW = Date.parse('2026-10-30T12:00:00Z');
const ago = (s: number) => new Date(NOW - s * 1000).toISOString();
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'running', step: 'Editing scan.rs', summary: '', created: ago(900), finished: null, commits: null, published: null,
  rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, title: 'Fix it',
  pipeline: { number: 2, total: 3, stages: [
    { id: 'prepare', title: 'Prepare', state: 'done', started: ago(600), finished: ago(578), seconds: 22 },
    { id: 'work', title: 'Work', state: 'current', started: ago(578), waited: 0 },
    { id: 'publish', title: 'Publish', state: 'upcoming', estimate: { low: 20, high: 40 } },
  ] },
  ...more,
});
const account = { id: 'a1', agent: 'claude', name: 'Personal', signedIn: true, fiveHour: 0.46, sevenDay: 0.3, sevenDayResets: null, running: 1 } as Account;

describe('the task page on obid data', () => {
  it('TASKS-4: the pull request link is its short name and opens its page on GitHub', () => {
    expect(prName('https://github.com/deskree-inc/oby/pull/375')).toBe('oby#375');
    expect(prUrl('https://github.com/deskree-inc/oby/pull/375')).toBe('https://github.com/deskree-inc/oby/pull/375');
    // the mock's
    expect(prName('deskree-inc/tetrix-ee#493')).toBe('tetrix-ee#493');
    expect(prUrl('deskree-inc/tetrix-ee#493')).toBe('https://github.com/deskree-inc/tetrix-ee/pull/493');
    expect(prUrl('not a link')).toBeUndefined();
  });

  it('TASKS-5: the timeline from the stage times and the estimate', () => {
    const stages = stagesOf(job(), NOW);
    expect(stages.map((s) => s.took)).toEqual(['22s', '9m 38s so far', '≈ 30s']);
    const running = pageOf(job({ estimate: { low: 720, high: 1200, basis: 'From the last 14 jobs like this.', samples: 14 } }), account, false, NOW);
    // 22s + 578s = 10m in; 12–20m in all
    expect(running.time).toMatchObject({ aside: '10m in · done in about 2–10m', basis: 'From the last 14 jobs like this.' });
    expect(running.time!.done).toBeLessThan(running.time!.from!);
    expect(running.time!.to).toBe(97);
    expect(pageOf(job({ estimate: { low: 720, high: 1200, basis: '', samples: 14 } }), account, true, NOW).time!.paused).toBe(true);
    const ended = job({
      status: 'done', estimate: { low: 720, high: 1200, basis: '', samples: 14, firstGiven: { low: 600, high: 1080 } },
      pipeline: { number: 3, total: 3, stages: [{ id: 'work', title: 'Work', state: 'done', seconds: 1500 }] },
    });
    expect(pageOf(ended, account, false, NOW).time).toEqual({ aside: 'Took 25m · estimated 10–18m' });
    expect(pageOf({ ...ended, status: 'failed' }, account, false, NOW).time).toEqual({ aside: 'Ran 25m · jobs like this: 12–20m' });
    // no estimate yet: Stages, no track
    expect(pageOf(job(), account, false, NOW).time).toBeUndefined();
    expect(span(3840)).toBe('1h 4m');
  });

  it('TASKS-6: the activity under Work, counted and grouped, the agent\'s latest words as its line', () => {
    const j = job({
      context: 'Changing the sort to oldest first.',
      activity: {
        counts: { read: 31, search: 1, edit: 6, command: 0, test: 2 },
        groups: [
          { id: 'g1', title: 'Read 14 files', by: 'obid', state: 'done', started: ago(500), finished: ago(440) },
          { id: 'g2', title: 'Changing the sort', by: 'agent', state: 'running', started: ago(120), files: ['Mac/App/LivePanel.swift'], diff: { added: 12, removed: 5 },
            tools: [{ tool: 'Read', kind: 'read', summary: 'Reading Engine/Inbox.swift', state: 'running', started: ago(3) }] },
        ],
      },
    });
    const page = pageOf(j, account, false, NOW);
    expect(page.counts).toEqual([[31, 'read'], [1, 'search'], [6, 'edits'], [2, 'test runs']]);
    expect(page.groups![0]).toMatchObject({ title: 'Read 14 files', state: 'done', took: '1m' });
    expect(page.groups![1]).toMatchObject({ meta: 'LivePanel.swift · +12 −5', took: '2m', tools: [{ verb: 'Reading', path: 'Engine/Inbox.swift', running: true }] });
    const work = stagesOf(j, NOW)[1];
    expect(work).toMatchObject({ activity: true, sub: 'Changing the sort to oldest first.' });
  });

  it('TASKS-7: usage only when a plan covers it, with the job\'s share of each window', () => {
    const j = job({ usage: { input: 100_000, output: 50_000, cacheRead: 2_850_000, costUsd: 4.2, priced: 'plan' }, windowShare: { fiveHour: 11.2, sevenDay: 0.4, estimate: true } });
    expect(pageOf(j, account, false, NOW).usage).toEqual({ tokens: '3.0M', cost: '$4.20', fiveHour: 46, job5: '11%', week: 30, jobWeek: '<1%' });
    // an API key pays: Sangam's words ("covered by your plan") wouldn't be true
    expect(pageOf(job({ usage: { input: 1, costUsd: 1, priced: 'api' } }), account, false, NOW).usage).toBeUndefined();
    // the account's windows not known yet
    expect(pageOf(j, { ...account, fiveHour: null }, false, NOW).usage).toBeUndefined();
  });

  it('TASKS-8: what went out, every outgoing action in words', () => {
    expect(outgoingText({ action: 'push', repo: 'deskree-inc/oby', branch: 'oby/fix-75', id: 'abc', date: ago(60) })).toBe('Pushed oby/fix-75 to deskree-inc/oby');
    expect(outgoingText({ action: 'pullRequest', id: 'https://github.com/deskree-inc/oby/pull/375', date: ago(60) })).toBe('Opened pull request oby#375, ready for review');
    expect(outgoingText({ action: 'merge', id: 'https://github.com/deskree-inc/oby/pull/375', branch: 'dev', date: ago(60) })).toBe('Merged oby#375 into dev');
    const page = pageOf(job({ outgoing: [{ action: 'push', repo: 'deskree-inc/oby', branch: 'oby/fix-75', date: ago(60) }] }), account, false, NOW);
    expect(page.outgoing).toHaveLength(1);
    expect(page.outgoing![0].at).toMatch(/\d/);
    expect(pageOf(job({ outgoing: [] }), account, false, NOW).outgoing).toBeUndefined();
  });

  it('TASKS-9: the full session from obid\'s transcript, in order', () => {
    const lines = sessionOf([
      { n: 3, at: ago(5), kind: 'tool', tool: 'Bash', activity: 'command', summary: 'Running cargo test', state: 'failed', seconds: 41.5, exit: 101 },
      { n: 1, at: ago(9), kind: 'start', summary: 'Started on Claude Code' },
      { n: 2, at: ago(8), kind: 'said', summary: 'Looking…', text: 'Looking for where a blocked scan ends the job.' },
      { n: 4, at: ago(4), kind: 'tool', tool: 'Edit', activity: 'edit', summary: 'Editing scan.rs', state: 'done', seconds: 0.1, added: 3, removed: 2 },
      { n: 5, at: ago(2), kind: 'tool', tool: 'Read', activity: 'read', summary: 'Reading lib.rs', state: 'running' },
      { n: 6, at: ago(1), kind: 'plan', summary: '1 of 2 done', todos: [{ text: 'Find it', state: 'done' }, { text: 'Fix it', state: 'running' }] },
    ], NOW);
    expect(lines.map((l) => l.kind)).toEqual(['start', 'said', 'tool', 'tool', 'tool', 'plan']);
    expect(lines[1]).toMatchObject({ text: 'Looking for where a blocked scan ends the job.' });
    expect(lines[2]).toMatchObject({ tool: 'Bash', verb: 'Running', target: 'cargo test', state: 'failed', meta: 'exit 101', seconds: 41.5 });
    expect(lines[3]).toMatchObject({ tool: 'Edit', meta: '+3 −2' });
    expect(lines[4]).toMatchObject({ tool: 'Read', state: 'running', seconds: 2 });
  });
});
