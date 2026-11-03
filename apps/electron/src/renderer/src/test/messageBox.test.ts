import { describe, expect, it, vi } from 'vitest';
import { $createParagraphNode, $getRoot, $getSelection, $isRangeSelection, createEditor } from 'lexical';

vi.stubGlobal('location', { search: '' });
vi.stubGlobal('window', {});
const { registerTokens, tokenize, TokenNode } = await import('../textTokens');

// The message box as it spots tokens, with no window: each key goes in where the cursor is
function box() {
  const editor = createEditor({ nodes: [TokenNode], onError: (e) => { throw e; } });
  registerTokens(editor);
  const update = (fn: () => void) => editor.update(fn, { discrete: true });
  update(() => { const p = $createParagraphNode(); $getRoot().append(p); p.select(); });
  const read = <T>(fn: () => T) => { let out!: T; update(() => { out = fn(); }); return out; };
  return {
    type: (keys: string) => { for (const key of keys) update(() => { const s = $getSelection(); if ($isRangeSelection(s)) s.insertText(key); }); },
    // the cursor this many letters back from the end of the last stretch of text
    back: (letters: number) => update(() => { const n = $getRoot().getAllTextNodes().at(-1)!, at = n.getTextContentSize() - letters; n.select(at, at); }),
    text: () => read(() => $getRoot().getTextContent()),
    tokens: () => read(() => $getRoot().getAllTextNodes().filter((n) => n instanceof TokenNode).map((n) => n.getTextContent())),
  };
}

describe('the message box keeps typed text as typed', () => {
  it.each([
    'see obi-e2e/fixture-app: x',
    'open src/app.tsx-x now',
    'read src/app.tsx. then src/app.test.tsx ok',
    'ref a-b/c#12-x and a-b/c#12 done',
    'go https://a.com/x.y-z, then',
    'plain words with dash-es. and dots',
    'at @Wellington-x and `co-de` end',
  ])('%s', (typed) => {
    const b = box();
    b.type(typed);
    expect(b.text()).toBe(typed);
    // set apart just as the sent message will be
    expect(b.tokens()).toEqual(tokenize(typed).filter((r) => r.kind).map((r) => r.text));
  });

  it('keys typed between a token and the words after it stay in order', () => {
    const b = box();
    b.type('see src/app. more');
    b.back(5);
    b.type('xy');
    expect(b.text()).toBe('see src/app.xy more');
    expect(b.tokens()).toEqual(['src/app.xy']);
  });
});
