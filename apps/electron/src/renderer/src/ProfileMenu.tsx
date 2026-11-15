import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Plus, X } from 'lucide-react';
import { Menu } from './Menu';
import { AVATAR_COLORS, OwnerAvatar } from './ObyFace';
import { chooseProfile, createProfile, openProfile } from './profiles';
import type { Profile } from './obid';

// The side panel title's menu (screen 07): each profile by its colour's initial and its name, a check
// on the open one and an orange dot on one that waits on you; then New profile…
export default function ProfileMenu({ list, kept, at, onClose, onNew }: {
  list: Profile[];
  kept: string | null;  // the open one's id, as kept (profiles.ts)
  at: DOMRect;
  onClose: () => void;
  onNew: () => void;  // New profile…: its form (NewProfileModal)
}) {
  const open = openProfile(list, kept);
  // its height: a 34px row and 2px gap each, then the rule, New profile… and the menu's padding
  return (
    <Menu at={at} onClose={onClose} align="left" width={220} height={list.length * 36 + 60}>
      {list.map((p) => (
        <button key={p.id} role="menuitemradio" aria-checked={p.id === open.id} className="profile-choice" onClick={() => { chooseProfile(p.id); onClose(); }}>
          <OwnerAvatar name={p.name} size={18} badge={false} color={p.color} />
          {p.name}
          {p.waiting && <span className="status-dot is-needs" role="img" aria-label="Needs you" />}
          {p.id === open.id && <Check size={14} strokeWidth={1.8} className="menu-check" />}
        </button>
      ))}
      <hr />
      <button role="menuitem" className="talk-choice is-add" onClick={() => { onClose(); onNew(); }}>
        <span className="talk-choice-mark"><Plus size={14} strokeWidth={1.8} /></span>
        New profile…
      </button>
    </Menu>
  );
}

// Making a profile: its name and colour (the first one no profile has yet). obid's refusal shows in
// the form, which stays; made, it's the open one. Esc, Cancel, the x or a click outside closes it.
export function NewProfileModal({ taken, onClose }: { taken: string[]; onClose: () => void }) {
  const [name, setName] = useState('');
  const [color, setColor] = useState(() => AVATAR_COLORS.find((c) => !taken.includes(c.toLowerCase())) ?? AVATAR_COLORS[0]);
  const [problem, setProblem] = useState<string | null>(null);
  const [making, setMaking] = useState(false);
  const ready = name.trim().length > 0 && !making;
  const create = async () => {
    if (!ready) return;
    setMaking(true); setProblem(null);
    try {
      await createProfile(name.trim(), color);
      onClose();
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
      <form className="modal is-small" role="dialog" aria-modal="true" aria-labelledby="profile-title" onSubmit={(e) => { e.preventDefault(); void create(); }}>
        <header className="modal-head">
          <h2 id="profile-title">New profile</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        <label className="modal-label" htmlFor="profile-name">Name</label>
        <input id="profile-name" className="modal-field" autoFocus maxLength={40} value={name} onChange={(e) => setName(e.target.value)} autoComplete="off" spellCheck={false} />
        <div className="profile-swatches" role="radiogroup" aria-label="Colour">
          {AVATAR_COLORS.map((c) => (
            <button key={c} type="button" role="radio" aria-checked={c === color} aria-label={c} className={`profile-swatch${c === color ? ' is-on' : ''}`} style={{ background: c }} onClick={() => setColor(c)} />
          ))}
        </div>
        {problem && <p className="sphere-note is-bad" role="alert">{problem}</p>}
        <div className="modal-actions">
          <button type="button" className="set-button" onClick={onClose}>Cancel</button>
          <button type="submit" className="modal-primary" disabled={!ready}>Create profile</button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
