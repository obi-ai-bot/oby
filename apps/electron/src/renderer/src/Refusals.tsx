import { X } from 'lucide-react';
import { dismissRefusal, useLive } from './live';

// What obid refused of a click in this thread (a Stop, an answer), in its words: each stays until its
// × dismisses it (docs/contracts/conversation-states.md, CHATSTATE-4), as T3 Code's error banners do.
// where: the thread's own id and its tasks' ids
export default function Refusals({ where }: { where: string[] }) {
  const refused = useLive().refused.filter((r) => where.includes(r.where));
  if (!refused.length) return null;
  return (
    <div className="refusals">
      {refused.map((r) => (
        <div key={r.id} className="refusal" role="alert">
          <span className="refusal-text">{r.text}</span>
          <button type="button" className="icon-button refusal-close" aria-label="Dismiss" onClick={() => dismissRefusal(r.id)}>
            <X size={14} strokeWidth={1.8} />
          </button>
        </div>
      ))}
    </div>
  );
}
