import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ArrowDown, Check, ChevronRight, ShieldCheck } from 'lucide-react';
import Composer, { typingIn } from './Composer';
import Formatted from './Formatted';
import Said from './Said';
import { BrandMark, type Brand } from './brands';
import { useScrollThumb } from './useScrollThumb';
import { AGENTS, type Attachments } from './mock';
import { SESSIONS, type Detail, type Entry } from './session';
import { useTasks, type Task } from './tasks';
import { box as draftBox } from './drafts';
import { onObid } from './lean';
import { entryDetail, useFollowedSession } from './live';
import { sessionOf } from './jobPage';
import { took } from './liveTask';
import './liveTask.css';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;
// a word of what the agent is writing comes in this often
const WORD_MS = 70;

// The filters over the session: everything, or one kind, each with its count.
const FILTERS: { id: string; label: string }[] = [
  { id: 'all', label: 'All' }, { id: 'said', label: 'Said' }, { id: 'Bash', label: 'Commands' },
  { id: 'Edit', label: 'Edits' }, { id: 'Read', label: 'Reads' }, { id: 'Tetrix', label: 'Tetrix' }, { id: 'Web', label: 'Web' },
];
const filterOf = (e: Entry) =>
  e.kind === 'tool' ? (e.tool === 'Grep' ? 'Read' : e.tool) : e.kind === 'said' || e.kind === 'thought' || e.kind === 'you' ? 'said' : null;

// A job's session as lines: obid's, kept up to date while follow, or the mock's
export function useEntries(id: string, follow = true) {
  const had = onObid ? useFollowedSession(id, follow) : undefined;
  return useMemo(() => (onObid ? sessionOf(had ?? []) : SESSIONS[id] ?? []), [id, had]);
}

// What the agent is writing now comes in a word at a time; the tool at work counts up
export function useLiveLines(entries: Entry[], running: boolean) {
  const liveSaid = entries.find((e): e is Extract<Entry, { kind: 'said' }> => e.kind === 'said' && !!e.live);
  const words = (liveSaid?.text ?? '').match(/\S+\s*/g) ?? [];
  const [shown, setShown] = useState(0);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!running) return;
    const w = setInterval(() => setShown((n) => Math.min(n + 1, words.length)), WORD_MS);
    const t = setInterval(() => setTick((n) => n + 1), 1000);
    return () => { clearInterval(w); clearInterval(t); };
  }, [running, words.length]);
  return { shown: words.slice(0, shown).join(''), tick };
}

// A session's scroller: it opens at the live end, as a conversation does, and stays there while you're there
export function useLiveEnd() {
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLElement>();
  const [atEnd, setAtEnd] = useState(true);
  const pinned = useRef(true);
  useLayoutEffect(() => {
    const el = list.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  });
  // and again once it has risen in and laid out, and whenever its size changes
  useEffect(() => {
    const el = list.current;
    if (!el) return;
    const pin = () => { if (pinned.current) el.scrollTop = el.scrollHeight; };
    const raf = requestAnimationFrame(pin);
    const ro = new ResizeObserver(pin);
    ro.observe(el);
    if (el.firstElementChild) ro.observe(el.firstElementChild);
    return () => { cancelAnimationFrame(raf); ro.disconnect(); };
  }, [list]);
  const onListScroll = (e: React.UIEvent<HTMLElement>) => {
    onScroll();
    const el = e.currentTarget, end = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
    pinned.current = end;
    setAtEnd(end);
  };
  return { list, onListScroll, atEnd, thumbProps };
}

// A job's session, live: the agent's words as it writes them, its thinking,
// each tool call as a quiet line that opens into what it ran and what came
// back, what you told it, its to-do list and the permissions it asked for.
// Opened from the task's page (Full session); it lies over it, as the task's
// conversation does, and back (or Esc) returns to the page. Values from the
// vault show as [vault: NAME], never the value.
export default function SessionView({ id, leaving, covered = false, onBack, onGone, onTell }: {
  id: string;
  leaving: boolean;
  covered?: boolean;
  onBack: () => void;
  onGone: () => void;
  onTell: (id: string, text: string, att: Attachments) => void;
}) {
  const tasks = useTasks();
  const kept = useRef<Task>(null);
  const task = tasks.find((t) => t.id === id) ?? kept.current!;
  kept.current = task;
  // on obid, its transcript, as it comes
  const entries = useEntries(id);
  const running = task.status === 'running' || task.status === 'needs';
  const [filter, setFilter] = useState('all');
  const { list, onListScroll, atEnd, thumbProps } = useLiveEnd();
  const live = useLiveLines(entries, running);

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

  const count = (f: string) => (f === 'all' ? entries.filter((e) => e.kind !== 'start').length : entries.filter((e) => filterOf(e) === f).length);
  // keyed by their place in the whole session, so an opened line stays open across filters
  const visible = entries.map((e, n) => ({ e, n })).filter(({ e }) => filter === 'all' || e.kind === 'start' || filterOf(e) === filter);
  const agent = AGENTS[task.job.agent];

  return (
    <div className={`chat-layer${leaving ? ' is-leaving' : ''}${covered ? ' is-covered' : ''}`} inert={covered}>
      <div className="chat-scroll">
        <section
          ref={list}
          className="chat task-scroll"
          aria-label={`${task.title}: session`}
          onScroll={onListScroll}
        >
          <div className="chat-column ss">
            <header className="ss-head">
              <h1>Session</h1>
              <p>
                {agent.name} · {task.job.model} · {task.job.account}
                {running && <span className="ss-live"><span className="status-dot is-working" />Live</span>}
              </p>
              <div className="ss-filters" role="tablist" aria-label="Show">
                {FILTERS.filter((f) => f.id === 'all' || count(f.id) > 0).map((f) => (
                  <button key={f.id} role="tab" aria-selected={filter === f.id} className={filter === f.id ? 'is-on' : undefined} onClick={() => setFilter(f.id)}>
                    {f.label} <span>{count(f.id)}</span>
                  </button>
                ))}
              </div>
            </header>
            <ol className="ss-entries">
              {visible.map(({ e, n }) => <Line key={n} entry={e} job={id} brand={agent.brand} live={live} />)}
            </ol>
          </div>
        </section>
        <div {...thumbProps} />
        {/* scrolled up while it works: back to what's happening now */}
        {running && !atEnd && (
          <button className="ss-now" onClick={() => { const el = list.current; if (el) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' }); }}>
            <ArrowDown size={14} strokeWidth={1.8} />Jump to now
          </button>
        )}
      </div>
      <Composer
        className="task-composer"
        autoFocus={false}
        placeholder={`Tell ${agent.name}…`}
        label={`Tell Oby about ${task.title}`}
        onSend={(text, att) => onTell(task.id, text, att)}
        draft={draftBox.task(task.id)}
      />
    </div>
  );
}

// One entry of the session. inPane: in the agent's own conversation beside the chat (AgentPane.tsx),
// where a message has its clock time and thinking shows without a click
export function Line({ entry: e, job, brand, live, inPane = false }: { entry: Entry; job: string; brand: Brand; live: { shown: string; tick: number }; inPane?: boolean }) {
  switch (e.kind) {
    case 'start':
      // a round begins: a quiet divider with the agent, model and time
      return <li className="ss-start"><span>{e.text}</span><time>{e.at}</time></li>;
    case 'said':
      return (
        <li className="ss-said">
          <span className="ss-mark"><BrandMark brand={brand} size={16} mono /></span>
          <div className="ss-words">
            <Said text={e.live ? live.shown : e.text} streaming={e.live} />
            {e.live && <span className="ss-caret" aria-hidden="true" />}
            {inPane && <time className="ss-clock">{e.at}</time>}
          </div>
        </li>
      );
    case 'thought':
      return <Thought thought={e} inPane={inPane} />;
    case 'you':
      return <li className="ss-you"><p>{e.text}</p><time>{e.at}</time></li>;
    case 'plan':
      return (
        <li className="ss-plan">
          <span className="ss-plan-head">To do · {e.todos.filter((t) => t.state === 'done').length} of {e.todos.length} done</span>
          <ul>
            {e.todos.map((t) => (
              <li key={t.text} className={`is-${t.state}`}>
                {t.state === 'done' ? <Check size={12} strokeWidth={2} /> : <span className={`status-dot ${t.state === 'running' ? 'is-working' : 'is-queued'}`} />}
                {t.text}
              </li>
            ))}
          </ul>
        </li>
      );
    case 'asked':
      return <li className="ss-asked"><ShieldCheck size={13} strokeWidth={1.7} /><span><Formatted text={e.text} /> · {e.answer}</span></li>;
    case 'tool':
      return <Tool call={e} job={job} tick={live.tick} />;
  }
}

// A tool call as a quiet line; a click opens what it ran and what came back, from obid then (and
// again when it ends, if it was opened running), or the mock's own
function Tool({ call: e, job, tick }: { call: Extract<Entry, { kind: 'tool' }>; job: string; tick: number }) {
  const [open, setOpen] = useState(false);
  const [fetched, setFetched] = useState<Detail>();
  useEffect(() => {
    if (!open || e.detail || e.n == null) return;
    let here = true;
    void entryDetail(job, e.n).then((d) => { if (here && d) setFetched(d); });
    return () => { here = false; };
  }, [open, e.state, e.n, e.detail, job]);
  const detail = e.detail ?? fetched, opens = !!e.detail || e.n != null;
  const secs = e.state === 'running' ? e.seconds + tick : e.seconds;
  return (
    <li className={`ss-tool is-${e.state}${open ? ' is-open' : ''}`}>
      <button className="ss-tool-row" onClick={() => opens && setOpen(!open)} aria-expanded={opens ? open : undefined} disabled={!opens}>
        <span className="ss-tool-mark">
          {e.state === 'done' ? <Check size={12} strokeWidth={2} /> : <span className={`status-dot ${e.state === 'running' ? 'is-working' : e.state === 'declined' ? 'is-queued' : 'is-failed'}`} />}
        </span>
        <span className="ss-tool-text"><span className="ss-verb">{e.verb}</span> <code>{e.target}</code></span>
        {e.meta && <span className="ss-tool-meta">{e.meta}</span>}
        <span className="ss-took">{took(secs)}</span>
        {opens && <ChevronRight size={12} strokeWidth={1.8} className="ss-chev" />}
      </button>
      {open && detail && <ToolDetail tool={e.tool} detail={detail} seconds={e.seconds} />}
    </li>
  );
}

// A thought, only what its tool gave. Thinking while it goes on. Claude Code's: how long, with no
// words, so nothing opens. Codex's and Cursor's: their words, when they give any, which open
function Thought({ thought: e, inPane }: { thought: Extract<Entry, { kind: 'thought' }>; inPane: boolean }) {
  const [open, setOpen] = useState(inPane || !e.seconds);
  if (e.running) return <li className="ss-thought"><span className="sheen">Thinking</span></li>;
  const label = e.seconds ? `Thought for ${took(e.seconds)}` : 'Thought';
  if (!e.text) return <li className="ss-thought"><span>{label}</span></li>;
  return (
    <li className={`ss-thought${open ? ' is-open' : ''}`}>
      <button onClick={() => setOpen(!open)} aria-expanded={open}>{label}<ChevronRight size={12} strokeWidth={1.8} /></button>
      {open && <p>{e.text}</p>}
    </li>
  );
}

// What a call ran and what came back: a command and its output, an edit's
// diff, a read's lines, a search's matches, a question and its answer.
function ToolDetail({ tool, detail: d, seconds }: { tool: string; detail: Detail; seconds: number }) {
  if (d.diff) {
    return (
      <pre className="ss-code ss-diff">
        {d.diff.split('\n').map((l, i) => (
          <span key={i} className={l.startsWith('@@') ? 'is-hunk' : l[0] === '+' ? 'is-add' : l[0] === '-' ? 'is-del' : undefined}>{l}{'\n'}</span>
        ))}
      </pre>
    );
  }
  const shownLines = d.output ? d.output.split('\n').length : 0;
  return (
    <div className="ss-detail">
      {d.input && (tool === 'Bash'
        ? <pre className="ss-code"><span className="ss-prompt">$ </span>{d.input}</pre>
        : <p className="ss-input">{d.input}</p>)}
      {d.range && <p className="ss-input">{d.range}</p>}
      {d.output && <pre className="ss-code ss-out"><Masked text={d.output} /></pre>}
      {tool === 'Bash' && (
        <p className="ss-foot">
          {d.exit != null && <span className={d.exit ? 'is-failed' : undefined}>exit {d.exit}</span>}
          <span>{took(seconds)}</span>
          {d.lines && d.lines > shownLines && <span>the last {shownLines} of {d.lines.toLocaleString()} lines</span>}
        </p>
      )}
    </div>
  );
}

// a value from the vault, as obid sends it: a chip with its name, never the value
function Masked({ text }: { text: string }) {
  return <>{text.split(/(\[vault: [^\]]+\])/).map((part, i) => (part.startsWith('[vault: ') ? <span key={i} className="ss-vault" title="A value from Oby's vault, masked before it's shown here">{part.slice(1, -1)}</span> : part))}</>;
}
