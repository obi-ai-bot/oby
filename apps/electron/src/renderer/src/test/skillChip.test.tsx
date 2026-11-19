// @vitest-environment happy-dom
import { act, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { $getRoot, type LexicalEditor } from 'lexical';
import MessageEditor, { type EditorHandle } from '../MessageEditor';

// The skill's chip before the words (docs/contracts/composer.md, COMPOSER-26), on the real editor
// (Lexical) in a page: Backspace with the cursor at the very start, or in an empty box, is the chip's.

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  document.body.innerHTML = '';
});

async function box(onBackAtStart: () => boolean) {
  const handle = createRef<EditorHandle>();
  const host = document.body.appendChild(document.createElement('div'));
  root = createRoot(host);
  await act(async () => root!.render(<MessageEditor label="Reply to Oby" placeholder="" onText={() => {}} onEnter={() => {}} onBackAtStart={onBackAtStart} handle={handle} />));
  const input = host.querySelector<HTMLElement>('.composer-input')!;
  await act(async () => { input.focus(); handle.current!.focus(); });
  const editor = (input as unknown as { __lexicalEditor: LexicalEditor }).__lexicalEditor;
  const backspace = () => act(async () => { input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true })); });
  return { handle, editor, backspace };
}

describe('COMPOSER-26: Backspace at the start takes the chip out', () => {
  it('composer_26_backspace_at_the_start_of_the_real_editor_is_the_chips', async () => {
    const chip = vi.fn(() => true);
    const b = await box(chip);
    // an empty box
    await b.backspace();
    expect(chip).toHaveBeenCalledTimes(1);
    // words, the cursor at their end: Backspace is theirs
    await act(async () => b.handle.current!.setText('fix it'));
    await b.backspace();
    expect(chip).toHaveBeenCalledTimes(1);
    // the cursor at the very start: the chip's, and the words stay
    await act(async () => { b.editor.update(() => { $getRoot().selectStart(); }); });
    await b.backspace();
    expect(chip).toHaveBeenCalledTimes(2);
    expect(b.handle.current!.text()).toBe('fix it');
  });
});
