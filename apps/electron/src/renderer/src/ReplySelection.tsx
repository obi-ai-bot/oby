import { Quote } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

// Words picked out of a reply, Oby's or an agent's (docs/contracts/composer.md, COMPOSER-23, 24):
// once the pointer lets go of a selection in one, a small "Reply about this" button shows by its
// end, as T3 Code's AssistantSelectionToolbar does; a click puts the words in the message box as a
// Markdown quote above the caret (Composer's quote). Nothing else is picked up: the owner's own
// words, a task's card, two replies at once.

// What counts as a reply's words: Oby's (and another Oby's it quotes), an agent's in its session
export const REPLY = '.msg.is-oby .msg-bubble, .msg-quote, .ss-said';

// The words of `selection` when they lie within one reply under `root`, else null
export function replyWords(root: HTMLElement | null, selection: Selection | null): { text: string; range: Range } | null {
  if (!root || !selection || selection.isCollapsed || !selection.rangeCount) return null;
  const range = selection.getRangeAt(0);
  const at = (n: Node | null) => (n?.nodeType === Node.ELEMENT_NODE ? (n as Element) : n?.parentElement ?? null)?.closest(REPLY) ?? null;
  const reply = at(range.startContainer);
  if (!reply || reply !== at(range.endContainer) || !root.contains(reply)) return null;
  const text = selection.toString().trim();
  return text ? { text, range } : null;
}

// The button's spot: under the selection's last line, at its end, kept inside the window
function spotOf(range: Range): { x: number; y: number } {
  const rects = range.getClientRects();
  const r = rects.length ? rects[rects.length - 1] : range.getBoundingClientRect();
  return { x: r.right, y: r.bottom + 6 };
}

export default function ReplySelection({ root, onQuote }: { root: React.RefObject<HTMLElement | null>; onQuote: (text: string) => void }) {
  const [shown, setShown] = useState<{ text: string; x: number; y: number } | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    let down = false;
    const look = () => {
      const picked = down ? null : replyWords(root.current, getSelection());
      setShown(picked ? { text: picked.text, ...spotOf(picked.range) } : null);
    };
    const press = (e: PointerEvent) => { if (!button.current?.contains(e.target as Node)) { down = true; setShown(null); } };
    const release = () => { if (down) { down = false; look(); } };
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') setShown(null); };
    document.addEventListener('pointerdown', press, true);
    document.addEventListener('pointerup', release, true);
    document.addEventListener('selectionchange', look);
    document.addEventListener('keydown', key, true);
    // the thread scrolled: the button follows its words
    document.addEventListener('scroll', look, true);
    return () => {
      document.removeEventListener('pointerdown', press, true);
      document.removeEventListener('pointerup', release, true);
      document.removeEventListener('selectionchange', look);
      document.removeEventListener('keydown', key, true);
      document.removeEventListener('scroll', look, true);
    };
  }, [root]);
  // kept inside the window, measured before it paints
  useLayoutEffect(() => {
    const el = button.current;
    if (!el || !shown) return;
    const r = el.getBoundingClientRect();
    el.style.left = `${Math.max(8, Math.min(shown.x - r.width, innerWidth - r.width - 8))}px`;
    el.style.top = `${Math.max(8, Math.min(shown.y, innerHeight - r.height - 8))}px`;
  }, [shown]);
  if (!shown) return null;
  const quote = () => {
    onQuote(shown.text);
    getSelection()?.removeAllRanges();
    setShown(null);
  };
  return createPortal(
    <button
      ref={button}
      type="button"
      className="reply-about"
      style={{ left: shown.x, top: shown.y }}
      // the selection stays while it's pressed
      onPointerDown={(e) => e.preventDefault()}
      onClick={quote}
    >
      <Quote size={12} strokeWidth={2} aria-hidden="true" />Reply about this
    </button>,
    document.body,
  );
}
