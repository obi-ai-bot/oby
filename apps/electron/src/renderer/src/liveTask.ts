import type { TranscriptEntry } from './obid';
import type { Entry } from './session';

// A job's session as it comes in (docs/contracts/live-task.md), and what a task's live card takes
// from it. Pure, so it's tested (test/liveTask.test.ts).

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
// A line the agent just wrote, on the end of its entry's words (already there: a fetch brought it)
export const withLine = (had: TranscriptEntry[], n: number, line: string) =>
  had.map((e) => (e.n !== n || (line.trim() && e.text?.endsWith(line)) ? e : { ...e, text: (e.text ?? '') + line }));
// The last n with none missing before it: what to ask `after` for, so a lost event comes too
export function knownUpTo(entries: TranscriptEntry[]) {
  const have = new Set(entries.map((e) => e.n));
  let n = 0;
  while (have.has(n + 1)) n++;
  return n;
}

// seconds as "0.2s", "41s" or "3m 12s"
export const took = (s: number) => (s < 10 ? `${Math.round(s * 10) / 10}s` : s < 60 ? `${Math.round(s)}s` : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`);

export type Todo = Extract<Entry, { kind: 'plan' }>['todos'][number];
export type Step = Extract<Entry, { kind: 'tool' | 'said' | 'thought' }>;

// Its to-do list as the agent last wrote it. None until it makes one: there's nothing to invent.
export const todosOf = (entries: Entry[]) => entries.filter((e) => e.kind === 'plan').at(-1)?.todos;
// The last things it did, oldest first
export const lastSteps = (entries: Entry[], keep = 3) => entries.filter((e): e is Step => e.kind === 'tool' || e.kind === 'said' || e.kind === 'thought').slice(-keep);
// A long list on a card: `most` of them, around the one it's on, and how many are left out each side
export function around(todos: Todo[], most = 8) {
  if (todos.length <= most) return { todos, before: 0, after: 0 };
  const on = todos.findIndex((t) => t.state === 'running');
  const start = Math.max(0, Math.min((on < 0 ? todos.findIndex((t) => t.state !== 'done') : on) - 2, todos.length - most));
  return { todos: todos.slice(start, start + most), before: start, after: todos.length - start - most };
}
// A thought in words, only what its tool gave: its own, or how long it took, or that it was one
export const thoughtWords = (e: Extract<Entry, { kind: 'thought' }>) =>
  (e.running ? 'Thinking' : e.text || (e.seconds ? `Thought for ${took(e.seconds)}` : 'Thought'));
