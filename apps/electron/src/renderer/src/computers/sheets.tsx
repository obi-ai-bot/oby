// The picker's sheets (the approved design's screens 02 to 05 and 10; docs/contracts/computers-app.md,
// COMPUTERSAPP-4 to 8 and 11): Add a computer, Over SSH | With a code; adding over SSH, its steps
// ticking off, or what SSH said; this computer's code; and Remove. One small sheet at a time, as the
// New project sheet is drawn: Esc, Cancel or a click outside closes it.
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { CircleCheck, Copy, Fingerprint, LoaderCircle, TriangleAlert, X } from 'lucide-react';
import { Refusal } from './obid';
import { addOverSsh, addWithCode, closeSheet, invite, nameOf, openSheet, readLink, remove, setReach, useComputers, type Sheet } from './store';
import { ROUTE_NAME, fingerprintText, goodUntil, stepWords, type AddStep, type Invitation, type LinkRead } from './view';

export function ComputerSheets() {
  const { ui } = useComputers();
  const sheet = ui.sheet;
  if (!sheet) return null;
  switch (sheet.kind) {
    case 'add': return <AddComputer key={sheet.how} sheet={sheet} />;
    case 'adding': return <Adding sheet={sheet} />;
    case 'code': return <ThisComputersCode />;
    case 'remove': return <RemoveComputer id={sheet.id} />;
  }
}

function ComputerSheet({ title, close, onSubmit, children }: { title: string; close?: boolean; onSubmit: () => void; children: ReactNode }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); closeSheet(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) closeSheet(); }}>
      <form className="modal is-small computer-sheet" role="dialog" aria-modal="true" aria-labelledby="computer-sheet-title" onSubmit={(e) => { e.preventDefault(); onSubmit(); }}>
        <header className="modal-head">
          <h2 id="computer-sheet-title">{title}</h2>
          {close && <button type="button" className="icon-button" aria-label="Close" onClick={closeSheet}><X size={16} strokeWidth={1.6} /></button>}
        </header>
        {children}
      </form>
    </div>,
    document.body,
  );
}

const LEDE = 'Its conversations, tasks and folders open in this window. Oby has to be running there.';

function AddComputer({ sheet }: { sheet: Extract<Sheet, { kind: 'add' }> }) {
  const [host, setHost] = useState(sheet.host ?? '');
  const [link, setLink] = useState(sheet.link ?? '');
  const [code, setCode] = useState(sheet.code ?? '');
  const [found, setFound] = useState<LinkRead | null>(null);
  const [linkProblem, setLinkProblem] = useState<string | null>(null);
  const [codeProblem, setCodeProblem] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const read = useRef(0);

  // obid reads the link, never the window: its name and fingerprint, or its refusal
  useEffect(() => {
    if (sheet.how !== 'code') return;
    const n = ++read.current;
    setFound(null); setLinkProblem(null);
    if (!link.trim()) return;
    readLink(link.trim()).then(
      (r) => { if (n === read.current) setFound(r); },
      (e) => { if (n === read.current) setLinkProblem(e instanceof Refusal ? e.say : (e as Error).message); },
    );
  }, [link, sheet.how]);

  const digits = code.replace(/\D/g, '');
  const ready = sheet.how === 'ssh' ? host.trim().length > 0 : !!found && digits.length === 6 && !adding;
  const submit = async () => {
    if (!ready) return;
    if (sheet.how === 'ssh') return void addOverSsh(host.trim());
    setAdding(true); setCodeProblem(null);
    try {
      await addWithCode(link.trim(), digits);
    } catch (e) {
      setCodeProblem(e instanceof Refusal ? e.say : (e as Error).message);
      setAdding(false);
    }
  };
  const tab = (how: 'ssh' | 'code', label: string) => (
    <button type="button" role="tab" aria-selected={sheet.how === how} className={sheet.how === how ? 'is-on' : ''}
      onClick={() => openSheet({ kind: 'add', how, host, link, code })}>{label}</button>
  );

  return (
    <ComputerSheet title="Add a computer" onSubmit={() => void submit()}>
      <p className="modal-lede">{LEDE}</p>
      <div className="panel-side" role="tablist" aria-label="How to add it">
        {tab('ssh', 'Over SSH')}
        {tab('code', 'With a code')}
      </div>
      {sheet.how === 'ssh' ? (
        <>
          <label className="modal-label" htmlFor="ssh-host">SSH host</label>
          <input id="ssh-host" className="modal-field" autoFocus autoComplete="off" spellCheck={false} placeholder="devbox, or you@192.168.1.20" value={host} onChange={(e) => setHost(e.target.value)} />
          <p className="computer-hint">Oby uses your own SSH: your keys, agent and known hosts. Nothing is installed there.</p>
        </>
      ) : (
        <>
          <label className="modal-label" htmlFor="pair-link">Link</label>
          <input id="pair-link" className="modal-field is-mono" autoFocus autoComplete="off" spellCheck={false} value={link} onChange={(e) => setLink(e.target.value)} />
          {linkProblem && <p className="sphere-note is-bad" role="alert">{linkProblem}</p>}
          <p className="computer-hint">On the other computer, choose <b>Show this computer’s code</b> below. On Linux, run <code>obid invite-computer</code>.</p>
          {found && (
            <>
              <div className="computer-found">
                <Fingerprint size={18} strokeWidth={1.6} aria-hidden="true" />
                <div>
                  <b>{found.name}</b>
                  <span className="fingerprint">{fingerprintText(found.fingerprint)}</span>
                  <span>Check that {found.name} shows the same fingerprint.</span>
                </div>
              </div>
              <label className="modal-label" htmlFor="pair-code">Code</label>
              <input id="pair-code" className="modal-field is-code" autoComplete="one-time-code" inputMode="numeric" value={code} onChange={(e) => { setCode(e.target.value); setCodeProblem(null); }} />
              {codeProblem && <p className="sphere-note is-bad" role="alert">{codeProblem}</p>}
            </>
          )}
          <a className="computer-switch-link" href="#" onClick={(e) => { e.preventDefault(); openSheet({ kind: 'code' }); }}>Show this computer’s code</a>
        </>
      )}
      <div className="modal-actions">
        <button type="button" className="set-button" onClick={closeSheet}>Cancel</button>
        <button type="submit" className="modal-primary" disabled={!ready}>Add computer</button>
      </div>
    </ComputerSheet>
  );
}

// obid's words, with what to type in a terminal (between backticks) as code
function Said({ words }: { words: string }) {
  return <>{words.split('`').map((part, i) => (i % 2 ? <code key={i}>{part}</code> : part))}</>;
}

const LATER = <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><circle cx="12" cy="12" r="9" strokeDasharray="2 3" /></svg>;

function Adding({ sheet }: { sheet: Extract<Sheet, { kind: 'adding' }> }) {
  const { host, steps, trouble } = sheet;
  return (
    <ComputerSheet title={`Adding ${host}`} onSubmit={() => void addOverSsh(host)}>
      <ul className="modal-list" aria-live="polite">
        {trouble ? (
          <li className="is-trouble" role="alert"><TriangleAlert size={16} strokeWidth={1.8} aria-hidden="true" /><span><Said words={trouble} /></span></li>
        ) : (['ssh', 'pair', 'routes'] as AddStep[]).map((step) => {
          const st = steps[step];
          return (
            <li key={step} className={st === 'done' ? 'is-done' : st === 'doing' ? 'is-now' : 'is-later'}>
              {st === 'done' ? <CircleCheck size={16} strokeWidth={1.8} aria-label="Done" /> : st === 'doing' ? <LoaderCircle size={16} strokeWidth={1.8} aria-hidden="true" /> : LATER}
              <span>{stepWords(step, host)}</span>
            </li>
          );
        })}
      </ul>
      <div className="modal-actions">
        <button type="button" className="set-button" onClick={closeSheet}>Cancel</button>
        {trouble && <button type="submit" className="modal-primary">Try again</button>}
      </div>
    </ComputerSheet>
  );
}

function ThisComputersCode() {
  const [inv, setInv] = useState<Invitation | null>(null);
  useEffect(() => { void invite().then(setInv); }, []);
  const turn = (kind: Invitation['reach'][number]['kind'], on: boolean) => {
    setInv((o) => o && { ...o, reach: o.reach.map((r) => (r.kind === kind ? { ...r, on } : r)) });
    void setReach(kind, on);
  };
  return (
    <ComputerSheet title="This Mac’s code" close onSubmit={closeSheet}>
      {inv && (
        <>
          <p className="modal-lede">On the Mac you’re adding {inv.name} to, paste its link and type this code.</p>
          <p className="big-code" aria-label={`Code ${inv.code.split('').join(' ')}`}>{inv.code.slice(0, 3)} {inv.code.slice(3)}</p>
          <span className="modal-label">Fingerprint</span>
          <span className="fingerprint">{fingerprintText(inv.fingerprint)}</span>
          <span className="modal-label">Other computers reach this Mac at</span>
          {inv.reach.length === 0 && <p className="computer-hint">This Mac has no address another computer can reach. Add it over SSH instead.</p>}
          {inv.reach.map((r) => (
            <div key={r.kind} className="reach-row">
              <span>{ROUTE_NAME[r.kind]}<small>{r.address}</small></span>
              <button type="button" role="switch" className="set-switch" aria-checked={r.on} aria-label={ROUTE_NAME[r.kind]} onClick={() => turn(r.kind, !r.on)} />
            </div>
          ))}
          <p className="computer-hint">{goodUntil(inv.expiresAt)}</p>
        </>
      )}
      <div className="modal-actions">
        <button type="button" className="set-button" disabled={!inv} onClick={() => { if (inv) void navigator.clipboard?.writeText(inv.link); }}><Copy size={14} strokeWidth={1.8} aria-hidden="true" />Copy link</button>
        <button type="submit" className="modal-primary">Done</button>
      </div>
    </ComputerSheet>
  );
}

function RemoveComputer({ id }: { id: string }) {
  const name = nameOf(id);
  return (
    <ComputerSheet title={`Remove ${name}?`} close onSubmit={() => void remove(id)}>
      <p className="modal-lede">This Mac stops reaching {name}, and {name} forgets this Mac’s key. Its tasks and folders stay on {name}.</p>
      <div className="modal-actions">
        <button type="button" className="set-button" onClick={closeSheet}>Cancel</button>
        {/* "Remove computer", not his bare "Remove": the owner's choice of 2026-11-21, so no other Remove
            button's click can sign a computer's removal (shared/words.ts, REMOVE_COMPUTER) */}
        <button type="submit" className="modal-primary is-danger">Remove computer</button>
      </div>
    </ComputerSheet>
  );
}
