import { useState } from 'react';
import { ASK_LABEL, type Offer } from './mock';
import { answerOffer } from './live';

// Work Oby offers to start (obid's oby_offer), under its answer: each job in obid's words with the
// task Oby planned, quoted, and whether Oby read something it didn't write first; then Dispatch,
// Skip and Later, as a decision in the thread is (AskBlock, kind start). Only this tap starts it;
// answered, the card goes and what started shows under the answer as its tasks.
export default function OfferCard({ offer }: { offer: Offer }) {
  const [busy, setBusy] = useState(false);
  // a click before its words have shown answers nothing (one click, one answer), as AskBlock's
  const [shownAt] = useState(() => performance.now());
  const pick = async (option: 'Dispatch' | 'Skip' | 'Later') => {
    if (performance.now() - shownAt < 500) return;
    setBusy(true);
    try { await answerOffer(offer.id, option); } finally { setBusy(false); }
  };
  return (
    <div className="msg-ask">
      <div className="tc-ask">
        <span className="tc-ask-kind is-start">{ASK_LABEL.start}</span>
        <p className="tc-ask-head">{offer.title}</p>
        {offer.jobs.map((j, i) => (
          <div key={i}>
            <p className="tc-ask-text">{j.what}. The task Oby planned:</p>
            <pre className="tc-code">{j.task}</pre>
          </div>
        ))}
        {offer.untrusted && <p className="tc-ask-text">Oby planned this after reading something it didn’t write, so it never runs in Bypass mode.</p>}
        <div className="tc-options">
          <button className="tc-option is-primary" disabled={busy} onClick={() => pick('Dispatch')}>Dispatch</button>
          <button className="tc-option" disabled={busy} onClick={() => pick('Skip')}>Skip</button>
          <button className="tc-option is-quiet" disabled={busy} onClick={() => pick('Later')}>Later</button>
        </div>
      </div>
    </div>
  );
}
