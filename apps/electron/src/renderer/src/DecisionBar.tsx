import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { AskBlock } from './TaskCard';
import type { Task } from './tasks';

// A decision waiting on the owner, as the drawer shows it: key, what it's kept by in this spot (a
// task's id, a limit card's); from, the task it's from (in a conversation, which may hold several)
export type Pending = { key: string; task: Pick<Task, 'id' | 'answer'> & { job: Pick<Task['job'], 'ask'> }; from?: string };

// The decisions waiting on the owner, as a drawer on top of the message box (docs/contracts/live-task.md,
// LIVETASK-24 to 27; T3 Code's composer drawer and its child thread's approvals over the composer): one
// at a time, on one line (AskBlock's bar), with "1 of 3" and the next one's arrow when there are
// several. Later takes one off this drawer only, while the view is open: it stays the task's
// decision, its card and rows still say it needs the owner, and opening the view again shows it
// again. Nothing here answers or puts off anything in obid.
export default function DecisionBar({ pending }: { pending: Pending[] }) {
  // put off here, each by its decision (a new ask from the same task shows again)
  const [later, setLater] = useState<string[]>([]);
  const [at, setAt] = useState(0);
  const idOf = (p: Pending) => `${p.key}\n${p.task.job.ask?.headline ?? ''}`;
  const shown = pending.filter((p) => p.task.job.ask && !later.includes(idOf(p)));
  if (!shown.length) return null;
  const i = Math.min(at, shown.length - 1), p = shown[i];
  const n = shown.length;
  const count = n > 1 && (
    <span className="decision-count">
      {i + 1} of {n}
      <button type="button" className="decision-next" aria-label="Next decision" onClick={() => setAt((i + 1) % n)}>
        <ChevronRight size={14} strokeWidth={1.8} />
      </button>
    </span>
  );
  return (
    <div className="decision-bar" role="group" aria-label="Waiting for your decision">
      <div className="decision-card">
        {/* a decision put off elsewhere (obid's later, an earlier build's) still waits here unanswered */}
        <AskBlock
          key={idOf(p)}
          task={p.task.answer === 'Later' ? { ...p.task, answer: undefined } : p.task}
          inChat
          bar={{ from: p.from, count, onLater: () => setLater([...later, idOf(p)]) }}
        />
      </div>
    </div>
  );
}
