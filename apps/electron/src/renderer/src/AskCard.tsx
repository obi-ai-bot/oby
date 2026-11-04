import { useEffect, useState } from 'react';
import { Check } from 'lucide-react';
import type { AskMark } from './obid';
import { OwnerAvatar } from './ObyFace';
import { askCardLine, askCardState, named } from './peers';
import './room.css';

// One question to another Oby, in the conversation that asked (docs/contracts/ask-an-oby.md; the
// canvas's AskCard board): the person, the question, what came back and where it stands. obid says
// where; the words are the thread's. Every word from the other Oby is a text node under this byline,
// never Markdown, HTML or a link, so nothing it writes reads as Oby's.
const DOT: Record<string, string> = { asking: 'is-working', waiting: 'is-needs', quiet: 'is-queued', declined: 'is-failed', expired: 'is-failed', refused: 'is-failed' };

export default function AskCard({ card, onOpen }: { card: AskMark; onOpen?: (owner: string) => void }) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const wait = card.state === 'asking' && card.quietAt ? Date.parse(card.quietAt) - Date.now() : 0;
    if (!(wait > 0)) return;
    const t = setTimeout(() => setNow(Date.now()), wait);
    return () => clearTimeout(t);
  }, [card.state, card.quietAt]);
  const state = askCardState(card, now), who = named(card).oby, line = askCardLine(card, now);
  const label = [`Open ${who}: ${card.question}`, card.said, line].filter(Boolean).join('. ');
  return (
    <button type="button" className="ask-card" aria-label={label} onClick={() => onOpen?.(card.owner)}>
      <span className="ask-card-head">
        <OwnerAvatar name={card.name} size={22} />
        <span className="ask-card-who">{who}</span>
        <span className="status" role="img" aria-label={line ?? state}>
          {state === 'answered' ? <Check size={12} strokeWidth={2} /> : <span className={`status-dot ${DOT[state] ?? 'is-queued'}`} />}
        </span>
      </span>
      <span className="ask-card-q">{card.question}</span>
      {card.said && <span className="ask-card-said">{card.said}</span>}
      {line && <span className="ask-card-line">{line}</span>}
    </button>
  );
}
