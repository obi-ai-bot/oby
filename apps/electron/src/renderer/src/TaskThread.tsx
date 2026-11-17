import { useEffect, useRef } from 'react';
import AgentThread from './AgentThread';
import { TaskHead } from './ChatView';
import { typingIn } from './Composer';
import { useTasks, type Task } from './tasks';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;

// A task's own thread, a conversation of its own (docs/contracts/live-task.md, LIVETASK-22): opened
// from its nested row under the conversation that started it, or from its card there, it lies over
// that conversation, and back (the title bar, or Esc) returns to it. Its head as its card shows it,
// then the agent's conversation (AgentThread): what it was told, its words, steps and thinking, its
// to-dos, the decision it waits on, and the box to tell it something with Stop while it runs.
export default function TaskThread({ id, leaving, covered = false, onBack, onGone, onOpenAgent }: {
  id: string;
  leaving: boolean;
  covered?: boolean;
  onBack: () => void;
  onGone: () => void;
  onOpenAgent: (id: string) => void;  // told something once it's finished: the follow-up it started
}) {
  const tasks = useTasks();
  // deleted, it's gone while this sinks away: keep showing the last of it
  const kept = useRef<Task | null>(null);
  const task = tasks.find((t) => t.id === id) ?? kept.current;
  if (task) kept.current = task;

  // a timer, not animationend: animations don't run while the window is hidden
  useEffect(() => {
    if (!leaving) return;
    const t = setTimeout(onGone, LEAVE_MS);
    return () => clearTimeout(t);
  }, [leaving, onGone]);

  useEffect(() => {
    if (covered) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape' && !typingIn(e)) onBack(); };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [onBack, covered]);

  if (!task) return null;
  return (
    <div className={`chat-layer agent-layer${leaving ? ' is-leaving' : ''}${covered ? ' is-covered' : ''}`} inert={covered}>
      <AgentThread id={task.id} onFollowUp={onOpenAgent} head={<div className="msg-task agent-head">{<TaskHead task={task} />}</div>} />
    </div>
  );
}
