import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';
import { api } from './obid';
import { onObid } from './lean';
import { lookAtThread, named } from './peers';
import { GOAL_MAX, handoffProblem } from '../../shared/handoff';
import type { Repo } from '../../shared/obid-wire';

// Handing a member's Oby a task (docs/contracts/sphere-app.md, SPHEREAPP-6; #2428), from their
// thread: what to do, in which repository and from which branch, and that its tests pass. Hand it
// over goes to main, which sends it through obid's own handoff send only right after this click.
// Their owner says yes or no on their Home; what comes back (yes, no and why, the result) reads in
// this thread, under the task.
export function HandOffButton({ owner, name }: { owner: string; name: string }) {
  const [open, setOpen] = useState(false);
  const { oby } = named({ owner, name });
  return <>
    <button type="button" className="room-handoff" onClick={() => setOpen(true)} aria-haspopup="dialog">Hand {oby} a task</button>
    {open && <HandOffModal owner={owner} name={name} onClose={() => setOpen(false)} />}
  </>;
}

export function HandOffModal({ owner, name, onClose }: { owner: string; name: string; onClose: () => void }) {
  const { person, oby } = named({ owner, name });
  const [goal, setGoal] = useState('');
  const [repo, setRepo] = useState('');
  const [base, setBase] = useState('main');
  const [repos, setRepos] = useState<Repo[]>([]);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [tried, setTried] = useState(false);
  const list = useId();
  useEffect(() => { if (onObid) void api.repos().then((r) => setRepos(r.repos), () => {}); }, []);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  const wrong = handoffProblem({ goal, repo, base });
  const shown = problem ?? (tried ? wrong : null);
  // a repository picked from the list starts from its own default branch
  const pickRepo = (r: string) => {
    setRepo(r);
    const known = repos.find((x) => x.repo === r);
    if (known) setBase(known.defaultBranch);
  };
  const send = async () => {
    setTried(true);
    if (wrong || busy) return;
    setBusy(true);
    const why = window.app?.sphereHandoff ? await window.app.sphereHandoff({ to: owner, goal, repo, base }).catch((e: Error) => e.message) : null;
    setBusy(false);
    if (why) { setProblem(why); return; }
    void lookAtThread(owner);
    onClose();
  };
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <form className="modal is-small" role="dialog" aria-modal="true" aria-labelledby="handoff-title" aria-describedby="handoff-what"
        onSubmit={(e) => { e.preventDefault(); void send(); }}>
        <header className="modal-head">
          <h2 id="handoff-title">Hand {oby} a task</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        <p id="handoff-what" className="modal-lede">{person} says yes or no on their Home. Their Oby does it in its box, and what comes back shows here.</p>
        <label className="modal-label" htmlFor="handoff-goal">What to do</label>
        <input id="handoff-goal" className="modal-field" autoFocus value={goal} maxLength={GOAL_MAX} onChange={(e) => { setGoal(e.target.value); setProblem(null); }}
          placeholder="Add a name option to the greeter" autoComplete="off" spellCheck />
        <div className="handoff-two">
          <span>
            <label className="modal-label" htmlFor="handoff-repo">Repository</label>
            <input id="handoff-repo" className="modal-field" value={repo} onChange={(e) => { pickRepo(e.target.value); setProblem(null); }} list={list}
              placeholder="owner/name" autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} />
            <datalist id={list}>{repos.map((r) => <option key={r.repo} value={r.repo} />)}</datalist>
          </span>
          <span>
            <label className="modal-label" htmlFor="handoff-base">From branch</label>
            <input id="handoff-base" className="modal-field" value={base} onChange={(e) => { setBase(e.target.value); setProblem(null); }}
              autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} />
          </span>
        </div>
        <p className="handoff-note">Done means its tests pass.</p>
        {shown && <p className="set-problem" role="alert">{shown}</p>}
        <div className="modal-actions">
          <button type="button" className="set-button" onClick={onClose}>Cancel</button>
          <button type="submit" className="modal-primary" disabled={busy}>{busy ? 'Handing it over…' : 'Hand it over'}</button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
