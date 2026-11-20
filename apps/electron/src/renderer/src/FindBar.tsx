import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { composing, registerCommands } from './shortcuts';

// Every place the words are in what's shown under root, in order: one Range each, matched in any
// case, never across two pieces of text (a word split by a link or bold is two). Code, inputs and
// the find bar's own field are left out.
export function findRanges(root: Element | null, query: string): Range[] {
  const q = query.trim().toLowerCase();
  if (!root || !q) return [];
  const ranges: Range[] = [];
  const walker = root.ownerDocument.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.parentElement?.closest('input, textarea, script, style, .find-bar') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const text = (n.textContent ?? '').toLowerCase();
    for (let at = text.indexOf(q); at >= 0; at = text.indexOf(q, at + q.length)) {
      const r = root.ownerDocument.createRange();
      r.setStart(n, at);
      r.setEnd(n, at + q.length);
      ranges.push(r);
    }
  }
  return ranges;
}

// CSS's highlights where the window has them (Chromium): the matches lit, the current one more, and
// nothing in the page changed (app.css, ::highlight(oby-find))
type Highlights = { set(name: string, h: unknown): void; delete(name: string): void };
const highlights = (): Highlights | undefined => (globalThis.CSS as unknown as { highlights?: Highlights } | undefined)?.highlights;
const Hl = (globalThis as unknown as { Highlight?: new (...r: Range[]) => unknown }).Highlight;
function light(all: Range[], current: Range | undefined) {
  const h = highlights();
  if (!h || !Hl) return;
  h.set('oby-find', new Hl(...all));
  if (current) h.set('oby-find-current', new Hl(current)); else h.delete('oby-find-current');
}
const unlight = () => { highlights()?.delete('oby-find'); highlights()?.delete('oby-find-current'); };

// ⌘F in a conversation (docs/contracts/shortcuts.md, SHORTCUTS-3), as T3 Code's find in a thread
// (apps/web/src/components/chat/useThreadFind.ts): a field over the thread's top right that finds the
// words in what's shown, "2 of 5", Return and the arrows to go through them (⇧Return back), each
// scrolled into view; Esc closes it and lights nothing. root: what it finds in; shown: changes when
// the thread does, so a new message is found too; off: a page is open over the conversation.
export default function FindBar({ root, shown, off = false }: { root: React.RefObject<HTMLElement | null>; shown: unknown; off?: boolean }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [at, setAt] = useState(0);
  const [ranges, setRanges] = useState<Range[]>([]);
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (off) return;
    return registerCommands({ 'chat.find': () => { setOpen(true); requestAnimationFrame(() => field.current?.select()); } });
  }, [off]);
  useLayoutEffect(() => {
    if (!open) { unlight(); setRanges([]); return; }
    setRanges(findRanges(root.current?.querySelector('.chat-thread') ?? root.current, query));
  }, [open, query, shown, root]);
  const i = ranges.length ? Math.min(at, ranges.length - 1) : 0;
  useEffect(() => {
    if (!open) return;
    light(ranges, ranges[i]);
    ranges[i]?.startContainer.parentElement?.scrollIntoView?.({ block: 'center' });
  }, [open, ranges, i]);
  useEffect(() => unlight, []);
  if (!open) return null;
  const go = (step: number) => ranges.length && setAt((i + step + ranges.length) % ranges.length);
  const close = () => { setOpen(false); setQuery(''); setAt(0); };
  return (
    <div className="find-bar" role="search" aria-label="Find in conversation">
      <input
        ref={field} className="find-field" autoFocus value={query} spellCheck={false} placeholder="Find in conversation" aria-label="Find in conversation"
        onChange={(e) => { setQuery(e.target.value); setAt(0); }}
        onKeyDown={(e) => {
          if (composing(e)) return;
          if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); }
          else if (e.key === 'Enter') { e.preventDefault(); go(e.shiftKey ? -1 : 1); }
        }}
      />
      <span className="find-count" aria-live="polite">{query.trim() ? (ranges.length ? `${i + 1} of ${ranges.length}` : 'No results') : ''}</span>
      <button type="button" className="icon-button" aria-label="Previous" disabled={!ranges.length} onClick={() => go(-1)}><ChevronUp size={15} strokeWidth={1.8} /></button>
      <button type="button" className="icon-button" aria-label="Next" disabled={!ranges.length} onClick={() => go(1)}><ChevronDown size={15} strokeWidth={1.8} /></button>
      <button type="button" className="icon-button" aria-label="Close" onClick={close}><X size={15} strokeWidth={1.8} /></button>
    </div>
  );
}
