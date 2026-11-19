import { Check } from 'lucide-react';
import type { Task } from './tasks';
import { ENDED } from './view';

// A task's state as one mark in one spot, wherever the task is listed:
// new (blue, until you've opened it), needs you (orange), working (a grey dot
// pulsing slowly), done (a small grey check). Queued has no mark: being in the
// list unstarted says it (the spot stays, so rows line up).
// live: the state alone, leaving out new (where you're already looking at it).
// A status obid sent that this build doesn't know: a still grey dot, named as obid names it
// (docs/contracts/wire.md, WIRE-3). A failed or interrupted one isn't done's check: a red dot,
// named as the conversation's card names it (TASKS-25).
type DotState = 'new' | 'needs' | 'working' | 'queued' | 'done';
const dotState = (t: Task, live = false): DotState =>
  !live && t.fresh ? 'new'
    : t.status === 'needs' ? 'needs'
      : t.status === 'done' ? 'done'
        : t.job.status === 'queued' ? 'queued' : 'working';
const LABEL: Record<DotState, string> = { new: 'New', needs: 'Needs you', working: 'Working', queued: 'Queued', done: 'Done' };

export default function StatusDot({ task, live = false }: { task: Task; live?: boolean }) {
  if (task.unknown) return <span className="status" role="img" aria-label={task.job.status}><span className="status-dot is-unknown" /></span>;
  const state = dotState(task, live);
  if (state === 'done' && (task.job.status === 'failed' || task.job.status === 'interrupted')) {
    return <span className="status" role="img" aria-label={ENDED[task.job.status]}><span className="status-dot is-failed" /></span>;
  }
  return (
    <span className="status" role="img" aria-label={LABEL[state]}>
      {state === 'done' ? <Check size={12} strokeWidth={2} /> : state !== 'queued' && <span className={`status-dot is-${state}`} />}
    </span>
  );
}

// A task's mark where it ended or goes on, as its card in the conversation shows it: stopped or failed,
// a red dot named as the card names it ("Stopped", "Failed"); else StatusDot's, live (no "new")
export function TaskMark({ task }: { task: Task }) {
  const ended = task.status === 'done' && !task.unknown ? ENDED[task.job.status] : undefined;
  return ended ? <span className="status" role="img" aria-label={ended}><span className="status-dot is-failed" /></span> : <StatusDot task={task} live />;
}
