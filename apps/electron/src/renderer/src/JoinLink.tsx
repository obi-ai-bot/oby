import { useEffect, useId, useState } from 'react';
import { parseInvite, type Joined } from '../../shared/invite';
import { onObid } from './lean';
import { ME } from './spheres';

// Joining an Obysphere from its invite link, pasted in the app (docs/contracts/sphere-app.md,
// SPHEREAPP-3 and 4): on first run's Obysphere step and in Settings › Obysphere. The page only holds
// the link; Join hands it to main, which joins through the one path an oby:// link takes, its own
// dialog included, signed with the app's key after the owner's click. Before any key is made the
// owner id it will act for shows here, and again in main's dialog.

// The owner id obid acts for now, or null until obid says (on the mock: the mock's own). editable:
// obid has the route that changes it before a first join (PUT /v1/obid/sphere/owner, add-only):
// asked once with an empty body, which changes nothing (422 where it's served, 404 where it isn't).
// change: the owner's new id, kept by obid, or why not in words.
const OWNER_ROUTE = '/v1/obid/sphere/owner';
export const OWNER_ID = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;
export function useOwnerId() {
  const [owner, setOwner] = useState<string | null>(onObid ? null : ME.name.toLowerCase());
  const [editable, setEditable] = useState(!onObid);
  useEffect(() => {
    if (!onObid || !window.app?.obid) return;
    let gone = false;
    const call = window.app.obid.call;
    void call('GET', '/v1/obid/sphere').then((a) => {
      if (gone || a.status !== 200) return;
      try {
        const m = JSON.parse(a.text) as { owner?: unknown; state?: unknown };
        if (typeof m.owner === 'string' && m.owner) setOwner(m.owner);
        // only before a first join; a membership keeps the owner it joined as
        if (m.state === 'none') void call('PUT', OWNER_ROUTE, {}).then((p) => { if (!gone) setEditable(p.status !== 404 && p.status !== 405 && p.status !== 0); }, () => {});
      } catch { /* none shown */ }
    }, () => {});
    return () => { gone = true; };
  }, []);
  const change = async (to: string): Promise<string | null> => {
    const next = to.trim().toLowerCase();
    if (!OWNER_ID.test(next)) return 'An owner id is lowercase letters, digits and hyphens.';
    if (!onObid || !window.app?.obid) { setOwner(next); return null; }
    const a = await window.app.obid.call('PUT', OWNER_ROUTE, { owner: next });
    if (a.status >= 200 && a.status < 300) {
      try { const o = (JSON.parse(a.text) as { owner?: unknown }).owner; setOwner(typeof o === 'string' && o ? o : next); } catch { setOwner(next); }
      return null;
    }
    if (a.status === 409) { setEditable(false); return 'It can’t change once you’ve joined an Obysphere.'; }
    if (a.status === 404) { setEditable(false); return 'This Oby can’t change it yet.'; }
    try { return String((JSON.parse(a.text) as { error?: unknown }).error ?? '') || 'obid didn’t take it.'; } catch { return 'obid didn’t take it.'; }
  };
  return { owner, editable, change };
}

// The owner id a join acts for, with Change while obid can change it (SPHEREAPP-9)
function OwnerLine({ who, invite }: { who: ReturnType<typeof useOwnerId>; invite: { name: string; host: string } }) {
  const [editing, setEditing] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const save = async () => {
    if (editing === null) return;
    const why = await who.change(editing);
    setProblem(why);
    if (!why) setEditing(null);
  };
  if (editing !== null) {
    return <>
      <span className="join-link-owner">
        <span className="wl-sub">Join as</span>
        <input className="modal-field setup-field" value={editing} autoFocus aria-label="Owner id" maxLength={63}
          onChange={(e) => { setEditing(e.target.value); setProblem(null); }}
          onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void save(); } if (e.key === 'Escape') { e.stopPropagation(); setEditing(null); setProblem(null); } }}
          autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} />
        <button type="button" className="set-button" onClick={() => void save()}>Save</button>
      </span>
      {problem && <span className="wl-sub is-problem join-link-under" role="alert">{problem}</span>}
    </>;
  }
  return (
    <span className="wl-sub join-link-under">
      {invite.name}, on {invite.host}. You’ll join as <strong>{who.owner}</strong>.
      {who.editable && <> <button type="button" className="join-link-change" onClick={() => setEditing(who.owner ?? '')}>Change</button></>}
      {problem && <span className="is-problem" role="alert"> {problem}</span>}
    </span>
  );
}

const NOT_A_LINK = 'This isn’t an Obysphere invite link.';

export function useJoinLink() {
  const [text, setText] = useState('');
  const [said, setSaid] = useState<{ joined?: string; problem?: string }>({});
  const [busy, setBusy] = useState(false);
  const who = useOwnerId();
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
  return { text, setText: change, invite, who, problem, joined: said.joined ?? null, busy, join };
}

// The field and its Join, then what's true: who it joins as, why not, or that it's in
export function JoinLink({ link, onb = false }: { link: ReturnType<typeof useJoinLink>; onb?: boolean }) {
  const why = useId();
  const { text, setText, invite, who, problem, joined, busy, join } = link;
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
          : invite && who.owner ? <OwnerLine who={who} invite={invite} />
            : null}
    </div>
  );
}
