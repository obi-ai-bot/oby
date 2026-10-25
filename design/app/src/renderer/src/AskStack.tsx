import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { BrandMark } from './brands';
import { AskBlock } from './TaskCard';
import { tasksWith, useTasks, type Task } from './tasks';

// What Oby is asking you, on Home, under the attention line: one card per task
// that needs you, stacked, so everything can be answered in one place without
// opening each task. The top card is the whole decision (where it came from,
// the task, where it is in the stack, Oby's headline and what happened, the
// choices, Later, and the way to its page); the next two peek out under it.
// The arrows (and ← →) go round the stack. Answered, a card lifts away (put
// off with Later, it sinks) and the next comes up; either way it leaves the
// stack (what's put off still needs you, so the line above still counts it).
// Nothing waiting: no stack, and Home is as it always was.
type Leave = 'out' | 'later' | 'next' | 'prev';
const LEAVE_MS = 320;  // the longest way out (app.css, ask-later and the like)
let left = 0;
export default function AskStack({ active, onOpen }: { active: boolean; onOpen: (id: string) => void }) {
  const tasks = useTasks();
  const asks = tasksWith('needs').filter((t) => !t.answer);
  const n = asks.length;
  // the card on top, and where it was: answered, the one after it takes its place
  const [at, setAt] = useState({ id: '', i: 0 });
  const found = asks.findIndex((t) => t.id === at.id);
  const i = found >= 0 ? found : n ? at.i % n : 0;
  const top = asks[i];
  const count = `${i + 1}/${n}`;
  // cards on their way out, over (or under) the stack until their animation is
  // over (a timer, so one paused with the window still goes)
  const [out, setOut] = useState<{ key: number; id: string; how: Leave; count: string }[]>([]);
  const leave = (how: Leave) => {
    const key = ++left;
    setOut((all) => [...all, { key, id: top.id, how, count }]);
    setTimeout(() => setOut((all) => all.filter((x) => x.key !== key)), LEAVE_MS);
  };
  const go = (step: number) => {
    if (n < 2) return;
    const j = (i + step + n) % n;
    leave(step > 0 ? 'next' : 'prev');
    setAt({ id: asks[j].id, i: j });
  };
  // ← and → go round it while Home shows it, unless you're typing
  const goRef = useRef(go);
  goRef.current = go;
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
      if ((e.target as Element | null)?.closest?.('input, textarea, [contenteditable="true"]')) return;
      e.preventDefault();
      goRef.current(e.key === 'ArrowRight' ? 1 : -1);
    };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [active]);

  // the stack is as tall as its top card, easing to each one's height
  const topEl = useRef<HTMLElement>(null);
  const [height, setHeight] = useState<number>();
  useLayoutEffect(() => {
    const el = topEl.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.offsetHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [top?.id]);

  const leaving = out.flatMap((o) => tasks.filter((t) => t.id === o.id).map((t) => ({ ...o, task: t })));
  if (!n && !leaving.length) return null;
  // the ones after the top, going round
  const peeks = Array.from({ length: Math.min(2, n - 1) }, (_, d) => asks[(i + d + 1) % n]);
  return (
    <section
      className={`asks${n ? '' : ' is-empty'}`}
      aria-label="Oby is asking"
      style={{ height, '--peeks': peeks.length } as React.CSSProperties}
    >
      {peeks.map((t, d) => <div key={t.id} className="ask-peek" style={{ '--d': d + 1 } as React.CSSProperties} />)}
      {top && (
        <AskCard key={top.id} ref={topEl} task={top} count={count} many={n > 1} onOpen={onOpen}
          onPrev={() => go(-1)} onNext={() => go(1)}
          onAnswer={(o) => { leave(o === 'Later' ? 'later' : 'out'); setAt({ id: n > 1 ? asks[(i + 1) % n].id : '', i }); }} />
      )}
      {leaving.map((o) => (
        <AskCard key={o.key} task={o.task} count={o.count} many className={`is-${o.how}`} />
      ))}
    </section>
  );
}

// One card: the task it's for, where it is in the stack (and the way round it)
// and the way to its page, then its decision as on the task's card.
function AskCard({ task, count, many, className = 'is-up', ref, onOpen, onPrev, onNext, onAnswer }: {
  task: Task;
  count: string;
  many: boolean;  // more than one: the arrows to go round
  className?: string;
  ref?: React.Ref<HTMLElement>;
  onOpen?: (id: string) => void;
  onPrev?: () => void;
  onNext?: () => void;
  onAnswer?: (option: string) => void;
}) {
  return (
    <article ref={ref} className={`ask-card ${className}`} aria-label={task.title}>
      <header className="tc-head">
        <BrandMark brand={task.brand} size={16} mono />
        <h2 className="tc-title">
          {task.who && <span className="tc-who">{task.who}</span>}
          {task.title}
        </h2>
        <span className="ask-nav">
          {many && <button className="icon-button" aria-label="Previous" onClick={onPrev}><ChevronLeft size={14} strokeWidth={1.8} /></button>}
          <span className="ask-count" aria-live="polite">{count}</span>
          {many && <button className="icon-button" aria-label="Next" onClick={onNext}><ChevronRight size={14} strokeWidth={1.8} /></button>}
        </span>
        <button className="tc-link ask-open" onClick={() => onOpen?.(task.id)}>Open task<ChevronRight size={14} strokeWidth={1.8} /></button>
      </header>
      <AskBlock task={task} onAnswer={onAnswer} />
    </article>
  );
}
