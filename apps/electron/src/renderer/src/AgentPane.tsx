import { useEffect, useMemo } from 'react';
import { ArrowUpRight, Check, X } from 'lucide-react';
import Composer from './Composer';
import { Earlier, Line, liveFor, useEntries, useLiveEnd, useLiveLines, useTail } from './SessionView';
import { AskBlock } from './TaskCard';
import { box as draftBox } from './drafts';
import { onObid } from './lean';
import { tellAgent } from './live';
import { around, lastSteps, thoughtWords, todosOf, type Step, type Todo } from './liveTask';
import { AGENTS } from './mock';
import { markSeen, stop, tell, useTasks, type Task } from './tasks';

// How the to-do it's on stands: at work, waiting on you, stopped by a failure, or just ended
const stateOf = (task: Task) => (task.status === 'needs' ? 'needs' : task.status === 'running' ? 'working' : task.job.status === 'done' ? 'ended' : 'failed');

// Its to-dos as the agent wrote them, the one it's on lit (Sangam's sp-todos), and under it what's
// passed in: on a card, the last things it did
function Todos({ todos, task, children }: { todos: Todo[]; task: Task; children?: React.ReactNode }) {
  const state = stateOf(task);
  return (
    <ol className="lt-todos" aria-label="What it’s doing">
      {todos.map((t, i) => {
        const on = t.state === 'running';
        const dot = !on ? 'is-queued' : state === 'working' ? 'is-working' : state === 'needs' ? 'is-needs' : state === 'failed' ? 'is-failed' : 'is-queued';
        return (
          <li key={i} className={t.state === 'done' ? 'is-done' : on ? `is-on is-${state}` : 'is-up'}>
            <div className="lt-todo">
              <span className="status">{t.state === 'done' ? <Check size={12} strokeWidth={2} /> : <span className={`status-dot ${dot}`} />}</span>
              <span className={on && state === 'working' ? 'sheen' : undefined}>{t.text}</span>
            </div>
            {on && children}
          </li>
        );
      })}
    </ol>
  );
}

// One of the last things it did, on a card: its thinking and its words as a line each, a tool call
// with its mark, its verb and what it ran or touched
function StepLine({ step: e }: { step: Step }) {
  if (e.kind === 'thought') return <p className="lt-thought">{e.running ? <span className="sheen">{thoughtWords(e)}</span> : thoughtWords(e)}</p>;
  if (e.kind === 'said') return <p className="lt-said">{e.text.replace(/\s+/g, ' ').slice(0, 200)}</p>;
  return (
    <div className={`lt-tool is-${e.state}`}>
      <span className="status">{e.state === 'done' ? <Check size={12} strokeWidth={2} /> : <span className={`status-dot ${e.state === 'running' ? 'is-working' : e.state === 'declined' ? 'is-queued' : 'is-failed'}`} />}</span>
      <span className="lt-tool-text"><span className="lt-verb">{e.verb}</span>{e.target && <> <span className={e.tool === 'Bash' || e.tool === 'Read' || e.tool === 'Edit' ? 'tok-path' : undefined}>{e.target}</span></>}</span>
    </div>
  );
}

// A task's work under its card while it goes on (Sangam's board "Oby at work"): its to-dos if the
// agent made a list, the last things it did either way. Nothing from its agent yet: what obid says
// it's on. Past its agent's work (the check, publishing): the stage it's in.
export function WorkNow({ task, onClick }: { task: Task; onClick?: () => void }) {
  const entries = useEntries(task.id);
  const j = task.job, list = todosOf(entries), steps = lastSteps(entries);
  const stage = j.stages.find((st) => st.state === 'current' && st.id !== 'work');
  const lines = steps.length > 0 && <div className="lt-steps">{steps.map((e, i) => <StepLine key={i} step={e} />)}</div>;
  const shown = list && around(list);
  // a list with none in hand now: the last things it did go under the whole list
  const under = !list?.some((t) => t.state === 'running');
  const now = stage && task.status === 'running' ? <p className="lt-thought"><span className="sheen">{stage.title}</span></p>
    : !steps.length && !list && task.status === 'running' && (j.waiting?.text || j.step) ? <p className="lt-thought"><span className="sheen">{j.waiting?.text || j.step}</span></p>
    : null;
  if (!shown && !lines && !now && task.status !== 'needs') return null;
  return (
    <div className="lt-now" onClick={onClick}>
      {task.status === 'needs' && j.ask && <p className="lt-needs">{j.ask.headline}</p>}
      {shown && (
        <>
          {shown.before > 0 && <p className="lt-more">{shown.before} done before these</p>}
          <Todos todos={shown.todos} task={task}>{!under && lines}</Todos>
          {shown.after > 0 && <p className="lt-more">{shown.after} more to do</p>}
        </>
      )}
      {(!shown || under) && lines}
      {now}
    </div>
  );
}

// The agent's own conversation (Sangam's board "The agent's own conversation"): opened beside the chat
// from its card, over it in a narrow window. From the start, what it said, did and thought and what
// you told it, its to-dos on top, and the box to tell it something, as a task's own page does
// (POST /v1/jobs/{id}/messages). Told something once it's finished, it starts a follow-up, which this
// then follows.
export default function AgentPane({ id, onClose, onDetails, onFollowUp }: {
  id: string;
  onClose: () => void;
  onDetails: (id: string) => void;
  onFollowUp: (id: string) => void;
}) {
  const tasks = useTasks();
  const task = tasks.find((t) => t.id === id)!;
  const entries = useEntries(id);
  // opened, it's seen, as its page is
  useEffect(() => { markSeen(id); }, [id]);
  const atWork = task.status === 'running' || task.status === 'needs';
  const live = useLiveLines(entries, atWork);
  const { list, onListScroll, thumbProps } = useLiveEnd();
  const j = task.job, agent = AGENTS[j.agent], todos = todosOf(entries);
  // its to-dos are on top, not in the conversation as well
  const tail = useTail(useMemo(() => entries.filter((e) => e.kind !== 'plan'), [entries]));
  const send = (text: string, att: Parameters<typeof tell>[2]) => {
    if (!onObid) return void tell(id, text, att);
    void tellAgent(id, text, att?.files).then((next) => { if (next) onFollowUp(next); });
  };
  return (
    <aside className="agent-pane" aria-label={`Conversation with ${agent.name}`}>
      <header className="agent-pane-bar">
        <button className="icon-button" aria-label="Close this conversation" onClick={onClose}><X size={16} strokeWidth={1.8} /></button>
        <div className="agent-pane-title">
          <span>{task.title}</span>
          <span>{agent.name} · {j.account}{j.repo.includes('/') && <> · {j.repo.split('/')[1]}</>}</span>
        </div>
        {task.status === 'running' && <button className="set-button is-danger" onClick={() => stop(id)}>Stop</button>}
        {/* proposed: the task's page (what went out, Publish), which the card used to open */}
        <button className="icon-button" aria-label="Open the task’s page" title="Open the task’s page" onClick={() => onDetails(id)}><ArrowUpRight size={16} strokeWidth={1.8} /></button>
      </header>
      {todos && <div className="agent-pane-todos"><Todos todos={todos} task={task} /></div>}
      <div className="agent-pane-scroll">
        <section ref={list} className="agent-pane-thread" onScroll={onListScroll}>
          <ol className="ss-entries">
            <Earlier count={tail.earlier} onClick={tail.more} />
            {tail.shown.map((e, i) => <Line key={e.n ?? i} entry={e} job={id} brand={agent.brand} live={liveFor(e, live)} inPane />)}
          </ol>
        </section>
        <div {...thumbProps} />
      </div>
      {task.status === 'needs' && j.ask && <div className="msg-ask agent-pane-ask"><AskBlock task={task} inChat /></div>}
      <Composer
        className="task-composer"
        autoFocus={false}
        placeholder={task.status === 'done' ? 'Ask for a follow-up…' : `Tell ${agent.name}…`}
        label={`Tell the agent about ${task.title}`}
        onSend={send}
        draft={draftBox.task(id)}
      />
    </aside>
  );
}
