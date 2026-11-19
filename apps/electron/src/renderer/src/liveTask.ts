import type { TranscriptEntry } from './obid';

// A job's session as it comes in (docs/contracts/live-task.md). Pure, so it's tested (test/liveTask.test.tsx).

// Entries over those already here, by n: a call comes again when it ends. A fetch answered late
// doesn't undo what an event already said: an ended call stays ended, and words stay as far as they got.
// What streams while the agent works (LIVETASK-32, 34) stays through a fetch: the words past the last
// whole line until an event says the entry anew, a call's last lines while it runs.
export function withEntries(had: TranscriptEntry[], entries: TranscriptEntry[]) {
  const by = new Map(had.map((e) => [e.n, e]));
  for (const e of entries) {
    const was = by.get(e.n);
    if (was?.state && was.state !== 'running' && e.state === 'running') continue;
    const further = !!was?.text && !!e.text && was.text.length > e.text.length && was.text.startsWith(e.text);
    let now = further ? { ...e, text: was!.text } : e;
    if (e.fetched && was?.tail !== undefined && (further || was.text === e.text)) now = { ...now, tail: was.tail };
    if (was?.output && now.state === 'running') now = { ...now, output: was.output };
    by.set(e.n, now);
  }
  return [...by.values()];
}
// A line the agent just wrote, on the end of its entry's words. Only right after a fetch can that
// line be there already, its event still on the way.
// ponytail: an identical line next after a fetch is dropped too, until the message's final copy
// The words shown past the last line are in it now: what's past it comes next (still being written).
export const withLine = (had: TranscriptEntry[], n: number, line: string) => had.map((e) => {
  if (e.n !== n) return e;
  const { fetched, ...rest } = e;
  return fetched && e.text?.endsWith(line) ? { ...rest, tail: '' } : { ...rest, text: (e.text ?? '') + line, tail: '' };
});
// The words it's writing past its last whole line, masked by obid: each replaces the one before
export const withTail = (had: TranscriptEntry[], n: number, tail: string) => had.map((e) => (e.n === n ? { ...e, tail } : e));
// A running command's last lines, masked by obid: each replaces the one before; an ended call keeps its own
export const withOutput = (had: TranscriptEntry[], n: number, output: { text: string; lines: number }) =>
  had.map((e) => (e.n === n && (e.state ?? 'running') === 'running' ? { ...e, output } : e));
// The last n with none missing before it: what to ask `after` for, so a lost event comes too
export function knownUpTo(entries: TranscriptEntry[]) {
  const have = new Set(entries.map((e) => e.n));
  let n = 0;
  while (have.has(n + 1)) n++;
  return n;
}

// seconds as "0.2s", "41s" or "3m 12s"
export const took = (s: number) => (s < 10 ? `${Math.round(s * 10) / 10}s` : s < 60 ? `${Math.round(s)}s` : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`);
