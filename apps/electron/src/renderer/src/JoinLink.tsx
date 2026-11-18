import { useEffect, useId, useState } from 'react';
import { parseInvite, type Joined } from '../../shared/invite';
import { onObid } from './lean';
import { ME } from './spheres';

// Joining an Obysphere from its invite link, pasted in the app (docs/contracts/sphere-app.md,
// SPHEREAPP-3 and 4): on first run's Obysphere step and in Settings › Obysphere. The page only holds
// the link; Join hands it to main, which joins through the one path an oby:// link takes, its own
// dialog included, signed with the app's key after the owner's click. Before any key is made the
// owner id it will act for shows here, and again in main's dialog.

// The owner id obid acts for now, or null until obid says (on the mock: the mock's own)
export function useOwnerId() {
  const [owner, setOwner] = useState<string | null>(onObid ? null : ME.name.toLowerCase());
  useEffect(() => {
    if (!onObid || !window.app?.obid) return;
    let gone = false;
    void window.app.obid.call('GET', '/v1/obid/sphere').then((a) => {
      if (gone || a.status !== 200) return;
      try { const o = (JSON.parse(a.text) as { owner?: unknown }).owner; if (typeof o === 'string' && o) setOwner(o); } catch { /* none shown */ }
    }, () => {});
    return () => { gone = true; };
  }, []);
  return owner;
}

const NOT_A_LINK = 'This isn’t an Obysphere invite link.';

export function useJoinLink() {
  const [text, setText] = useState('');
  const [said, setSaid] = useState<{ joined?: string; problem?: string }>({});
  const [busy, setBusy] = useState(false);
  const owner = useOwnerId();
  const invite = parseInvite(text);
  const problem = said.problem ?? (text.trim() && !invite ? NOT_A_LINK : null);
  const join = async () => {
    if (!invite || busy) return;
    setBusy(true);
    // the mock (a browser, ?everything) joins nothing: it says so as obid would
    const r: Joined = window.app?.sphereJoin ? await window.app.sphereJoin(text.trim()).catch((e: Error) => ({ kind: 'failed' as const, name: invite.name, why: e.message }))
      : { kind: 'joined', name: invite.name };
    setBusy(false);
    if (r.kind === 'joined') { setSaid({ joined: r.name }); setText(''); }
    else if (r.kind === 'failed') setSaid({ problem: r.why });
    else if (r.kind === 'not_a_link') setSaid({ problem: NOT_A_LINK });
    else setSaid({});
  };
  const change = (t: string) => { setText(t); setSaid({}); };
  return { text, setText: change, invite, owner, problem, joined: said.joined ?? null, busy, join };
}

// The field and its Join, then what's true: who it joins as, why not, or that it's in
export function JoinLink({ link, onb = false }: { link: ReturnType<typeof useJoinLink>; onb?: boolean }) {
  const why = useId();
  const { text, setText, invite, owner, problem, joined, busy, join } = link;
  return (
    <div className={`join-link${onb ? ' is-onb' : ''}`}>
      <div className="join-link-line">
        <input className="modal-field setup-field" value={text} onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void join(); } }}
          type="url" inputMode="url" placeholder="Paste its invite link" aria-label="Invite link" aria-invalid={!!problem} aria-describedby={problem ? why : undefined}
          autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} />
        <button type="button" className="set-button" disabled={!invite || busy} onClick={() => void join()}>{busy ? 'Joining…' : 'Join'}</button>
      </div>
      {problem ? <span id={why} className="wl-sub is-problem join-link-under" role="alert">{problem}</span>
        : joined ? <span className="wl-sub join-link-under" role="status">You’re in {joined}.</span>
          : invite && owner ? <span className="wl-sub join-link-under">{invite.name}, on {invite.host}. You’ll join as <strong>{owner}</strong>.</span>
            : null}
    </div>
  );
}
