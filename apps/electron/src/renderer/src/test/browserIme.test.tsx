// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BrowserProvider } from '../browser/store';
import { BrowserAsks } from '../browser/Asks';
import { PasswordCard } from '../browser/cards';

// #2642: the Enter that confirms an input-method word is the typist's (`composing`, shortcuts.ts), in
// the browser's password and code cards as in every field of the window (T3 Code 4a2f8b04bb)
describe('the browser cards\' Enter while a word is being composed (T3: IME Enter)', () => {
  let root: Root, host: HTMLDivElement;
  beforeEach(() => { vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true); host = document.createElement('div'); document.body.append(host); root = createRoot(host); });
  afterEach(() => { act(() => root.unmount()); host.remove(); });
  const type = (input: HTMLInputElement, value: string) => act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  const enter = (input: HTMLInputElement, init: KeyboardEventInit = {}) => act(() => { input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...init })); });
  const composingEnter = { isComposing: true, keyCode: 229 } as KeyboardEventInit;

  it('t3_ime_the_enter_that_confirms_a_word_in_the_password_card_fills_nothing', () => {
    const onFill = vi.fn();
    act(() => root.render(<PasswordCard agent="codex" host="staging.acme.dev" onFill={onFill} onCancel={() => {}} />));
    const input = host.querySelector('[data-browser="secret-input"]') as HTMLInputElement;
    type(input, 'hunter22');
    enter(input, composingEnter);
    expect(onFill).not.toHaveBeenCalled();
    enter(input);
    expect(onFill).toHaveBeenCalledWith('hunter22');
  });

  it('t3_ime_the_enter_that_confirms_a_word_in_the_code_card_fills_nothing', () => {
    act(() => root.render(<BrowserProvider board={9}><BrowserAsks /></BrowserProvider>));
    const next = () => act(() => (host.querySelector('[aria-label="Next decision"]') as HTMLButtonElement).click());
    next();
    next();
    expect(host.querySelector('.tc-ask-kind')?.textContent).toBe('Code');
    const input = () => host.querySelector('[data-browser="code-input"]') as HTMLInputElement | null;
    type(input()!, '123456');
    enter(input()!, composingEnter);
    expect(input()?.value).toBe('123456');
    enter(input()!);
    expect(input()).toBeNull();
  });
});
