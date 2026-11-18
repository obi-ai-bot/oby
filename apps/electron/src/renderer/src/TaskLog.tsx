import { useLayoutEffect, useRef, useState } from 'react';
import { Check } from 'lucide-react';
import { waitSays } from './chatgpt';
import ContextMeter from './ContextMeter';
import Said from './Said';
import { BrandMark } from './brands';
import { Line, liveFor, useEntries, useLiveLines } from './SessionView';
import { ASK_LABEL, agentOf } from './view';
import type { Entry } from './session';
import type { Task } from './tasks';

// how many of its last lines show while it works, the rest a click away (T3 Code's work log)
export const SHOWN = 4;

// What a task did, in the conversation under its card (docs/contracts/live-task.md, LIVETASK-28 to 31): its
// newest to-do list and its last lines while it works, each a short line as it happens; ended, its last
// words (what changed, the test run, the commit, the result) with the lines before them folded. Long
// words fold behind Show more, as T3 Code's do. The card above it stays progress only.
export default function TaskLog({ task }: { task: Task }) {
  const entries = useEntries(task.id);
  const [open, setOpen] = useState(false);
  const atWork = task.status === 'running' || task.status === 'needs';
  const live = useLiveLines(entries, atWork);
  const j = task.job, agent = agentOf(j.agent);
  const plan = atWork ? entries.filter((e) => e.kind === 'plan').at(-1) : undefined;
  // what it was told to start with is the conversation's already (Oby's answer that started it): the
  // words told it since stay
  const all = entries.filter((e) => e.kind !== 'plan' && e.kind !== 'start');
  const told = all.findIndex((e) => e.kind !== 'you');
  const lines = told < 0 ? [] : all.slice(told);
  // ended: its last words are its result, shown whole; the rest folds
  const last = atWork ? -1 : lines.map((e) => e.kind).lastIndexOf('said');
  const result = last >= 0 ? (lines[last] as Extract<Entry, { kind: 'said' }>) : undefined;
  const steps = result ? lines.filter((_, i) => i !== last) : lines;
  const extra = Math.max(0, steps.length - (atWork ? SHOWN : 0));
  const shown = open || !extra ? steps : steps.slice(extra);
  // obid's own line while it works: the stage past its agent's work, a wait of obid's, or a step its
  // session doesn't hold ("Trying again in 5 minutes (2 of 3)", RECOVERY-5)
  const stage = j.stages.find((st) => st.state === 'current' && st.id !== 'work');
  const own = j.step && j.step !== 'Working' && j.step !== 'Thinking' && !entries.some((e) => e.kind === 'tool' && e.summary === j.step);
  const says = task.status !== 'running' ? null : stage ? stage.title : !lines.length && !plan ? waitSays(j) : own ? j.step : null;
  const answered = task.status !== 'needs' && j.ask && task.answer && task.answer !== 'Later';
  // how full its agent's context is while it works, as its tool said (docs/contracts/usage.md, USAGE-17)
  const context = atWork ? j.page?.context : undefined;
  if (!plan && !lines.length && !says && !answered && !context) return null;
  return (
    <div className="lt-log" role="group" aria-label={`What ${agent.name} did`}>
      {plan && <ol className="ss-entries"><Line entry={plan} job={task.id} brand={agent.brand} live={liveFor(plan, live)} /></ol>}
      {extra > 0 && <button type="button" className="lt-log-more" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Show less' : `Show ${extra} more`}</button>}
      {shown.length > 0 && (
        <ol className="ss-entries">
          {shown.map((e, i) => (e.kind === 'said'
            ? <Words key={e.n ?? `s${i}`} text={e.live ? live.shown : e.text} streaming={e.live} brand={agent.brand} />
            : <Line key={e.n ?? i} entry={e} job={task.id} brand={agent.brand} live={liveFor(e, live)} />))}
        </ol>
      )}
      {says && <p className="lt-log-now"><span className="sheen">{says}</span></p>}
      {result && <ol className="ss-entries"><Words text={result.text} brand={agent.brand} /></ol>}
      {/* a decision answered: one line where it was asked, until obid asks something else (LIVETASK-27) */}
      {answered && <p className="lt-answered"><Check size={12} strokeWidth={2} />{j.ask!.label ?? ASK_LABEL[j.ask!.kind] ?? j.ask!.kind} · {task.answer}</p>}
      {context && <ContextMeter used={context.used} size={context.size} />}
    </div>
  );
}

// The agent's words, folded past a few lines behind Show more
function Words({ text, streaming, brand }: { text: string; streaming?: boolean; brand: Parameters<typeof BrandMark>[0]['brand'] }) {
  return (
    <li className="ss-said">
      <span className="ss-mark"><BrandMark brand={brand} size={16} mono /></span>
      <Fold><Said text={text} streaming={streaming} /></Fold>
    </li>
  );
}

// Something long, cut to a few lines with Show more; only when it's longer than that
export function Fold({ children }: { children: React.ReactNode }) {
  const body = useRef<HTMLDivElement>(null);
  const [long, setLong] = useState(false);
  const [open, setOpen] = useState(false);
  useLayoutEffect(() => {
    const el = body.current;
    if (el && !open) setLong(el.scrollHeight > el.clientHeight + 2);
  });
  return (
    <div className={`lt-fold${long ? ' is-long' : ''}${open ? ' is-open' : ''}`}>
      <div ref={body} className="lt-fold-body">{children}</div>
      {(long || open) && <button type="button" className="lt-log-more" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Show less' : 'Show more'}</button>}
    </div>
  );
}
