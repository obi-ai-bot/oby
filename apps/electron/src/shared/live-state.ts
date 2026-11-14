// What obid has, as the app keeps it, and the one rule its events and answers follow over it
// (docs/contracts/live-state.md). Pure: no I/O, no module state and nothing of the screens, so
// main can run it too. Jobs only for now; the other kinds are still live.ts's.
import type { Event, Job } from './obid-wire';

export type ObidState = {
  jobs: Record<string, Job>;
  gone: ReadonlySet<string>;  // the jobs obid said it deleted: never back from an answer that crossed it
};

// A job, from an event or an answer, over what's held: a deleted one stays out; of two copies, the
// newer by obid's revision, as an answer can come after an event that's newer than it (#1709). One
// with none (an older obid, after a downgrade) is taken as it comes, never held off by a revision
// from before
const taken = (s: ObidState, j: Job): ObidState => {
  if (s.gone.has(j.id)) return s;
  const had = s.jobs[j.id];
  return had && j.revision !== undefined && (had.revision ?? 0) > j.revision ? s : { ...s, jobs: { ...s.jobs, [j.id]: j } };
};

export function apply(s: ObidState, e: Extract<Event, { kind: 'job' | 'jobGone' }>): ObidState {
  if (e.kind === 'job') return taken(s, e.job);
  const { [e.id]: _, ...jobs } = s.jobs;
  return { ...s, jobs, gone: new Set([...s.gone, e.id]) };
}

// a job a call answered with: the same rule as its event
export const answer = (s: ObidState, j: Job): ObidState => taken(s, j);
