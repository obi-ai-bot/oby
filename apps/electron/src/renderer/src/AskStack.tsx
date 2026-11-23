import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { BrandMark } from './brands';
import { GitHubAsk } from './GitHubCard';
import { AskBlock, startsOnClick } from './TaskCard';
import { OfferBlock } from './OfferCard';
import { QuestionBlock } from './QuestionCard';
import { ReviewBlock } from './ReviewRequestCard';
import { answer, asksWaiting, openOnGitHub, tasksWith, useTasks, type Asking, type Task } from './tasks';
import './askStack.css';

// What Oby is asking you, on Home, under the attention line: one card per task
// that needs you, stacked, so everything can be answered in one place without
// opening each task. The top card is the whole decision (where it came from,
// the task, where it is in the stack, Oby's headline and what happened, the
// choices, Later, and the way to its page); the next two peek out under it.
// The arrows (and ← →) go round the stack. Answered, a card lifts away (put
// off with Later, it sinks) and the next comes up; either way it leaves the
// stack (what's put off still needs you, so the line above still counts it).
// Nothing waiting: no stack, and Home is as it always was. On obid, what it asks
// with no task to show it on (a GitHub offer, a Confirm, an offer to start work)
// comes after the tasks'. Never over a conversation (chat first): what that conversation asks waits
// in the drawer over its message box (DecisionBar), and what others ask on their rows and in Waiting
// for you (docs/contracts/chat-first.md, CHATFIRST-8).
type Leave = 'out' | 'later' | 'next' | 'prev';
// a card: what obid asks (tasks.ts), and for a task's decision, its page one click away
type Entry = Asking & { task?: boolean; answer?: string };
const entryOf = (t: Task): Entry => ({ id: t.id, title: t.title, who: t.who, brand: t.brand, ask: t.job.ask!, answer: t.answer, task: true });
const LEAVE_MS = 320;  // the longest way out (app.css, ask-later and the like)
let left = 0;
let keptAt = { id: '', i: 0 };
export default function AskStack({ active, onOpen, onOpenTalk }: { active: boolean; onOpen: (id: string) => void; onOpenTalk: (id: string) => void }) {
  useTasks();
  const asks: Entry[] = [...tasksWith('needs').filter((t) => !t.answer).map(entryOf), ...asksWaiting()];
  const n = asks.length;
  // the card on top, and where it was: answered, the one after it takes its place
  // (kept here: chat first, the stack comes and goes with the page under it, and stays where you left it)
  const [at, setShown] = useState(keptAt);
  const setAt = (next: typeof keptAt) => { keptAt = next; setShown(next); };
  const found = asks.findIndex((t) => t.id === at.id);
  const i = found >= 0 ? found : n ? at.i % n : 0;
  const top = asks[i];
  const count = `${i + 1}/${n}`;
  // cards on their way out, over (or under) the stack until their animation is
  // over (a timer, so one paused with the window still goes); answered, with the answer
  const [out, setOut] = useState<{ key: number; entry: Entry; how: Leave; count: string }[]>([]);
  const leave = (how: Leave, answer?: string) => {
    const key = ++left;
    setOut((all) => [...all, { key, entry: answer ? { ...top, answer } : top, how, count }]);
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

  if (!n && !out.length) return null;
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
        <AskCard key={top.id} ref={topEl} entry={top} count={count} many={n > 1} onOpen={onOpen} onOpenTalk={onOpenTalk}
          onPrev={() => go(-1)} onNext={() => go(1)}
          onAnswer={(o) => { leave(o === 'Later' ? 'later' : 'out', o); setAt({ id: n > 1 ? asks[(i + 1) % n].id : '', i }); }} />
      )}
      {out.map((o) => (
        <AskCard key={o.key} entry={o.entry} count={o.count} many className={`is-${o.how}`} />
      ))}
    </section>
  );
}

// One card: the task it's for, where it is in the stack (and the way round it)
// and the way to its page (or GitHub's, or the offer's conversation), then its
// decision as on the task's card (an offer's, as in its conversation).
// A GitHub offer is its lean card instead (GitHubCard.tsx, GITHUBWORK-20 to 27): one line,
// the failed checks, its buttons, the pager on its line only when there are several.
function AskCard({ entry, count, many, className = 'is-up', ref, onOpen, onOpenTalk, onPrev, onNext, onAnswer }: {
  entry: Entry;
  count: string;
  many: boolean;  // more than one: the arrows to go round
  className?: string;
  ref?: React.Ref<HTMLElement>;
  onOpen?: (id: string) => void;
  onOpenTalk?: (id: string) => void;
  onPrev?: () => void;
  onNext?: () => void;
  onAnswer?: (option: string) => void;
}) {
  // A GitHub offer's card (docs/contracts/github-work.md, GITHUBWORK-10): a click on it, but on its
  // own buttons and links or one that selected its text, does what its start button does (Fix CI,
  // Review it, Fix it), as AskBlock's would: once, and not before its words have shown. Its work
  // already in a conversation, the click opens that instead.
  const start = entry.url && entry.ask?.kind === 'start' && !entry.answer && onAnswer ? entry.ask.options[0] : undefined;
  const [shownAt] = useState(() => performance.now());
  const onCard = (e: React.MouseEvent) => {
    if (!start || !startsOnClick(e, shownAt)) return;
    if (entry.talk) return onOpenTalk?.(entry.talk);
    answer(entry.id, start);
    onAnswer?.(start);
  };
  const nav = (
    <span className="ask-nav">
      {many && <button className="icon-button" aria-label="Previous" onClick={onPrev}><ChevronLeft size={14} strokeWidth={1.8} /></button>}
      <span className="ask-count" aria-live="polite">{count}</span>
      {many && <button className="icon-button" aria-label="Next" onClick={onNext}><ChevronRight size={14} strokeWidth={1.8} /></button>}
    </span>
  );
  // a GitHub offer: its lean card, the pager on its line only when there are several (GITHUBWORK-20 to 27)
  const github = !entry.task && entry.url && entry.ask?.kind === 'start' ? { ...entry, url: entry.url, ask: entry.ask } : undefined;
  return (
    <article ref={ref} className={`ask-card ${className}${github ? ' is-github' : ''}${start ? ' is-startable' : ''}`} aria-label={entry.title} onClick={onCard} data-task-ask={entry.task ? entry.id : undefined}>
      {github ? <GitHubAsk entry={github} nav={many ? nav : undefined} onAnswer={onAnswer} onOpenTalk={onOpenTalk} /> : <>
        <header className="tc-head">
          {entry.brand && <BrandMark brand={entry.brand} size={16} mono />}
          <h2 className="tc-title">
            {entry.who && <span className="tc-who">{entry.who}</span>}
            {entry.title}
          </h2>
          {nav}
          {entry.task ? <button className="tc-link ask-open" onClick={() => onOpen?.(entry.id)}>Open task<ChevronRight size={14} strokeWidth={1.8} /></button>
            : entry.url ? <button className="tc-link ask-open" onClick={() => openOnGitHub(entry.url)}>Open on GitHub<ChevronRight size={14} strokeWidth={1.8} /></button>
            : entry.talk && <button className="tc-link ask-open" onClick={() => onOpenTalk?.(entry.talk!)}>Open conversation<ChevronRight size={14} strokeWidth={1.8} /></button>}
        </header>
        {entry.offer ? <OfferBlock offer={entry.offer} onAnswer={onAnswer} />
          : entry.question ? <QuestionBlock question={entry.question} onAnswer={onAnswer} />
          : entry.review ? <ReviewBlock review={entry.review} onAnswer={onAnswer} />
          : <AskBlock task={{ id: entry.id, answer: entry.answer, job: { ask: entry.ask } }} onAnswer={onAnswer} />}
      </>}
    </article>
  );
}
