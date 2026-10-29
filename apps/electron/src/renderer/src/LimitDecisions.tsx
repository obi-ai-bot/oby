import { useState } from 'react';
import { answerLimit, realId, useLive } from './live';
import { onObid } from './lean';
import type { LimitDecision } from './obid';

// The same decision can be reached from Home and the conversation that started a job.
// Its options and revision are obid's; this view never decides which accounts have room.
export default function LimitDecisions({ conversation, job }: { conversation?: string; job?: string }) {
  const { limits, jobs } = useLive();
  if (!onObid) return null;
  const relevant = limits.filter((d) => !conversation && !job || d.jobs.some((j) => job === j.id || jobs[j.id]?.conversation === realId(conversation ?? '')));
  return <>{relevant.map((d) => <Decision key={d.id} decision={d} />)}</>;
}

function Decision({ decision }: { decision: LimitDecision }) {
  const { status } = useLive();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const source = status?.accounts.find((a) => a.id === decision.account);
  const reset = decision.resetAt != null && Number.isFinite(decision.resetAt)
    ? new Date(decision.resetAt * 1000).toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' }) : null;
  const count = decision.jobs.length;
  const choose = async (option: LimitDecision['options'][number]) => {
    if (busy) return;
    setBusy(true); setError('');
    try { await answerLimit(decision, option); }
    catch (e) { setError(e instanceof Error ? e.message : 'Couldn’t save your choice. Try again.'); }
    finally { setBusy(false); }
  };
  return (
    <article className="ask-card limit-decision" aria-label="Account limit decision">
      <div className="tc-ask">
        <span className="tc-ask-kind is-limit">Usage limit</span>
        <p className="tc-ask-head">{source?.name ?? decision.agent} reached its limit</p>
        <p className="tc-ask-text">
          {count} {count === 1 ? 'task is' : 'tasks are'} waiting{reset ? ` · resets ${reset}` : ' · reset time unknown'}.
          {source?.fiveHour != null ? ` Five-hour usage: ${Math.round(source.fiveHour * 100)}%.` : ''}
        </p>
        <ul className="limit-jobs">{decision.jobs.map((job) => <li key={job.id}>{job.title}</li>)}</ul>
        <div className="tc-options">
          {decision.options.map((option) => (
            <button key={option.kind === 'move' ? `move-${option.account}` : option.kind} className="tc-option"
              disabled={busy} onClick={() => void choose(option)}>
              {option.kind === 'move' ? `Move all to ${option.name} · ${option.agent}` : option.kind === 'resume' ? 'Resume when available' : 'Keep stopped'}
            </button>
          ))}
        </div>
        {busy && <p className="tc-ask-text" role="status">Saving your choice…</p>}
        {(error || decision.problem) && <p className="tc-ask-text" role="alert">{error || decision.problem}</p>}
      </div>
    </article>
  );
}
