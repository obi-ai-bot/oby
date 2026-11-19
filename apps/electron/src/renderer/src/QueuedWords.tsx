import { useState } from 'react';
import { Clock3, Paperclip, Pencil, X } from 'lucide-react';
import type { Queued } from './chats';

// Words sent while Oby answers, waiting their turn (docs/contracts/composer.md, COMPOSER-8 to 10), as
// T3 Code's queued runs show: over the message box, each on one line with Edit and Cancel, until a
// turn takes them (then they're in the conversation). Cancel drops them alone (POST
// /v1/plans/cancel with their turn); Edit does too and puts their words back in the box. While
// obid answers either, the row's buttons wait.
export default function QueuedWords({ queued, onCancel, onEdit }: {
  queued: Queued[];
  onCancel: (q: Queued) => Promise<unknown>;
  onEdit: (q: Queued) => Promise<unknown>;
}) {
  const [busy, setBusy] = useState<string | null>(null);
  if (!queued.length) return null;
  const run = (q: Queued, act: (q: Queued) => Promise<unknown>) => {
    setBusy(q.id);
    void act(q).finally(() => setBusy(null));
  };
  return (
    <div className="queued-words" role="list" aria-label="Waiting to send">
      {queued.map((q) => (
        <div key={q.id} role="listitem" className="queued-row" aria-busy={busy === q.id || undefined}>
          <Clock3 size={14} strokeWidth={1.7} className="queued-icon" aria-hidden="true" />
          <span className="queued-text" title={q.text}>{q.text || q.files.join(', ')}</span>
          {q.files.length > 0 && q.text && <span className="queued-files" title={q.files.join(', ')}><Paperclip size={12} strokeWidth={1.7} />{q.files.length}</span>}
          <span className="queued-label">Waiting</span>
          <button type="button" className="icon-button" aria-label={`Edit “${q.text}”`} title="Edit" disabled={busy === q.id} onMouseDown={(e) => e.preventDefault()} onClick={() => run(q, onEdit)}>
            <Pencil size={13} strokeWidth={1.7} />
          </button>
          <button type="button" className="icon-button" aria-label={`Cancel “${q.text}”`} title="Cancel" disabled={busy === q.id} onMouseDown={(e) => e.preventDefault()} onClick={() => run(q, onCancel)}>
            <X size={14} strokeWidth={1.8} />
          </button>
        </div>
      ))}
    </div>
  );
}
