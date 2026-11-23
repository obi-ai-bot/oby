import { agentOf, ENDED } from './view';
import type { Task } from './tasks';

// A conversation's tasks as the side panel nests them under its row (Sangam's screens 03 and 04,
// docs/contracts/live-task.md LIVETASK-19 to 23): each task a session of its own under the
// conversation that started it, in the order they started, with its live state.

// How a nested task stands: at work (queued or running), waiting on the owner, done, or stopped by
// a failure; unknown: a status this build doesn't know (docs/contracts/wire.md, WIRE-3)
export type KidState = 'working' | 'needs' | 'done' | 'failed' | 'unknown';
export const kidState = (t: Task): KidState =>
  t.unknown ? 'unknown'
    : t.status === 'needs' ? 'needs'
      : t.status === 'running' ? 'working'
        : ENDED[t.job.status] ? 'failed' : 'done';

// The tasks a conversation started, oldest first, archived ones left out: `of` names the
// conversation a task belongs to (chatOfTask, which reads obid's `conversation` on the job, or the
// mock's message that started it)
export const kidsOf = (tasks: Task[], chat: string, of: (taskId: string) => string | undefined) =>
  tasks.filter((t) => !t.job.archived && of(t.id) === chat).sort((a, b) => b.job.created - a.job.created);

// What a task waits on, as the words after its agent's name: "asks to run a command"
export function asksFor(t: Task): string {
  const ask = t.job.ask;
  if (!ask) return 'needs you';
  if (ask.kind === 'permission') return ask.detail ? 'asks to run a command' : 'asks for permission';
  if (ask.kind === 'question') return 'has a question';
  if (ask.kind === 'merge') return 'is ready to merge';
  if (ask.kind === 'failed') return 'stopped and needs you';
  if (ask.kind === 'limit') return 'hit a usage limit';
  return 'needs you';
}

// A nested row's line after its title: what it's on now, what it waits on, or how it ended
export function kidLine(t: Task): string {
  const j = t.job;
  switch (kidState(t)) {
    case 'needs': return asksFor(t);
    case 'working': return j.status === 'queued' ? 'Queued' : j.step || 'Working';
    case 'failed': return ENDED[j.status] ?? 'Failed';
    case 'done': return j.nothingDone ? 'Nothing done' : 'Done';
    default: return j.status;
  }
}

// The conversation row's second line while its tasks are what's going on (screen 03: "2 agents
// working"; screen 04: "Codex asks to run a command"): what waits on the owner first, then how many
// are at work; nothing (undefined) when none is, so the row keeps its own line
export function kidsLine(kids: Task[]): string | undefined {
  const needs = kids.find((t) => kidState(t) === 'needs');
  if (needs) return `${agentOf(needs.job.agent).name} ${asksFor(needs)}`;
  const n = kids.filter((t) => kidState(t) === 'working').length;
  return n ? `${n} agent${n === 1 ? '' : 's'} working` : undefined;
}

// Its row's title when hovered: the task and the account it runs on (screen 03's title attribute)
export const kidTitle = (t: Task) => (t.job.account ? `${t.title} on ${t.job.account}` : t.title);

// Which conversations' rows the owner folded or showed with its twist (LIVETASK-20), kept as
// oby.kidsShown: conversation id to shown or not
const KIDS = 'oby.kidsShown';
export function keptKids(): Record<string, boolean> {
  try {
    const kept: unknown = JSON.parse(localStorage.getItem(KIDS) ?? '{}');
    return kept && typeof kept === 'object' && !Array.isArray(kept) ? Object.fromEntries(Object.entries(kept).filter(([, v]) => typeof v === 'boolean')) as Record<string, boolean> : {};
  } catch { return {}; }
}
export function keepKids(next: Record<string, boolean>) {
  try { localStorage.setItem(KIDS, JSON.stringify(next)); } catch { /* not remembered, that's all */ }
}
// A task's own thread opened (its card, its nested row): its conversation's rows show once, the lit
// one among them, even folded by the owner before; the twist folds them again (LIVETASK-22)
const revealed = new Set<(chat: string) => void>();
export function revealKids(chat: string) {
  const kept = keptKids();
  if (kept[chat] !== false) return;
  delete kept[chat];
  keepKids(kept);
  for (const f of revealed) f(chat);
}
export const onRevealKids = (f: (chat: string) => void) => { revealed.add(f); return () => { revealed.delete(f); }; };
