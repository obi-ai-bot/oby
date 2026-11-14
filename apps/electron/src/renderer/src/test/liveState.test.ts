import { describe, expect, it } from 'vitest';
import { answer, apply, type ObidState } from '../../../shared/live-state';
import { parseEvent, type Event, type Job } from '../../../shared/obid-wire';

// The one rule jobs reach the app's state by (docs/contracts/live-state.md, LIVE-1 to LIVE-4)
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'running', step: 'Working', summary: '', created: '2026-10-30T10:00:00Z', finished: null,
  commits: 0, published: null, rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null,
  title: 'Fix it', ...more,
});
const empty: ObidState = { jobs: {}, gone: new Set() };
const holding = (j: Job): ObidState => ({ jobs: { [j.id]: j }, gone: new Set() });

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
    const start: ObidState = { jobs: { j1: job({ revision: 2 }), j2: job({ id: 'j2', revision: 1 }) }, gone: new Set(['j9']) };
    const before = structuredClone(start);
    const events: Extract<Event, { kind: 'job' | 'jobGone' }>[] = [
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
  });
});
