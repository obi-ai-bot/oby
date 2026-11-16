import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { AGENT_COLORS, ObyFace } from './ObyFace';
import { BrandMark } from './brands';
import { AGENTS } from './view';
import { createHelper } from './helpers';
import { onObid } from './lean';
import type { Helper } from './obid';

// What an agent can use (screen 10), in the order obid keeps them. Gmail and Slack are drawn, but obid
// has no such connector: on obid they can't be chosen and are never sent (docs/contracts/helpers.md)
const CAN_USE = [
  { id: 'gmail', label: 'Gmail', obid: false },
  { id: 'slack', label: 'Slack', obid: false },
  { id: 'github', label: 'GitHub', obid: true },
  { id: 'web', label: 'The web', obid: true },
];
const TOOLS = ['claude', 'codex', 'cursor'] as const;

// New agent (screen 10): its Oby's colour (the first no agent of this profile has), its name, what it
// helps with, the tool it runs on and what it can use. Sent as chosen (POST /v1/helpers); obid's
// refusal shows in the sheet, which stays as it was. Esc, Cancel or a click outside closes it.
export default function AgentSheet({ profile, taken, onClose, onMade }: {
  profile?: string;  // the open profile's id, sent only when the panel has a profile list
  taken: string[];   // the colours this profile's agents have
  onClose: () => void;
  onMade: (helper: Helper) => void;
}) {
  const [color, setColor] = useState(() => AGENT_COLORS.find((c) => !taken.includes(c)) ?? AGENT_COLORS[0]);
  const [name, setName] = useState('');
  const [instructions, setInstructions] = useState('');
  const [tool, setTool] = useState<(typeof TOOLS)[number]>('claude');
  const [uses, setUses] = useState<ReadonlySet<string>>(() => new Set());
  const [problem, setProblem] = useState<string | null>(null);
  const [making, setMaking] = useState(false);
  const ready = name.trim().length > 0 && instructions.trim().length > 0 && !making;
  const choosable = (c: (typeof CAN_USE)[number]) => c.obid || !onObid;
  const toggle = (id: string) => setUses((u) => { const next = new Set(u); if (!next.delete(id)) next.add(id); return next; });
  const create = async () => {
    if (!ready) return;
    setMaking(true); setProblem(null);
    try {
      const connectors = CAN_USE.filter((c) => choosable(c) && uses.has(c.id)).map((c) => c.id);
      onMade(await createHelper({ name: name.trim(), color, instructions: instructions.trim(), tool, connectors, ...(profile ? { profile } : {}) }));
    } catch (e) {
      setProblem((e as Error).message);
      setMaking(false);
    }
  };

  // the latest onClose (the panel behind re-renders often, as Oby's words stream in)
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);

  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <form className="modal is-small agent-sheet" role="dialog" aria-modal="true" aria-labelledby="agent-title" onSubmit={(e) => { e.preventDefault(); void create(); }}>
        <h2 id="agent-title" className="agent-sheet-title">New agent</h2>
        <p className="agent-sheet-lede">An agent is an Oby of its own: one conversation with you, one kind of work. It stays in this profile.</p>
        <span className="modal-label" id="agent-oby">Its Oby</span>
        <div className="agent-swatches" role="radiogroup" aria-labelledby="agent-oby">
          {AGENT_COLORS.map((c) => (
            <button key={c} type="button" role="radio" aria-checked={c === color} aria-label={c} className={`agent-swatch${c === color ? ' is-on' : ''}`} onClick={() => setColor(c)}>
              <ObyFace size={24} color={c} />
            </button>
          ))}
        </div>
        <label className="modal-label" htmlFor="agent-name">Name</label>
        <input id="agent-name" className="modal-field" autoFocus maxLength={100} value={name} onChange={(e) => setName(e.target.value)} autoComplete="off" spellCheck={false} />
        <label className="modal-label" htmlFor="agent-instructions">What should this agent help with?</label>
        <textarea id="agent-instructions" className="modal-field agent-instructions" maxLength={4000} rows={2} value={instructions} onChange={(e) => setInstructions(e.target.value)} />
        <span className="modal-label" id="agent-tool">Runs on</span>
        <div className="agent-chips" role="radiogroup" aria-labelledby="agent-tool">
          {TOOLS.map((t) => (
            <button key={t} type="button" role="radio" aria-checked={t === tool} className={`agent-chip${t === tool ? ' is-on' : ''}`} onClick={() => setTool(t)}>
              <BrandMark brand={AGENTS[t].brand} size={14} />
              {AGENTS[t].name}
            </button>
          ))}
        </div>
        <span className="modal-label" id="agent-uses">Can use</span>
        <div className="agent-chips" role="group" aria-labelledby="agent-uses">
          {CAN_USE.map((c) => (
            <button key={c.id} type="button" aria-pressed={choosable(c) && uses.has(c.id)} disabled={!choosable(c)} aria-disabled={!choosable(c) || undefined}
              className={`agent-chip${choosable(c) && uses.has(c.id) ? ' is-on' : ''}`} onClick={() => toggle(c.id)}>
              {c.label}
            </button>
          ))}
        </div>
        {problem && <p className="sphere-note is-bad" role="alert">{problem}</p>}
        <div className="modal-actions">
          <button type="button" className="set-button" onClick={onClose}>Cancel</button>
          <button type="submit" className="modal-primary" disabled={!ready}>Create agent</button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
