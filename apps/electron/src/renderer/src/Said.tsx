import { Component, createElement, Fragment, useMemo, type ReactNode } from 'react';
import { lexer, type Token, type Tokens } from 'marked';
import { tokenize } from './textTokens';

// What Oby (or a job's agent) said, from its Markdown (GitHub's): paragraphs, lists (nested,
// numbered, to-dos), **bold**, *italic*, `code` and code blocks, links, headings, quotes and
// tables. Built as elements from marked's tokens, never as HTML: HTML in a message shows as the
// text it is. In plain text, references, paths and @mentions are set apart as in the message
// box, and each word is its own span, so a word coming in can fade in (see .is-streaming). Read
// again only when its text changes: the conversation renders again with each word coming in.
// streaming: its words are still coming in (closed, below)
export default function Said({ text, streaming = false }: { text: string; streaming?: boolean }) {
  return <Safe text={text}><Blocks text={text} streaming={streaming} /></Safe>;
}
function Blocks({ text, streaming }: { text: string; streaming: boolean }) {
  return useMemo(() => { const ts = lex(streaming ? closed(text) : text); return ts ? blocks(ts) : <p>{text}</p>; }, [text, streaming]);
}

// marked slows down sharply on long runs of *, _, ~ or ! and on deep nesting, and throws past a
// few thousand levels; the words are anyone's (an agent quoting a GitHub comment). Past these, or
// if it throws anyway, a message shows as the text it is.
// ponytail: limits measured on marked 18.1.1, each shape under ~20 ms; parse in a Worker with a
// time budget if another slow shape turns up
const LIMIT = 16 * 1024, MARKS = 300;
function lex(text: string): Token[] | null {
  if (text.length > LIMIT) return null;
  // its *, _, ~ and ! outside code
  if ((text.replace(/```[\s\S]*?(?:```|$)|`[^`]*`/g, '').match(/[*_~!]/g)?.length ?? 0) > MARKS) return null;
  // a line 16 lists or quotes deep
  if (/^(?:[ \t]*(?:>|[-*+][ \t]|\d{1,9}[.)][ \t])){16}/m.test(text)) return null;
  try { return lexer(text); } catch { return null; }
}

// One message that can't be shown never takes the window with it: it shows as the text it is,
// until its text changes
class Safe extends Component<{ text: string; children: ReactNode }, { text: string; failed: boolean }> {
  state = { text: this.props.text, failed: false };
  static getDerivedStateFromProps(p: { text: string }, s: { text: string }) { return p.text === s.text ? null : { text: p.text, failed: false }; }
  static getDerivedStateFromError() { return { failed: true }; }
  render() { return this.state.failed ? <p>{this.props.text}</p> : this.props.children; }
}

// While words come in, the marks they haven't closed yet (**bold**, `code`, a [link](…)) show as
// they will once they're closed, never as typed so far: on the line being written, and never in a
// code block, which is code until its fence comes. A ` with nothing after it yet waits; a ** opens
// bold only before a word and closes it only after one (a ** b stays as it is)
// ponytail: **, ` and links only; a lone *italic* or ~~strike~~ shows its marks until it closes
export function closed(text: string) {
  if ((text.match(/^ {0,3}(?:```|~~~)/gm) ?? []).length % 2) return text;
  const start = text.lastIndexOf('\n') + 1;
  let line = text.slice(start).trimEnd().replace(/\[([^\]]*)(?:\]\([^)]*)?$/, '$1');
  // a ** inside a code span still open is code, not bold
  const prose = line.replace(/`[^`]*`/g, ''), tick = prose.indexOf('`');
  const close = (mark: string) => { line = line.endsWith(mark) ? line.slice(0, -mark.length).trimEnd() : line + mark; };
  if (tick >= 0) close('`');
  const bold = tick >= 0 ? prose.slice(0, tick) : prose;
  let open = false;
  for (const { index: i } of bold.matchAll(/\*\*/g)) if (/\S/.test(bold[open ? i - 1 : i + 2] ?? ' ')) open = !open;
  if (open) close('**');
  return text.slice(0, start) + line;
}

const blocks = (ts: Token[]): ReactNode[] => ts.map((t, k) => {
  switch (t.type) {
    case 'paragraph': return <p key={k}>{inline(t.tokens!)}</p>;
    // a list item's words, when its list is tight
    case 'text': return <Fragment key={k}>{t.tokens ? inline(t.tokens) : words(t.text)}</Fragment>;
    // under the page's own headings, all at the words' size
    case 'heading': return createElement(`h${Math.min(t.depth + 2, 6)}`, { key: k }, inline(t.tokens!));
    case 'list': return createElement(t.ordered ? 'ol' : 'ul', { key: k, start: t.ordered ? t.start : undefined },
      (t.items as Tokens.ListItem[]).map((item, i) => <li key={i}>{blocks(item.tokens)}</li>));
    case 'checkbox': return <input key={k} type="checkbox" checked={t.checked} disabled />;
    case 'code': return <pre key={k} className="ss-code">{t.text}</pre>;
    case 'blockquote': return <blockquote key={k} className="msg-quote">{blocks(t.tokens!)}</blockquote>;
    case 'table': {
      const cell = (c: Tokens.TableCell, i: number) => createElement(c.header ? 'th' : 'td', { key: i, style: { textAlign: c.align ?? undefined } }, inline(c.tokens));
      return (
        <div key={k} className="md-table">
          <table>
            <thead><tr>{(t.header as Tokens.TableCell[]).map(cell)}</tr></thead>
            <tbody>{(t.rows as Tokens.TableCell[][]).map((r, i) => <tr key={i}>{r.map(cell)}</tr>)}</tbody>
          </table>
        </div>
      );
    }
    case 'hr': return <hr key={k} />;
    case 'space': case 'def': return null;
    // HTML, and anything else: as it was written
    default: return <p key={k}>{words(t.raw.trim())}</p>;
  }
});

// inLink: inside a link's words, where nothing is a link of its own (one click, one address)
const inline = (ts: Token[], inLink = false): ReactNode[] => ts.map((t, k) => {
  switch (t.type) {
    case 'strong': return <strong key={k}>{inline(t.tokens!, inLink)}</strong>;
    case 'em': return <em key={k}>{inline(t.tokens!, inLink)}</em>;
    case 'del': return <del key={k}>{inline(t.tokens!, inLink)}</del>;
    case 'codespan': return <code key={k} className="tok tok-code">{split(t.text)}</code>;
    case 'br': return <br key={k} />;
    // a to-do's box, in a list with blank lines between its items
    case 'checkbox': return <input key={k} type="checkbox" checked={t.checked} disabled />;
    case 'link': return inLink ? <Fragment key={k}>{inline(t.tokens!, true)}</Fragment> : <Link key={k} href={t.href}>{inline(t.tokens!, true)}</Link>;
    // a picture is never fetched: its words, linked (inside a link, a badge: just its words)
    case 'image': return inLink ? <Fragment key={k}>{words(t.text || t.href)}</Fragment> : <Link key={k} href={t.href}>{words(t.text || t.href)}</Link>;
    case 'text': return <Fragment key={k}>{t.tokens ? inline(t.tokens, inLink) : words(t.text)}</Fragment>;
    case 'escape': return t.text;
    // HTML, and anything else: as it was written
    default: return <Fragment key={k}>{words(t.raw)}</Fragment>;
  }
});

// A link opens in the person's browser, never in the window; main opens https only, so
// anything else stays its words. Its address shows on hover, whatever its words say
function Link({ href, children }: { href: string; children: ReactNode }) {
  if (!href.startsWith('https://')) return children;
  return (
    <a className="tok tok-url" href={href} title={href} target="_blank" rel="noreferrer"
      onClick={(e) => { if (window.app) { e.preventDefault(); window.app.openExternal(href); } }}>
      {children}
    </a>
  );
}

// references, paths and @mentions set apart as in the message box (Formatted)
const words = (s: string) => tokenize(s).map((r, i) => (r.kind ? <span key={i} className={`tok tok-${r.kind}`}>{split(r.text)}</span> : split(r.text, i)));
const split = (s: string, at = 0) => s.split(/(\s+)/).map((w, k) => (!w || /\s/.test(w) ? w : <span key={`${at}.${k}`} className="w">{w}</span>));

// Oby's words as they read in a message, in one line of plain text, where there's room for only
// how it starts (the side panel)
export function plain(text: string) {
  const ts = lex(text.slice(0, 400));
  return ts ? flat(ts).replace(/\s+/g, ' ').trim() : text;
}
// tokens' words, a space after each block
const flat = (ts: Token[]): string => ts.map((t) => (t.type === 'list' ? flat(t.items) : 'tokens' in t && t.tokens ? flat(t.tokens) : 'text' in t ? t.text : '')
  + (['paragraph', 'heading', 'list_item', 'code', 'br'].includes(t.type) ? ' ' : '')).join('');
