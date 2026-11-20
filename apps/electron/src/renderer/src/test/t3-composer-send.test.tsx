// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { KEY } from '../drafts';

// T3 Code's composer fixes on Oby's real Composer, its editor a plain field holding the words (as
// keptWords.test.tsx stands it in): words typed while a send is on its way, and the box shown
// meanwhile, aren't lost when it settles (docs/contracts/drafts.md DRAFTS-1; T3 2b07556d71 "a send
// that finishes after you leave its thread no longer blanks the open thread's composer", efd683ccc3);
// a panel put away by Esc hands the focus back to its button (T3 310baf7c99, 350ecd63be); and the
// Enter that confirms an input method's word in + › Attach tasks' search picks nothing (T3 54c1f1007c,
// 50c5437add; docs/contracts/shortcuts.md "How a key is heard": a word being composed keeps its keys).

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, handle }: { label: string; onText: (t: string) => void; handle: React.Ref<unknown> }) {
    const field = useRef<HTMLTextAreaElement>(null);
    useImperativeHandle(handle, () => ({
      text: () => field.current?.value ?? '', clear: () => { if (field.current) field.current.value = ''; },
      setText: (t: string) => { if (field.current) field.current.value = t; }, focus: () => field.current?.focus(), blur: () => {}, mention: () => {},
    }));
    return <textarea ref={field} aria-label={label} data-box="" onChange={(e) => onText(e.currentTarget.value)} />;
  },
}));

let root: Root | null = null;
let host: HTMLElement;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  host?.remove();
  localStorage.clear();
  vi.unstubAllGlobals();
});

type Props = Partial<Parameters<typeof import('../Composer').default>[0]>;
async function box(props: Props) {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { default: Composer } = await import('../Composer');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  const render = (p: Props = {}) => act(async () => root!.render(<Composer label="Reply to Oby" {...props} {...p} />));
  await render();
  return { form: host.querySelector('form')!, render };
}
const field = () => host.querySelector<HTMLTextAreaElement>('[data-box]')!;
const type = (words: string) => act(async () => {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field(), words);
  field().dispatchEvent(new Event('input', { bubbles: true }));
});
const submit = (form: Element) => act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
const button = (label: string) => [...host.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === label || b.textContent === label)!;
const click = (b: Element) => act(async () => { b.dispatchEvent(new MouseEvent('click', { bubbles: true })); });
// a send obid answers later: the words are on their way until `answer`
function slowSend() {
  let answer!: (taken: boolean) => void;
  const onSend = vi.fn(() => new Promise<boolean>((done) => { answer = done; }));
  return { onSend, answer: (taken: boolean) => act(async () => answer(taken)) };
}

describe('drafts while a send is on its way (T3: drafts lost)', () => {
  // Known bug #2640: Composer.tsx's onSubmit clears the whole box once the send settles
  // (`b.clear()`), words typed after the send began included; the room's box (`say`) keeps them with
  // leftInBox, the others don't. DRAFTS-1: a box keeps its words until they're sent.
  it.skip('t3_drafts_words_typed_while_the_send_goes_stay_in_the_box (known bug #2640)', async () => {
    const s = slowSend();
    const { form } = await box({ onSend: s.onSend });
    await type('fix the login test');
    await submit(form);
    expect(s.onSend).toHaveBeenCalledTimes(1);
    // the owner goes on typing while obid takes the words
    await type('fix the login test\n\nand then the footer');
    await s.answer(true);
    expect(field().value).toBe('and then the footer');
  });

  // Known bug #2641: the conversation's box isn't keyed by its conversation (ChatView.tsx
  // renders one Composer and changes its `draft`), so a send that settles after the owner opened
  // another conversation clears that conversation's draft and its tags (Composer.tsx onSubmit:
  // `b.clear()`, `setAttached([])`). DRAFTS-1 and DRAFTS-4: another box keeps its own words and tags.
  it.skip('t3_drafts_a_send_that_settles_after_another_box_shows_leaves_that_box_alone (known bug #2641)', async () => {
    localStorage.setItem(KEY, JSON.stringify({ 'conversation:c2': { text: 'my half-written plan for c2', at: Date.now() } }));
    const s = slowSend();
    const { form, render } = await box({ onSend: s.onSend, draft: 'conversation:c1' });
    await type('fix the login test');
    await submit(form);
    // the owner opens conversation c2 while c1's words are on their way: its draft comes back
    await render({ draft: 'conversation:c2' });
    expect(field().value).toBe('my half-written plan for c2');
    await s.answer(true);
    expect(field().value).toBe('my half-written plan for c2');
    expect(JSON.parse(localStorage.getItem(KEY) ?? '{}')['conversation:c2']?.text).toBe('my half-written plan for c2');
  });

  it('t3_drafts_a_refused_send_keeps_the_words_and_another_box_keeps_its_own', async () => {
    const s = slowSend();
    const { form, render } = await box({ onSend: s.onSend, draft: 'conversation:c1' });
    await type('fix the login test');
    await submit(form);
    await s.answer(false);
    expect(field().value).toBe('fix the login test');
    // c2 shows its own (none), and c1's words come back with c1
    await render({ draft: 'conversation:c2' });
    expect(field().value).toBe('');
    await render({ draft: 'conversation:c1' });
    expect(field().value).toBe('fix the login test');
  });
});

describe('focus (T3: Esc returns focus to the opener)', () => {
  it('t3_focus_esc_on_the_plus_panel_hands_the_focus_back_to_plus', async () => {
    await box({ onSend: () => {} });
    const plus = button('Add to the message');
    await click(plus);
    const panel = host.querySelector('.add-panel')!;
    expect(panel).not.toBeNull();
    // the panel took the focus on opening (its first row)
    expect(panel.contains(document.activeElement)).toBe(true);
    await act(async () => { dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); });
    expect(host.querySelector('.add-panel')).toBeNull();
    expect(document.activeElement).toBe(plus);
  });
});

describe('+ › Attach tasks search (T3: IME Enter in a picker)', () => {
  async function searching() {
    const { replaceTasks } = await import('../tasks');
    const { AGENTS } = await import('../view');
    const job = { id: 'j1', type: 'issues', title: 'ログイン画面を直す', request: 'fix', agent: 'claude', account: 'Personal', model: 'Opus', repo: 'o/o', branch: 'b', base: 'dev', status: 'running', step: 'Working', stages: [], created: 1 };
    replaceTasks([{ id: 'j1', type: 'issues', title: job.title, brand: AGENTS.claude.brand, status: 'running', job } as never]);
    await box({ onSend: () => {} });
    await click(button('Add to the message'));
    await click(button('Attach tasks'));
    const search = host.querySelector<HTMLInputElement>('input[aria-label="Search tasks"]')!;
    expect(search).not.toBeNull();
    // the word as the input method has it so far
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(search, 'ログイン');
      search.dispatchEvent(new Event('input', { bubbles: true }));
    });
    const enter = (init: KeyboardEventInit) => act(async () => { search.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...init })); });
    const picked = () => !!host.querySelector('.composer-tags [aria-label="Remove ログイン画面を直す"]');
    return { enter, picked };
  }

  // Known bug #2642: AddPanel's search (Composer.tsx, `onKeyDown … if (e.key === 'Enter')`) has no
  // isComposing check, so the Enter that confirms an IME candidate attaches the first task and closes
  // the panel. The same handler shape is in TalkPill.tsx (Search models), the rename fields (Sidebar.tsx,
  // Home.tsx), EmailField.tsx, JoinLink.tsx, TetrixAddress.tsx and FindBar.tsx.
  it.skip('t3_ime_the_enter_that_confirms_a_word_in_the_task_search_picks_nothing (known bug #2642)', async () => {
    const s = await searching();
    await s.enter({ isComposing: true, keyCode: 229 } as KeyboardEventInit);
    expect(s.picked()).toBe(false);
    expect(host.querySelector('input[aria-label="Search tasks"]')).not.toBeNull();
  });

  it('t3_picker_enter_after_the_word_is_done_attaches_the_first_match', async () => {
    const s = await searching();
    await s.enter({});
    expect(s.picked()).toBe(true);
  });
});
