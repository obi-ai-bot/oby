import { createElement, Fragment, useMemo, type ReactNode } from 'react';
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
  return useMemo(() => blocks(lexer(streaming ? closed(text) : text)), [text, streaming]);
}

// While words come in, the marks they haven't closed yet (**bold**, `code`, a [link](…)) show as
// they will once they're closed, never as typed so far: on the line being written, and never in a
// code block, which is code until its fence comes. A mark with nothing after it yet waits
// ponytail: **, ` and links only; a lone *italic* or ~~strike~~ shows its marks until it closes
export function closed(text: string) {
  if ((text.match(/^ {0,3}(?:```|~~~)/gm) ?? []).length % 2) return text;
  const start = text.lastIndexOf('\n') + 1;
  let line = text.slice(start).trimEnd().replace(/\[([^\]]*)(?:\]\([^)]*)?$/, '$1');
  // a ** inside a code span still open is code, not bold
  const prose = line.replace(/`[^`]*`/g, ''), tick = prose.indexOf('`');
  const close = (mark: string) => { line = line.endsWith(mark) ? line.slice(0, -mark.length).trimEnd() : line + mark; };
  if (tick >= 0) close('`');
  if ((tick >= 0 ? prose.slice(0, tick) : prose).split('**').length % 2 === 0) close('**');
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

const inline = (ts: Token[]): ReactNode[] => ts.map((t, k) => {
  switch (t.type) {
    case 'strong': return <strong key={k}>{inline(t.tokens!)}</strong>;
    case 'em': return <em key={k}>{inline(t.tokens!)}</em>;
    case 'del': return <del key={k}>{inline(t.tokens!)}</del>;
    case 'codespan': return <code key={k} className="tok tok-code">{split(t.text)}</code>;
    case 'br': return <br key={k} />;
    case 'link': return <Link key={k} href={t.href}>{inline(t.tokens!)}</Link>;
    // a picture is never fetched: its words, linked
    case 'image': return <Link key={k} href={t.href}>{words(t.text || t.href)}</Link>;
    case 'text': return <Fragment key={k}>{t.tokens ? inline(t.tokens) : words(t.text)}</Fragment>;
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

// Oby's words without their Markdown marks, where there's room for one line only (the side panel)
// ponytail: bold, code, links and line marks only; a lone *italic* keeps its stars
export const plain = (text: string) => text
  .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
  .replace(/^\s*(?:#{1,6}|>|[-*+]|\d+\.)\s+/gm, '')
  .replace(/\*\*|__|`/g, '');
