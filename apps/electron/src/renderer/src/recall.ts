import {
  $createLineBreakNode, $createParagraphNode, $createTextNode, $getRoot, $getSelection, $isElementNode, $isLineBreakNode, $isRangeSelection, $isTextNode, type LexicalEditor,
} from 'lexical';

// What the message box brings back and puts in besides typing (docs/contracts/composer.md):
// ↑ and ↓ through the owner's earlier words in this conversation (COMPOSER-20 to 22, T3 Code's
// composerPromptHistory), and a reply's words quoted above the caret (COMPOSER-23, 24).

// Where ↑ has got to: the earlier words shown (`index` in the history, oldest first) and their
// text as they went in, so a box edited since is the owner's own again and its arrows move the caret
export type Recalled = { index: number; text: string } | null;

// The owner's earlier words as ↑ walks them, oldest first: none empty, and the same words twice in
// a row once
export function historyOf(words: string[]): string[] {
  const out: string[] = [];
  for (const w of words.map((w) => w.trim())) if (w && out.at(-1) !== w) out.push(w);
  return out;
}

// One step: ↑ (back) from an empty box, or from words it recalled and nobody changed, to the ones
// before; ↓ (forward) only from recalled words, and past the newest to an empty box. null: not the
// history's key now (the box has the owner's own words, or there's nothing further), so the arrow
// moves the caret
export function stepHistory(dir: 'back' | 'forward', history: string[], at: Recalled, current: string): { at: Recalled; text: string } | null {
  const active = at && at.text === current && history[at.index] === at.text ? at.index : -1;
  if (dir === 'back') {
    if (active < 0 && current.trim()) return null;
    const index = active < 0 ? history.length - 1 : active - 1;
    return index >= 0 ? { at: { index, text: history[index] }, text: history[index] } : null;
  }
  if (active < 0) return null;
  const index = active + 1;
  return index < history.length ? { at: { index, text: history[index] }, text: history[index] } : { at: null, text: '' };
}

// `text` as a Markdown quote: each line after "> " (an empty one a bare ">")
export function quoted(text: string): string {
  return text.trim().split(/\r?\n/).map((line) => (line.trim() ? `> ${line.trimEnd()}` : '>')).join('\n');
}

// In the box: `text` in place of whatever was there, the caret at its end (inside editor.update)
export function $putText(text: string) {
  const root = $getRoot(), p = $createParagraphNode();
  root.clear();
  text.split('\n').forEach((line, i) => {
    if (i) p.append($createLineBreakNode());
    if (line) p.append($createTextNode(line));
  });
  root.append(p);
  p.selectEnd();
}

// In the box: `selection` quoted on lines of its own above the caret, an empty line after it, the
// caret there to write about it (inside editor.update). No caret (the box wasn't focused): at the end
export function $quoteAtCaret(selection: string) {
  const q = quoted(selection);
  if (!q) return;
  if (!$isRangeSelection($getSelection())) $getRoot().selectEnd();
  const s = $getSelection();
  if (!$isRangeSelection(s)) return;
  if (!s.isCollapsed()) s.removeText();
  const n = s.anchor.getNode();
  // at a line's start: an empty paragraph, the start of one, or right after a line break
  const prev = $isTextNode(n) ? (s.anchor.offset === 0 ? n.getPreviousSibling() : n) : $isElementNode(n) && s.anchor.offset > 0 ? n.getChildAtIndex(s.anchor.offset - 1) : null;
  const lineStart = !prev || $isLineBreakNode(prev);
  s.insertRawText(`${lineStart ? '' : '\n'}${q}\n\n`);
}

// The box's words now
export const textOf = (editor: LexicalEditor) => editor.getEditorState().read(() => $getRoot().getTextContent());
