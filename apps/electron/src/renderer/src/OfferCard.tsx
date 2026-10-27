import { useState } from 'react';
import { ASK_LABEL, type Offer } from './mock';
import { answerOffer } from './live';

// Work Oby offers to start (obid's oby_offer), under its answer: each job in obid's words, then
// Dispatch, Skip and Later, as a decision in the thread is (AskBlock, kind start). Only this tap
// starts it; answered, the card goes and what started shows under the answer as its tasks.
export default function OfferCard({ offer }: { offer: Offer }) {
  const [busy, setBusy] = useState(false);
  const pick = async (option: 'Dispatch' | 'Skip' | 'Later') => {
    setBusy(true);
    try { await answerOffer(offer.id, option); } finally { setBusy(false); }
  };
  return (
    <div className="msg-ask">
      <div className="tc-ask">
        <span className="tc-ask-kind is-start">{ASK_LABEL.start}</span>
        <p className="tc-ask-head">{offer.title}</p>
        {offer.what.map((w, i) => <p key={i} className="tc-ask-text">{w}</p>)}
        <div className="tc-options">
          <button className="tc-option is-primary" disabled={busy} onClick={() => pick('Dispatch')}>Dispatch</button>
          <button className="tc-option" disabled={busy} onClick={() => pick('Skip')}>Skip</button>
          <button className="tc-option is-quiet" disabled={busy} onClick={() => pick('Later')}>Later</button>
        </div>
      </div>
    </div>
  );
}
