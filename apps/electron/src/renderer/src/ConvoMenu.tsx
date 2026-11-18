import { useEffect, useRef, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';
import { Menu } from './Menu';
import { archiveChat, deleteChat, keepsMarks, markChatUnread, pinChat, useChats, type Chat } from './chats';

// A conversation's menu, from its side panel row (⋮ or right-click) or its
// page's title bar: pin it to the top (or unpin), rename, mark it unread (or read); last, set apart,
// archive it and delete it, which asks first (docs/contracts/conversation-states.md, CHATSTATE-7 to 9)
export default function ConvoMenu({ chat, at, align = 'right', onClose, onRename }: {
  chat: Chat;
  at: DOMRect;
  align?: 'left' | 'right';
  onClose: () => void;
  onRename: () => void;
}) {
  const pick = (fn: () => void) => () => { fn(); onClose(); };
  // a read one only once something's been said in it, or it can't read as new
  const said = chat.said.length > 0 || chat.at != null;
  // an obid that keeps no marks: no Mark unread, no Archive
  const marks = keepsMarks(chat.id);
  return (
    <Menu at={at} align={align} onClose={onClose} height={200} width={180}>
      <button role="menuitem" onClick={pick(() => pinChat(chat.id, !chat.pinned))}>{chat.pinned ? 'Unpin' : 'Pin'}</button>
      <button role="menuitem" onClick={pick(onRename)}>Rename</button>
      {marks && (chat.unread || said) && <button role="menuitem" onClick={pick(() => markChatUnread(chat.id, !chat.unread))}>{chat.unread ? 'Mark as read' : 'Mark as unread'}</button>}
      <hr />
      {marks && <button role="menuitem" onClick={pick(() => archiveChat(chat.id, !chat.archived))}>{chat.archived ? 'Restore' : 'Archive'}</button>}
      <button role="menuitem" className="is-danger" onClick={pick(() => askToDelete(chat.id))}>Delete conversation…</button>
    </Menu>
  );
}

// The one conversation being asked about before it's deleted (CHATSTATE-8): kept here, so the window
// outlives the menu that opened it, wherever that was
let asking: string | null = null;
const listeners = new Set<() => void>();
export const askToDelete = (id: string | null) => { asking = id; listeners.forEach((l) => l()); };
const useAsking = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => asking);

// Delete it? Its name, that its tasks stay, Cancel (focused) and Delete
export function DeleteChatWindow() {
  const id = useAsking();
  const chat = useChats().find((c) => c.id === id);
  const close = useRef(() => askToDelete(null));
  useEffect(() => {
    if (!chat) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, [!!chat]);
  if (!chat) return null;
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) close.current(); }}>
      <div className="modal is-small" role="alertdialog" aria-modal="true" aria-labelledby="delete-chat-title" aria-describedby="delete-chat-what">
        <header className="modal-head">
          <h2 id="delete-chat-title">Delete “{chat.title}”?</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={close.current}><X size={16} strokeWidth={1.6} /></button>
        </header>
        <p id="delete-chat-what" className="modal-lede">Its messages go for good. Tasks it started keep going and stay in your tasks.</p>
        <div className="modal-actions">
          <button type="button" className="set-button" autoFocus onClick={close.current}>Cancel</button>
          <button type="button" className="modal-primary is-danger" onClick={() => { close.current(); deleteChat(chat.id); }}>Delete</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
