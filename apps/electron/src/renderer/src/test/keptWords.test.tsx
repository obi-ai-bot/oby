// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// ATTACH-20 (docs/contracts/attachments.md): words to a task, with a file or not, stay in the
// message box until obid takes them; refused (a file to a task at work is ATTACH-13's 422), they
// and their file stay, and obid's words say why (live.tellJob's say). The real Composer, its
// editor a plain field holding the words, as projects.test.tsx stands it in.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, handle }: { label: string; onText: (t: string) => void; handle: React.Ref<unknown> }) {
    const field = useRef<HTMLTextAreaElement>(null);
    useImperativeHandle(handle, () => ({
      text: () => field.current?.value ?? '', clear: () => { if (field.current) field.current.value = ''; },
      setText: (t: string) => { if (field.current) field.current.value = t; }, focus: () => {}, blur: () => {}, mention: () => {},
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
  vi.unstubAllGlobals();
});

async function box(onSend: (text: string) => unknown) {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { default: Composer } = await import('../Composer');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Composer label="Tell the agent about Fix it" onSend={(text) => onSend(text)} />));
  return host.querySelector('form')!;
}
const field = () => host.querySelector<HTMLTextAreaElement>('[data-box]')!;
async function send(form: Element, words: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field(), words);
    field().dispatchEvent(new Event('input', { bubbles: true }));
  });
  await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
}

describe('ATTACH-20', () => {
  it('ATTACH-20: words obid refused stay in the box, and go again from there', async () => {
    let answer!: (taken: boolean) => void;
    const onSend = vi.fn(() => new Promise<boolean>((done) => { answer = done; }));
    const form = await box(onSend);
    await send(form, 'look at this file');
    expect(onSend).toHaveBeenCalledTimes(1);
    // on their way: the box still holds them, and a second press sends nothing more
    expect(field().value).toBe('look at this file');
    await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
    expect(onSend).toHaveBeenCalledTimes(1);
    await act(async () => answer(false));
    expect(field().value).toBe('look at this file');
    // sent again and taken: the box lets them go
    await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
    expect(onSend).toHaveBeenCalledTimes(2);
    await act(async () => answer(true));
    expect(field().value).toBe('');
  });

  it('ATTACH-20: words to Oby (no promise) leave the box at once, as before', async () => {
    const onSend = vi.fn(() => 'c1');
    const form = await box(onSend);
    await send(form, 'hello');
    expect(onSend).toHaveBeenCalledTimes(1);
    expect(field().value).toBe('');
  });
});
