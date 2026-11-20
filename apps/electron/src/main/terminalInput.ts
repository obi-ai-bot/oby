import { WITHIN_MS } from '../shared/words';

// What the page may type into the owner's shell (docs/contracts/terminal.md, TERMINAL-5): only what a
// real key, paste or mouse input main saw reach the window itself makes (index.ts: before-input-event,
// before-mouse-event), never what the page says alone. A script that took the page over could
// otherwise run any command as the owner in an open terminal. Each key lets one write through, and
// only the bytes that key makes: its own character, its control character, its escape sequence, Enter
// a carriage return. A paste (⌘V) lets the clipboard's own text through, as main reads it. The
// terminal's answers to a program's questions (where the cursor is, which terminal this is, its
// colors) and mouse reports go without a key: they are fixed shapes that carry no command.

export type SeenKey = { key: string; control: boolean; alt: boolean; meta: boolean; shift: boolean; composing: boolean; at: number };
let keys: SeenKey[] = [];
let mouseAt = -Infinity;

// a real keyDown main saw reach the window (auto-repeat too: a held key types again)
export function sawKey(k: Omit<SeenKey, 'at'>, at = Date.now()) {
  keys = [...keys.filter((s) => at - s.at < WITHIN_MS), { ...k, at }].slice(-64);
}
// a real mouse press, release, move or wheel main saw: mouse reports may follow for 2 s
export function sawMouse(at = Date.now()) { mouseAt = at; }
// forgets everything seen (tests)
export function forgetInput() { keys = []; mouseAt = -Infinity; }

// The terminal's own answers to a program's questions (xterm.js answers them for the page): device
// attributes, status and cursor reports, a mode's state, the window's size, focus in and out, and its
// colors. Digits, `;`, `?`, `>`, `$` and a color's hex only: no text a shell would run.
const REPORTS = [
  /^\x1b\[[?>=]?[\d;]*c$/,               // device attributes (DA1, DA2, DA3)
  /^\x1b\[\??\d+(;\d+)*[nR]$/,           // status and cursor position reports
  /^\x1b\[\??\d+;\d+\$y$/,               // a mode's state (DECRPM)
  /^\x1b\[\d+(;\d+){0,2}t$/,             // the window's size
  /^\x1b\[[IO]$/,                         // focus in, focus out
  /^\x1b\](?:1[0-9]|4;\d{1,3});rgb:[0-9a-fA-F]{1,4}\/[0-9a-fA-F]{1,4}\/[0-9a-fA-F]{1,4}(?:\x07|\x1b\\)$/,  // a color (OSC 4, 10 to 19)
  /^\x1bP[01]\$r[\d;]*[ -~]{0,2}\x1b\\$/, // a setting's state (DECRQSS), digits only
];
export const isReport = (data: string) => REPORTS.some((r) => r.test(data));
// a mouse report: SGR (`ESC [ < b ; x ; y M|m`) or X10 (`ESC [ M` and three bytes past space)
const isMouseReport = (data: string) => /^\x1b\[<\d+;\d+;\d+[Mm]$/.test(data) || /^\x1b\[M[\x20-\xff]{3}$/.test(data);

// the bytes a named key makes: an escape sequence of digits, `;` and one final letter or `~`, ESC
// alone, DEL, backspace or a tab. Never a carriage return or text.
const NAMED = /^(?:\x1b(?:\[[\d;]*[A-Za-z~]|O[A-Za-z]|\x7f|\x08)?|\x7f|\x08|\t)$/;
const CONTROL = /[\x00-\x1f\x7f]/;
const chars = (s: string) => [...s];

// whether `data` is what key `k` makes, as xterm.js makes it
export function fits(k: SeenKey, data: string): boolean {
  // a word an input method composed (Chinese, Japanese, a dead key's accent), committed by a key:
  // text alone, no control character
  if (k.composing || k.key === 'Process' || k.key === 'Dead' || k.key === 'Unidentified') return data.length > 0 && data.length <= 256 && !CONTROL.test(data);
  if (k.key === 'Enter') return data === '\r' || data === '\x1b\r';
  if (chars(k.key).length === 1) {
    // ⌘ and a key types nothing (⌘V is the paste, below)
    if (k.meta) return false;
    // ⌃ and a key: its one control character; a carriage return or line feed only from ⌃M or ⌃J
    if (k.control) return data.length === 1 && CONTROL.test(data) && (data !== '\r' || k.key.toLowerCase() === 'm') && (data !== '\n' || k.key.toLowerCase() === 'j');
    return data === k.key;
  }
  return NAMED.test(data);
}

// a paste of `text` as xterm.js sends it: its line ends as carriage returns, bracketed or not
const pasted = (text: string) => { const t = text.replace(/\r?\n/g, '\r'); return [t, `\x1b[200~${t}\x1b[201~`]; };
const isPasteKey = (k: SeenKey) => k.key.toLowerCase() === 'v' && ((k.meta && !k.control) || (k.control && k.shift));

// Whether the page may write `data` to a shell now; a key it uses is used up, judged as the write
// arrives. clipboard: the clipboard's text as main reads it, asked only for a paste, which answers
// once it's read (a paste's ⌘V is used up either way)
export function mayWrite(data: string, clipboard: () => string | Promise<string>, at = Date.now()): boolean | Promise<boolean> {
  if (typeof data !== 'string' || !data.length) return false;
  if (isReport(data)) return true;
  if (isMouseReport(data)) return at - mouseAt < WITHIN_MS;
  keys = keys.filter((s) => at - s.at < WITHIN_MS);
  const i = keys.findIndex((k) => !isPasteKey(k) && fits(k, data));
  if (i >= 0) { keys.splice(i, 1); return true; }
  const paste = keys.findIndex(isPasteKey);
  if (paste < 0) return false;
  keys.splice(paste, 1);
  return (async () => { try { return pasted(await clipboard()).includes(data); } catch { return false; } })();
}
