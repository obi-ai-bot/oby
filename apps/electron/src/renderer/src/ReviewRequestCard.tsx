import { useState, useSyncExternalStore } from 'react';
import { answerQuestion, liveState, putAwayClosed } from './live';
import { answer, type PersonReview } from './tasks';

// Another Oby's review request (docs/contracts/teammate-review.md, TEAMREVIEW-2; peer-thread.md,
// PEER-46 to 49), on Home's stack and over that member's thread box, shaped like the question card:
// obid's words, then what that Oby wrote (its goal, summary and conditions' notes), quoted, as text
// nodes, then the item's own answers and Later. Each answer is its button's label (POST
// /v1/inbox/{id}/answer, docs/contracts/signed-words.md); one at a time, and a refusal stays on the
// card in obid's words. Per item and outliving the card, as QuestionBlock's.
// Closed while it showed, not by an answer (it expired, or was called off): the card stays, saying
// why, with nothing to click but Close (PEER-49).
type Sent = { busy?: boolean; problem?: string };
const NOTHING: Sent = {};
let sent: ReadonlyMap<string, Sent> = new Map();
const sentListeners = new Set<() => void>();
const mark = (id: string, change: Sent | null) => {
  const next = new Map(sent);
  if (change) next.set(id, { ...sent.get(id), ...change }); else next.delete(id);
  sent = next;
  sentListeners.forEach((l) => l());
};
const subscribeSent = (l: () => void) => { sentListeners.add(l); return () => { sentListeners.delete(l); }; };
// still open in obid, as last heard: a click on a card that closed meanwhile sends nothing
const open = (id: string) => liveState().inbox.some((i) => i.id === id && i.state !== 'done');

export function ReviewBlock({ review: r, onAnswer }: { review: PersonReview; onAnswer?: (option: string) => void }) {
  const s = useSyncExternalStore(subscribeSent, () => sent.get(r.id) ?? NOTHING);
  // a click before its words have shown answers nothing (one click, one answer), as AskBlock's
  const [shownAt] = useState(() => performance.now());
  const send = async (option: string) => {
    if (sent.get(r.id)?.busy || performance.now() - shownAt < 500 || !open(r.id)) return;
    mark(r.id, { busy: true, problem: undefined });
    try {
      await answerQuestion(r.id, option);
      mark(r.id, null);
      onAnswer?.(option);
    } catch (e) {
      mark(r.id, { busy: false, problem: e instanceof Error ? e.message : String(e) });
    }
  };
  return (
    <div className="tc-ask">
      <span className="tc-ask-kind is-review">{r.kind === 'task' ? 'Task' : 'Review request'}</span>
      <p className="tc-ask-head">{r.text}</p>
      {/* all of it, never clamped as Home's other text is: one quote that keeps its lines and scrolls */}
      {(r.goal || r.summary || r.conditions.length > 0) && (
        <blockquote className="msg-quote room-question">
          {r.goal && <p>{r.goal}</p>}
          {r.summary && <p>{r.summary}</p>}
          {r.conditions.map((c, i) => <p key={i}>Their condition: {[c.text, c.note].filter(Boolean).join(' · ')}</p>)}
        </blockquote>
      )}
      {r.closed ? <>
        <p className="tc-report" role="status">{r.closed}</p>
        <div className="tc-options">
          <button className="tc-option is-quiet" onClick={() => { putAwayClosed(r.id); mark(r.id, null); onAnswer?.('Close'); }}>Close</button>
        </div>
      </> : <>
        {s.problem && <p className="tc-report is-problem" role="alert">{s.problem}</p>}
        <div className="tc-options">
          {/* none looks preferred: obid recommends none for another Oby's request */}
          {r.options.map((o) => <button key={o} className="tc-option" disabled={s.busy} onClick={() => void send(o)}>{o}</button>)}
          <button className="tc-option is-quiet" onClick={() => { answer(r.id, 'Later'); onAnswer?.('Later'); }}>Later</button>
        </div>
      </>}
    </div>
  );
}
