import { useLayoutEffect, useRef, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { BrandMark } from './brands';
import { AskBlock } from './TaskCard';
import { tasksWith, useTasks, type Task } from './tasks';

// What Oby is asking you, on Home, under the attention line: one card per task
// that needs you, stacked, so everything can be answered in one place without
// opening each task. The top card is the whole decision (where it came from,
// the task, Oby's headline and what happened, the choices, Later, and the way
// to its page); the next two peek out under it. Answered, a card lifts away
// (put off with Later, it sinks) and the next comes up; either way it leaves
// the stack (what's put off still needs you, so the line above still counts
// it). Nothing waiting: no stack, and Home is as it always was.
export default function AskStack({ onOpen }: { onOpen: (id: string) => void }) {
  const tasks = useTasks();
  const asks = tasksWith('needs').filter((t) => !t.answer);
  // cards on their way out, over the stack until their animation ends
  const [out, setOut] = useState<{ id: string; later: boolean }[]>([]);
  // the stack is as tall as its top card, easing to each one's height
  const top = useRef<HTMLElement>(null);
  const [height, setHeight] = useState<number>();
  useLayoutEffect(() => {
    const el = top.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.offsetHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [asks[0]?.id]);

  const leaving = out.flatMap((o) => tasks.filter((t) => t.id === o.id).map((t) => ({ ...o, task: t })));
  if (!asks.length && !leaving.length) return null;
  const peeks = asks.slice(1, 3);
  return (
    <section
      className={`asks${asks.length ? '' : ' is-empty'}`}
      aria-label="Oby is asking"
      style={{ height, '--peeks': peeks.length } as React.CSSProperties}
    >
      {peeks.map((t, i) => <div key={t.id} className="ask-peek" style={{ '--d': i + 1 } as React.CSSProperties} />)}
      {asks[0] && (
        <AskCard key={asks[0].id} ref={top} task={asks[0]} left={asks.length} onOpen={onOpen}
          onAnswer={(o) => setOut((all) => [...all, { id: asks[0].id, later: o === 'Later' }])} />
      )}
      {leaving.map((o) => (
        <AskCard key={`out:${o.id}`} task={o.task} className={o.later ? 'is-later' : 'is-out'}
          onGone={() => setOut((all) => all.filter((x) => x.id !== o.id))} />
      ))}
    </section>
  );
}

// One card: the task it's for, how many are left and the way to its page,
// then its decision as on the task's card.
function AskCard({ task, left, className = 'is-up', ref, onOpen, onAnswer, onGone }: {
  task: Task;
  left?: number;
  className?: string;
  ref?: React.Ref<HTMLElement>;
  onOpen?: (id: string) => void;
  onAnswer?: (option: string) => void;
  onGone?: () => void;
}) {
  return (
    <article ref={ref} className={`ask-card ${className}`} aria-label={task.title} onAnimationEnd={(e) => e.target === e.currentTarget && onGone?.()}>
      <header className="tc-head">
        <BrandMark brand={task.brand} size={16} mono />
        <h2 className="tc-title">
          {task.who && <span className="tc-who">{task.who}</span>}
          {task.title}
        </h2>
        {left != null && <span className="tc-ago">{left} left</span>}
        {onOpen && <button className="tc-link ask-open" onClick={() => onOpen(task.id)}>Open task<ChevronRight size={14} strokeWidth={1.8} /></button>}
      </header>
      <AskBlock task={task} onAnswer={onAnswer} />
    </article>
  );
}
