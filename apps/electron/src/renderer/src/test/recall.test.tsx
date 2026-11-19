// @vitest-environment happy-dom
import { act, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MessageEditor, { type EditorHandle } from '../MessageEditor';
import { historyOf, quoted, stepHistory } from '../recall';

// The message box's ↑ and ↓ through the owner's earlier words (COMPOSER-20 to 22) and a reply's
// words quoted into it (COMPOSER-24), on the real editor (Lexical) in a page.

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  document.body.innerHTML = '';
});

async function box(history: string[], onMentionKey?: (key: string) => boolean) {
  const handle = createRef<EditorHandle>();
  const host = document.body.appendChild(document.createElement('div'));
  root = createRoot(host);
  await act(async () => root!.render(
    <MessageEditor label="Reply to Oby" placeholder="" onText={() => {}} onEnter={() => {}} history={history} onMentionKey={onMentionKey} handle={handle} />,
  ));
  const input = host.querySelector<HTMLElement>('.composer-input')!;
  await act(async () => { input.focus(); handle.current!.focus(); });
  const press = async (key: 'ArrowUp' | 'ArrowDown', mods: KeyboardEventInit = {}) => {
    const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...mods });
    await act(async () => { input.dispatchEvent(e); });
    return e.defaultPrevented;
  };
  return { handle, press, text: () => handle.current!.text() };
}

const SAID = ['fix the invite links', 'make it a draft PR', 'and review #389'];

describe('the owner’s earlier words', () => {
  it('composer_20_up_in_an_empty_box_brings_back_earlier_words', async () => {
    const b = await box(SAID);
    expect(await b.press('ArrowUp')).toBe(true);
    expect(b.text()).toBe('and review #389');
    await b.press('ArrowUp');
    expect(b.text()).toBe('make it a draft PR');
    await b.press('ArrowUp');
    expect(b.text()).toBe('fix the invite links');
    // nothing older: the arrow is the caret's
    expect(await b.press('ArrowUp')).toBe(false);
    expect(b.text()).toBe('fix the invite links');
  });

  it('composer_21_down_goes_forward_then_back_to_an_empty_box', async () => {
    const b = await box(SAID);
    await b.press('ArrowUp');
    await b.press('ArrowUp');
    expect(b.text()).toBe('make it a draft PR');
    expect(await b.press('ArrowDown')).toBe(true);
    expect(b.text()).toBe('and review #389');
    await b.press('ArrowDown');
    expect(b.text()).toBe('');
    // an empty box's ↓ is nothing's
    expect(await b.press('ArrowDown')).toBe(false);
  });

  it('composer_22_the_owners_own_words_keep_their_arrows', async () => {
    const b = await box(SAID);
    await act(async () => b.handle.current!.setText('a thought of mine'));
    expect(await b.press('ArrowUp')).toBe(false);
    expect(b.text()).toBe('a thought of mine');
    // recalled, then changed: theirs now, so ↑ moves the caret
    await act(async () => b.handle.current!.clear());
    await b.press('ArrowUp');
    await act(async () => b.handle.current!.setText('and review #389 too'));
    expect(await b.press('ArrowUp')).toBe(false);
    expect(b.text()).toBe('and review #389 too');
    // with a modifier, never the history's
    await act(async () => b.handle.current!.clear());
    expect(await b.press('ArrowUp', { altKey: true })).toBe(false);
    expect(b.text()).toBe('');
  });

  it('composer_22_an_open_at_list_keeps_its_arrows', async () => {
    const keys: string[] = [];
    const b = await box(SAID, (key) => { keys.push(key); return true; });
    await b.press('ArrowUp');
    expect(keys).toEqual(['up']);
    expect(b.text()).toBe('');
  });

  it('composer_20_history_is_the_owners_words_once_each', () => {
    expect(historyOf(['a', '', ' a ', 'b', 'a'])).toEqual(['a', 'b', 'a']);
    expect(stepHistory('back', [], null, '')).toBeNull();
    expect(stepHistory('forward', ['a'], null, '')).toBeNull();
  });
});

describe('a reply quoted into the box', () => {
  it('composer_24_the_words_go_in_as_a_quote_above_the_caret', async () => {
    expect(quoted('first line\n\nsecond  ')).toBe('> first line\n>\n> second');
    const b = await box([]);
    await act(async () => b.handle.current!.quote('Use SQLite here'));
    expect(b.text()).toBe('> Use SQLite here\n\n');
    // with words before the caret: on a line of its own under them
    await act(async () => b.handle.current!.setText('About this'));
    await act(async () => b.handle.current!.quote('two\nlines'));
    expect(b.text()).toBe('About this\n> two\n> lines\n\n');
  });
});

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
