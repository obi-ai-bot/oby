import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { Check, Copy, X } from 'lucide-react';
import { highlight } from './highlight';
import './replies.css';

// Reading a reply, as T3 Code's are read (docs/contracts/chat-first.md, CHATFIRST-37 to 41): its
// words copied in a click, a code block copied and in colour, a file it names opened in the editor
// at its line, a picture shown full size. Quiet until the pointer is on them, in Sangam's tokens.

// Copy: the words as they were written (the reply's Markdown, a block's code), a check a moment
// after. A clipboard that refuses changes nothing: the words are still there to select.
export function CopyButton({ text, label = 'Copy', className = '' }: { text: string; label?: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1200);
    return () => clearTimeout(t);
  }, [copied]);
  return (
    <button
      type="button"
      className={`reply-copy${copied ? ' is-copied' : ''}${className ? ` ${className}` : ''}`}
      aria-label={copied ? 'Copied' : label}
      title={copied ? 'Copied' : label}
      onClick={() => void navigator.clipboard?.writeText(text).then(() => setCopied(true), () => {})}
    >
      {copied ? <Check size={14} strokeWidth={2} /> : <Copy size={14} strokeWidth={1.8} />}
    </button>
  );
}

// A code block: its code in colour (highlight/), and Copy at its top right
export function CodeBlock({ text, lang }: { text: string; lang?: string }) {
  return (
    <div className="code-block">
      <pre className="ss-code">{highlight(text, lang)}</pre>
      <CopyButton text={text} label="Copy code" className="code-copy" />
    </div>
  );
}

// Where a reply is: a task's (its worktree) or a conversation's (its folders). Its paths are read
// there and nowhere else; none (another Oby's words, a page with neither) leaves every path text.
export type Place = { place: 'task' | 'conversation'; id: string };
export const ReplyPlace = createContext<Place | null>(null);
// its words still coming in: a path may be half written, so none is looked up yet
export const Streaming = createContext(false);

// what main answered, by place and path: a file stays one (main checks again when it's clicked);
// one that wasn't is asked again 10 s on, as an agent may have just written it
const AGAIN_MS = 10_000, KEEP = 5000, BATCH = 200;
const known = new Map<string, { ok: boolean; at: number }>();
const waiting = new Map<string, { at: Place; asks: Map<string, ((ok: boolean) => void)[]> }>();
const keyOf = (at: Place) => `${at.place}:${at.id}`;
export const forgetFiles = () => known.clear();

// whether main finds `ref` a file of `at`'s: the paths asked for at once go in one call
function isFile(at: Place, ref: string): Promise<boolean> {
  const key = keyOf(at), had = known.get(`${key}\n${ref}`);
  if (had && (had.ok || Date.now() - had.at < AGAIN_MS)) return Promise.resolve(had.ok);
  return new Promise((done) => {
    let w = waiting.get(key);
    if (!w) {
      w = { at, asks: new Map() };
      waiting.set(key, w);
      setTimeout(() => void ask(key), 0);
    }
    w.asks.set(ref, [...(w.asks.get(ref) ?? []), done]);
  });
}
async function ask(key: string) {
  const w = waiting.get(key);
  if (!w) return;
  waiting.delete(key);
  if (known.size > KEEP) known.clear();
  const refs = [...w.asks.keys()];
  for (let i = 0; i < refs.length; i += BATCH) {
    const part = refs.slice(i, i + BATCH);
    let answer: boolean[] = [];
    try { answer = (await window.app?.replyFiles?.(w.at.place, w.at.id, part)) ?? []; } catch { /* none, this time */ }
    part.forEach((ref, j) => {
      const ok = answer[j] === true;
      known.set(`${key}\n${ref}`, { ok, at: Date.now() });
      w.asks.get(ref)!.forEach((done) => done(ok));
    });
  }
}

// the editor's name, for a link's tooltip, asked once
let editor: Promise<string | null> | undefined;
const editorName = () => (editor ??= window.app?.editorName?.().catch(() => null) ?? Promise.resolve(null));

// A path in a reply (`src/x.ts`, `src/x.ts:42`): a link that opens it in the editor at that line,
// once main has found it a file of the reply's task or conversation; until then, and when it isn't
// one, the coloured text it was. A click sends main only the words and where the reply is.
export function FileLink({ text, className, children }: { text: string; className: string; children: ReactNode }) {
  const at = useContext(ReplyPlace), streaming = useContext(Streaming);
  const [ok, setOk] = useState(() => (at ? known.get(`${keyOf(at)}\n${text}`)?.ok === true : false));
  const [title, setTitle] = useState('Open in your editor');
  const place = at?.place, id = at?.id;
  useEffect(() => {
    if (!place || !id || streaming || !window.app?.replyFiles) return;
    let live = true;
    void isFile({ place, id }, text).then((v) => { if (live) setOk(v); });
    return () => { live = false; };
  }, [place, id, text, streaming]);
  useEffect(() => {
    if (!ok) return;
    let live = true;
    void editorName().then((name) => { if (live && name) setTitle(name === 'Finder' ? 'Show in Finder' : `Open in ${name}`); });
    return () => { live = false; };
  }, [ok]);
  if (!ok || !place || !id) return <span className={className}>{children}</span>;
  return (
    <a
      href="#"
      className={`${className} tok-file`}
      title={title}
      onClick={(e) => { e.preventDefault(); void window.app?.openReplyFile?.(place, id, text); }}
    >
      {children}
    </a>
  );
}

// A picture that went with a message: a click shows it full size (ImageView)
export function Photo({ src, alt, className }: { src: string; alt: string; className?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="photo-open" aria-label={`Show ${alt} full size`} onClick={() => setOpen(true)}>
        <img className={className} src={src} alt={alt} />
      </button>
      {open && <ImageView src={src} alt={alt} onClose={() => setOpen(false)} />}
    </>
  );
}

// A picture full size over the window: Esc, a click outside it or the close button puts it away,
// and the keys go back where they were. Esc closes only this, never the conversation under it.
export function ImageView({ src, alt, onClose }: { src: string; alt: string; onClose: () => void }) {
  const close = useRef<HTMLButtonElement>(null);
  const done = useRef(onClose);
  done.current = onClose;
  useEffect(() => {
    const was = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    close.current?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopImmediatePropagation();
      done.current();
    };
    window.addEventListener('keydown', key, true);
    return () => { window.removeEventListener('keydown', key, true); if (was?.isConnected) was.focus(); };
  }, []);
  return createPortal(
    <div className="image-view" role="dialog" aria-modal="true" aria-label={alt} onClick={() => done.current()}>
      <img src={src} alt={alt} onClick={(e) => e.stopPropagation()} />
      <button ref={close} type="button" className="image-view-close" aria-label="Close" onClick={() => done.current()}>
        <X size={16} strokeWidth={1.8} />
      </button>
    </div>,
    document.body,
  );
}
