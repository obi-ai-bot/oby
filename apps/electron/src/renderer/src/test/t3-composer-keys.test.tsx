// @vitest-environment happy-dom
import { act, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MessageEditor, { type EditorHandle, type MentionKey } from '../MessageEditor';

// T3 Code's composer fixes, on Oby's real message box (Lexical, MessageEditor.tsx) in a page: Enter
// sends and Shift+Enter starts a line (Composer.tsx's header, COMPOSER), a word an input method is
// composing keeps its keys (docs/contracts/shortcuts.md, "How a key is heard"; T3 e096deebf6,
// 91a76cf5a6, 4a2f8b04bb), a pasted screenshot is attached and types nothing, and pasted text keeps
// its blank lines (T3 4e6924753c, ec01233fb6).

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  document.body.innerHTML = '';
  vi.unstubAllGlobals();
});

async function box(more: { onMentionKey?: (key: MentionKey) => boolean; onPasteFiles?: (files: File[]) => void } = {}) {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const handle = createRef<EditorHandle>();
  const onEnter = vi.fn();
  const host = document.body.appendChild(document.createElement('div'));
  root = createRoot(host);
  await act(async () => root!.render(
    <MessageEditor label="Reply to Oby" placeholder="" onText={() => {}} onEnter={onEnter} handle={handle} {...more} />,
  ));
  const input = host.querySelector<HTMLElement>('.composer-input')!;
  await act(async () => { input.focus(); handle.current!.focus(); });
  const key = async (init: KeyboardEventInit) => {
    const e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
    await act(async () => { input.dispatchEvent(e); });
    return e;
  };
  const type = async (text: string) => act(async () => handle.current!.setText(text));
  return { handle, input, onEnter, key, type, text: () => handle.current!.text() };
}

// a paste as Chromium sends it: the clipboard's text and its files
async function paste(input: HTMLElement, { text = '', files = [] as File[] }) {
  const data = {
    files, types: [...(text ? ['text/plain'] : []), ...(files.length ? ['Files'] : [])],
    getData: (type: string) => (type === 'text/plain' ? text : ''),
  };
  const e = new ClipboardEvent('paste', { bubbles: true, cancelable: true });
  Object.defineProperty(e, 'clipboardData', { value: data });
  await act(async () => { input.dispatchEvent(e); });
  return e;
}

describe('Enter and Shift+Enter (T3: send vs newline)', () => {
  it('t3_composer_enter_sends_once_and_shift_enter_never_sends', async () => {
    const b = await box();
    await b.type('fix the login test');
    await b.key({ key: 'Enter', shiftKey: true });
    expect(b.onEnter).not.toHaveBeenCalled();
    // a new line, in the box (Lexical's own line break)
    expect(b.text()).toBe('fix the login test\n');
    const enter = await b.key({ key: 'Enter' });
    expect(enter.defaultPrevented).toBe(true);
    expect(b.onEnter).toHaveBeenCalledTimes(1);
  });

  it('t3_composer_enter_with_the_at_list_open_picks_and_does_not_send', async () => {
    const picks: MentionKey[] = [];
    const b = await box({ onMentionKey: (k) => { picks.push(k); return true; } });
    await b.type('ask @Well');
    await b.key({ key: 'Enter' });
    expect(picks).toEqual(['pick']);
    expect(b.onEnter).not.toHaveBeenCalled();
  });
});

describe('a word an input method is composing (T3: IME)', () => {
  it('t3_ime_the_enter_that_confirms_a_candidate_never_sends', async () => {
    const b = await box();
    await b.type('こんにちは');
    // Chromium's Enter that picks the IME's candidate: isComposing, keyCode 229
    await act(async () => { b.input.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' })); });
    await b.key({ key: 'Enter', isComposing: true, keyCode: 229 } as KeyboardEventInit);
    await act(async () => { b.input.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: 'こんにちは' })); });
    expect(b.onEnter).not.toHaveBeenCalled();
    // and with no compositionstart seen (focus came mid-word): the event's own flag still holds it
    await b.key({ key: 'Enter', isComposing: true } as KeyboardEventInit);
    expect(b.onEnter).not.toHaveBeenCalled();
    // the word done, Enter sends
    await b.key({ key: 'Enter' });
    expect(b.onEnter).toHaveBeenCalledTimes(1);
  });

  it('t3_ime_escape_and_arrows_while_composing_are_the_input_methods_not_the_at_list', async () => {
    const keys: MentionKey[] = [];
    const b = await box({ onMentionKey: (k) => { keys.push(k); return true; } });
    await b.type('@わた');
    for (const key of ['Escape', 'ArrowDown', 'ArrowUp', 'Tab']) await b.key({ key, isComposing: true } as KeyboardEventInit);
    expect(keys).toEqual([]);
    await b.key({ key: 'Escape' });
    expect(keys).toEqual(['close']);
  });
});

describe('paste (T3: pasted images and text)', () => {
  it('t3_paste_a_screenshot_is_attached_and_types_nothing', async () => {
    const got: File[][] = [];
    const b = await box({ onPasteFiles: (f) => got.push(f) });
    await b.type('look at this ');
    const shot = new File([new Uint8Array([137, 80, 78, 71])], 'image.png', { type: 'image/png' });
    const e = await paste(b.input, { files: [shot] });
    expect(e.defaultPrevented).toBe(true);
    expect(got).toEqual([[shot]]);
    expect(b.text()).toBe('look at this ');
  });

  it('t3_paste_crlf_text_keeps_its_blank_lines', async () => {
    const b = await box({ onPasteFiles: () => {} });
    await paste(b.input, { text: 'first line\r\n\r\nthird line\r\nfourth' });
    expect(b.text()).toBe('first line\n\nthird line\nfourth');
  });
});
