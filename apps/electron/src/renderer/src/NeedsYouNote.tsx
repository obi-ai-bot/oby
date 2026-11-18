import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { chatOfTask, useChats, type Chat } from './chats';
import { profileChats, useProfiles } from './profiles';
import { tasksWith, useTasks } from './tasks';

// Another conversation needs you while Oby's window is in front (docs/contracts/alerts.md,
// ALERTS-21): nothing alerts on the Mac then (ALERTS-8), so the window says it, quietly, the way T3
// Code's in-app toast does (ThreadNotificationCoordinator): a note in the window's corner with the
// conversation's title and Open, gone after NOTE_FOR or when that conversation opens. Only what
// newly needs you: what needed you as the window opened, or before, says nothing again. Words
// proposed, Sangam's to rewrite.

// how long a note stays
export const NOTE_FOR = 8000;

// the conversations something in waits on you: their own (Chat.needs) and their tasks'
export const needingOf = (chats: Chat[], taskChats: string[]) =>
  [...new Set([...chats.filter((c) => c.needs).map((c) => c.id), ...taskChats])].filter((id) => chats.some((c) => c.id === id));

// what's new among `now` since `before` (null: the first look, a baseline), but the one open
export const newlyNeeding = (before: Set<string> | null, now: string[], open: string | null) =>
  before ? now.filter((id) => !before.has(id) && id !== open) : [];

// whether the owner is looking at Oby's window: shown and focused, as ALERTS-8's rule reads it
export const inFront = () => document.visibilityState === 'visible' && document.hasFocus();

export default function NeedsYouNote({ open, onOpen }: { open: string | null; onOpen: (conversation: string) => void }) {
  useTasks();
  const profiles = useProfiles();
  const chats = profileChats(useChats(), profiles.list, profiles.kept);
  const now = needingOf(chats, tasksWith('needs').flatMap((t) => chatOfTask(t.id)?.id ?? []));
  const seen = useRef<Set<string> | null>(null);
  const [note, setNote] = useState<{ id: string; more: number } | null>(null);
  const key = now.join('|');
  useEffect(() => {
    const fresh = newlyNeeding(seen.current, now, open);
    seen.current = new Set(now);
    if (fresh.length && inFront()) setNote({ id: fresh[0], more: fresh.length - 1 });
  }, [key, open]);
  useEffect(() => {
    if (!note) return;
    const t = setTimeout(() => setNote(null), NOTE_FOR);
    return () => clearTimeout(t);
  }, [note]);
  const chat = note && chats.find((c) => c.id === note.id);
  // opened, answered (it no longer needs you) or gone: the note goes with it
  if (!note || !chat || note.id === open || !now.includes(note.id)) return null;
  return (
    <div className="needs-note" role="status" aria-live="polite">
      <span className="needs-note-dot" aria-hidden />
      <span className="needs-note-words">
        <b>Needs you</b>
        <span className="needs-note-title">{chat.title || 'A conversation'}{note.more ? ` and ${note.more} more` : ''}</span>
      </span>
      <button className="set-button" onClick={() => { setNote(null); onOpen(chat.id); }}>Open</button>
      <button className="icon-button needs-note-close" aria-label="Dismiss" onClick={() => setNote(null)}><X size={14} strokeWidth={1.8} /></button>
    </div>
  );
}
