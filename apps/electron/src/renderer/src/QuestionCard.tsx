import { useState, useSyncExternalStore } from 'react';
import { CheckField } from './TaskCard';
import { answerQuestion } from './live';
import { answer, type PersonQuestion } from './tasks';

// A question only the owner can answer (docs/contracts/peer-thread.md, PEER-30 to PEER-36), on Home's
// stack and in that member's thread: the asker's words, quoted, as text nodes, then Answer, Decline
// and Later. obid takes the answer only with this Mac's signature, which main adds only on a click or
// keystroke it saw itself: the words the box held, or this button's label (docs/contracts/signed-words.md).
// Per question and outliving the card, so the words a refusal left show wherever it's opened next:
// writing, the box open; words, what went; problem, obid's words for why they didn't; busy, on their way.
type Held = { writing?: boolean; words?: string; problem?: string; busy?: boolean };
const NOTHING: Held = {};
let held: ReadonlyMap<string, Held> = new Map();
const listeners = new Set<() => void>();
const hold = (id: string, change: Held | null) => {
  const next = new Map(held);
  if (change) next.set(id, { ...held.get(id), ...change }); else next.delete(id);
  held = next;
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };

export function QuestionBlock({ question: q, onAnswer }: { question: PersonQuestion; onAnswer?: (option: string) => void }) {
  const h = useSyncExternalStore(subscribe, () => held.get(q.id) ?? NOTHING);
  // a click before its words have shown answers nothing (one click, one answer), as AskBlock's
  const [shownAt] = useState(() => performance.now());
  const send = async (words: string, typed?: string) => {
    if (h.busy || performance.now() - shownAt < 500) return;
    hold(q.id, { busy: true, words: typed, problem: undefined });
    try {
      await answerQuestion(q.id, words);
      hold(q.id, null);
      onAnswer?.(words);
    } catch (e) {
      hold(q.id, { busy: false, problem: e instanceof Error ? e.message : String(e) });
    }
  };
  return (
    <div className="tc-ask">
      <span className="tc-ask-kind is-question">Question</span>
      <p className="tc-ask-head">Only you can answer this one.</p>
      <blockquote className="msg-quote room-question">{q.words}</blockquote>
      {q.choices.length > 0 && <p className="tc-ask-text">Their choices: {q.choices.join(' · ')}</p>}
      {q.repos.length > 0 && <p className="tc-ask-text">About {q.repos.join(', ')}</p>}
      {h.problem && <p className="tc-report is-problem" role="alert">{h.problem}</p>}
      {h.writing
        ? <CheckField initial={h.words ?? ''} label="Your answer" use="Send" max={2000} onUse={(words) => { if (words) void send(words, words); }} onCancel={() => hold(q.id, { writing: false })} />
        : (
          <div className="tc-options">
            <button className="tc-option is-primary" onClick={() => hold(q.id, { writing: true })}>Answer</button>
            <button className="tc-option" disabled={h.busy} onClick={() => void send('Decline')}>Decline</button>
            <button className="tc-option is-quiet" onClick={() => { answer(q.id, 'Later'); onAnswer?.('Later'); }}>Later</button>
          </div>
        )}
    </div>
  );
}
