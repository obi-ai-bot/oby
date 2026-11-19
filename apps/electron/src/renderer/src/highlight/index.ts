import { createElement, type ReactNode } from 'react';
import Prism from './prism';
// (each needs those before it: javascript needs clike, tsx jsx and typescript, markdown markup)
import 'prismjs/components/prism-markup';
import 'prismjs/components/prism-css';
import 'prismjs/components/prism-clike';
import 'prismjs/components/prism-javascript';
import 'prismjs/components/prism-typescript';
import 'prismjs/components/prism-jsx';
import 'prismjs/components/prism-tsx';
import 'prismjs/components/prism-json';
import 'prismjs/components/prism-bash';
import 'prismjs/components/prism-python';
import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-go';
import 'prismjs/components/prism-swift';
import 'prismjs/components/prism-yaml';
import 'prismjs/components/prism-toml';
import 'prismjs/components/prism-diff';
import 'prismjs/components/prism-sql';
import 'prismjs/components/prism-markdown';

// A code block's words in colour (docs/contracts/chat-first.md, CHATFIRST-38): Prism, in the page,
// offline, read into elements (never HTML: the code is anyone's words). A language it doesn't know,
// none named, or a block past 20,000 characters shows as the text it is.
const MAX = 20_000;
// what a fence names, to Prism's name
const ALIAS: Record<string, string> = {
  ts: 'typescript', mts: 'typescript', cts: 'typescript', js: 'javascript', mjs: 'javascript', cjs: 'javascript',
  sh: 'bash', shell: 'bash', zsh: 'bash', console: 'bash', shellsession: 'bash', py: 'python', rs: 'rust', golang: 'go',
  yml: 'yaml', html: 'markup', xml: 'markup', svg: 'markup', md: 'markdown', jsonc: 'json', json5: 'json', patch: 'diff',
};
export const languageOf = (lang?: string) => {
  const name = (lang ?? '').trim().split(/\s+/)[0].toLowerCase();
  const id = ALIAS[name] ?? name;
  return id && Object.hasOwn(Prism.languages, id) && typeof Prism.languages[id] === 'object' ? id : null;
};

export function highlight(code: string, lang?: string): ReactNode {
  const id = languageOf(lang);
  if (!id || code.length > MAX) return code;
  try { return tokens(Prism.tokenize(code, Prism.languages[id])); } catch { return code; }
}

type Stream = string | Prism.Token | (string | Prism.Token)[];
const tokens = (s: Stream, key = 0): ReactNode => {
  if (typeof s === 'string') return s;
  if (Array.isArray(s)) return s.map((t, i) => (typeof t === 'string' ? t : tokens(t, i)));
  const alias = Array.isArray(s.alias) ? s.alias : s.alias ? [s.alias] : [];
  return createElement('span', { key, className: ['hl', `hl-${s.type}`, ...alias.map((a) => `hl-${a}`)].join(' ') }, tokens(s.content as Stream));
};
