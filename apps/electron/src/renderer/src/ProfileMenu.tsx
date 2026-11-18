import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Pencil, Plus, X } from 'lucide-react';
import { Menu } from './Menu';
import { OwnerAvatar } from './ObyFace';
import { changeProfile, chooseProfile, createProfile, deleteProfile, openProfile } from './profiles';
import type { Profile } from './obid';

// The side panel title's menu (screen 07): each profile by its colour's initial and its name, a check
// on the open one and an orange dot on one that waits on you; then Edit profile… (the open one's
// name, colour and delete) and New profile…
export default function ProfileMenu({ list, kept, at, onClose, onNew, onEdit }: {
  list: Profile[];
  kept: string | null;  // the open one's id, as kept (profiles.ts)
  at: DOMRect;
  onClose: () => void;
  onNew: () => void;  // New profile…: its form (NewProfileModal)
  onEdit: (p: Profile) => void;  // Edit profile…: the open one's form (EditProfileModal)
}) {
  const open = openProfile(list, kept);
  // its height: a 34px row and 2px gap each, then the rule, Edit profile…, New profile… and the menu's padding
  return (
    <Menu at={at} onClose={onClose} align="left" width={220} height={list.length * 36 + 96}>
      {list.map((p) => (
        <button key={p.id} role="menuitemradio" aria-checked={p.id === open.id} className="profile-choice" onClick={() => { chooseProfile(p.id); onClose(); }}>
          <OwnerAvatar name={p.name} size={18} badge={false} color={p.color} />
          {p.name}
          {p.waiting && <span className="status-dot is-needs" role="img" aria-label="Needs you" />}
          {p.id === open.id && <Check size={14} strokeWidth={1.8} className="menu-check" />}
        </button>
      ))}
      <hr />
      <button role="menuitem" className="talk-choice is-add" onClick={() => { onClose(); onEdit(open); }}>
        <span className="talk-choice-mark"><Pencil size={13} strokeWidth={1.8} /></span>
        Edit profile…
      </button>
      <button role="menuitem" className="talk-choice is-add" onClick={() => { onClose(); onNew(); }}>
        <span className="talk-choice-mark"><Plus size={14} strokeWidth={1.8} /></span>
        New profile…
      </button>
    </Menu>
  );
}

// Making a profile: its name and colour (the first one no profile has yet). obid's refusal shows in
// the form, which stays; made, it's the open one. Esc, Cancel, the x or a click outside closes it.
// A new profile's colours, by name (Sangam's screen 18)
export const PROFILE_COLORS = [['Blue', '#3a5a8c'], ['Green', '#5a7a4f'], ['Brown', '#8c5a3a'], ['Purple', '#6a4f8c'], ['Grey', '#5c5c60']] as const;

export function NewProfileModal({ taken, onClose }: { taken: string[]; onClose: () => void }) {
  const first = PROFILE_COLORS.find(([, c]) => !taken.includes(c.toLowerCase()))?.[1] ?? PROFILE_COLORS[0][1];
  return <ProfileForm title="New profile" go="Create profile" name="" color={first} onClose={onClose}
    lede="A profile is a place of its own: its conversations, projects, agents, routines and vault. Nothing crosses from one profile to another."
    onGo={async (name, color) => { await createProfile(name, color); }} />;
}

// Editing the open profile (docs/contracts/profiles.md, PROFILES-48 and 49): its name and colour as
// they are, Save sending only what changed; and, for one that isn't the default, Delete profile…,
// which asks first and says where its work goes (the default's, nothing deleted). obid's refusal shows
// in the form, which stays. The words are proposals, Sangam's to rewrite.
export function EditProfileModal({ profile, defaultName, onClose }: { profile: Profile; defaultName: string; onClose: () => void }) {
  const [asking, setAsking] = useState(false);
  if (asking) return <DeleteProfile profile={profile} defaultName={defaultName} onClose={onClose} onBack={() => setAsking(false)} />;
  const known = PROFILE_COLORS.some(([, c]) => c.toLowerCase() === profile.color.toLowerCase());
  return <ProfileForm title="Edit profile" go="Save" name={profile.name} color={profile.color} extra={known ? undefined : profile.color} onClose={onClose}
    onDelete={profile.default ? undefined : () => setAsking(true)}
    onGo={async (name, color) => {
      const change = { ...(name !== profile.name ? { name } : {}), ...(color.toLowerCase() !== profile.color.toLowerCase() ? { color } : {}) };
      if (Object.keys(change).length) await changeProfile(profile.id, change);
    }} />;
}

function DeleteProfile({ profile, defaultName, onClose, onBack }: { profile: Profile; defaultName: string; onClose: () => void; onBack: () => void }) {
  const [problem, setProblem] = useState<string | null>(null);
  const [going, setGoing] = useState(false);
  const remove = async () => {
    setGoing(true); setProblem(null);
    try {
      await deleteProfile(profile.id);
      onClose();
    } catch (e) {
      setProblem((e as Error).message);
      setGoing(false);
    }
  };
  return <Sheet title={`Delete ${profile.name}?`} onClose={onClose} onSubmit={() => void remove()}>
    <p className="modal-lede">Its conversations, tasks, agents, projects and accounts move to {defaultName}. Nothing is deleted with it.</p>
    {problem && <p className="sphere-note is-bad" role="alert">{problem}</p>}
    <div className="modal-actions">
      <button type="button" className="set-button" onClick={onBack}>Cancel</button>
      <button type="submit" className="modal-primary is-danger" disabled={going}>Delete</button>
    </div>
  </Sheet>;
}

// The form both use: a name and a colour, obid's refusal under them
function ProfileForm({ title, go, lede, name: name0, color: color0, extra, onGo, onDelete, onClose }: {
  title: string; go: string; lede?: string; name: string; color: string;
  extra?: string;  // a colour of the profile's own that isn't one of the five: shown first, so it stays choosable
  onGo: (name: string, color: string) => Promise<void>; onDelete?: () => void; onClose: () => void;
}) {
  const [name, setName] = useState(name0);
  const [color, setColor] = useState<string>(color0);
  const [problem, setProblem] = useState<string | null>(null);
  const [making, setMaking] = useState(false);
  const ready = name.trim().length > 0 && !making;
  const submit = async () => {
    if (!ready) return;
    setMaking(true); setProblem(null);
    try {
      await onGo(name.trim(), color);
      onClose();
    } catch (e) {
      setProblem((e as Error).message);
      setMaking(false);
    }
  };
  const colours: (readonly [string, string])[] = [...(extra ? [['Its own', extra] as const] : []), ...PROFILE_COLORS];
  return <Sheet title={title} onClose={onClose} onSubmit={() => void submit()}>
    {lede && <p className="modal-lede">{lede}</p>}
    <label className="modal-label" htmlFor="profile-name">Name</label>
    <input id="profile-name" className="modal-field" autoFocus maxLength={40} value={name} onChange={(e) => setName(e.target.value)} autoComplete="off" spellCheck={false} />
    <span className="modal-label" id="profile-colour">Colour</span>
    <div className="profile-swatches" role="radiogroup" aria-labelledby="profile-colour">
      {colours.map(([label, c]) => {
        const on = c.toLowerCase() === color.toLowerCase();
        return <button key={c} type="button" role="radio" aria-checked={on} aria-label={label} className={`profile-swatch${on ? ' is-on' : ''}`} style={{ background: c }} onClick={() => setColor(c)} />;
      })}
    </div>
    {problem && <p className="sphere-note is-bad" role="alert">{problem}</p>}
    <div className="modal-actions">
      {onDelete && <button type="button" className="set-button is-danger profile-delete" onClick={onDelete}>Delete profile…</button>}
      <button type="button" className="set-button" onClick={onClose}>Cancel</button>
      <button type="submit" className="modal-primary" disabled={!ready}>{go}</button>
    </div>
  </Sheet>;
}

// A small sheet over the window: Esc, the x or a click outside closes it
function Sheet({ title, onClose, onSubmit, children }: { title: string; onClose: () => void; onSubmit: () => void; children: React.ReactNode }) {
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
      <form className="modal is-small profile-sheet" role="dialog" aria-modal="true" aria-labelledby="profile-title" onSubmit={(e) => { e.preventDefault(); onSubmit(); }}>
        <header className="modal-head">
          <h2 id="profile-title">{title}</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        {children}
      </form>
    </div>,
    document.body,
  );
}
