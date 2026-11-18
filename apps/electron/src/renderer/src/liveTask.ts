import type { TranscriptEntry } from './obid';

// A job's session as it comes in (docs/contracts/live-task.md). Pure, so it's tested (test/liveTask.test.tsx).

// Entries over those already here, by n: a call comes again when it ends. A fetch answered late
// doesn't undo what an event already said: an ended call stays ended, and words stay as far as they got.
export function withEntries(had: TranscriptEntry[], entries: TranscriptEntry[]) {
  const by = new Map(had.map((e) => [e.n, e]));
  for (const e of entries) {
    const was = by.get(e.n);
    if (was?.state && was.state !== 'running' && e.state === 'running') continue;
    by.set(e.n, was?.text && e.text && was.text.length > e.text.length && was.text.startsWith(e.text) ? { ...e, text: was.text } : e);
  }
  return [...by.values()];
}
// A line the agent just wrote, on the end of its entry's words. Only right after a fetch can that
// line be there already, its event still on the way.
// ponytail: an identical line next after a fetch is dropped too, until the message's final copy
export const withLine = (had: TranscriptEntry[], n: number, line: string) => had.map((e) => {
  if (e.n !== n) return e;
  const { fetched, ...rest } = e;
  return fetched && e.text?.endsWith(line) ? rest : { ...rest, text: (e.text ?? '') + line };
});
// The last n with none missing before it: what to ask `after` for, so a lost event comes too
export function knownUpTo(entries: TranscriptEntry[]) {
  const have = new Set(entries.map((e) => e.n));
  let n = 0;
  while (have.has(n + 1)) n++;
  return n;
}

// seconds as "0.2s", "41s" or "3m 12s"
export const took = (s: number) => (s < 10 ? `${Math.round(s * 10) / 10}s` : s < 60 ? `${Math.round(s)}s` : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`);
