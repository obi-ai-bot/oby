// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// The message box in Sangam's whole design (?everything), as his boards draw it (board 2): a send
// arrow at the end, dim and disabled till there's something to send, and no voice button: the window
// has none (AGENTS.md, "Sangam's design leads"). Every other composer test runs v1, where send was
// always shown; this one runs the design build, where a "Talk to Oby" button stood in send's place.

vi.mock('../lean', async (original) => ({ ...await original<typeof import('../lean')>(), everything: true, onObid: false }));
vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
// the box's words: a plain field holding them, Enter sending as the real editor's does
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, onEnter, handle }: { label: string; onText: (t: string) => void; onEnter: () => void; handle: React.Ref<unknown> }) {
    const field = useRef<HTMLTextAreaElement>(null);
    useImperativeHandle(handle, () => ({
      text: () => field.current?.value ?? '', clear: () => { if (field.current) field.current.value = ''; },
      setText: (t: string) => { if (field.current) field.current.value = t; }, focus: () => {}, blur: () => {}, mention: () => {},
    }));
    return <textarea ref={field} aria-label={label} data-box="" onChange={(e) => onText(e.currentTarget.value)}
      onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onEnter(); } }} />;
  },
}));

let root: Root | null = null;
vi.setConfig({ testTimeout: 120_000 });
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
});

type Props = Partial<Parameters<typeof import('../Composer').default>[0]>;
// Home's box, as Home renders it
async function homeBox(props: Props = {}) {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { default: Composer } = await import('../Composer');
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  const render = (p: Props) => act(async () => root!.render(<Composer home label="Message Oby" repos talk draft="home" {...props} {...p} />));
  await render({});
  return { render };
}
const send = () => document.querySelector<HTMLButtonElement>('form.composer button[aria-label="Send"]');
const type = (words: string) => act(async () => {
  const field = document.querySelector<HTMLTextAreaElement>('form.composer [data-box]')!;
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field, words);
  field.dispatchEvent(new Event('input', { bubbles: true }));
});

describe('the design build’s message box: send, never a microphone (board 2)', () => {
  it('composer_send_no_voice_button_and_send_is_there_dim_till_there_are_words', async () => {
    const onSend = vi.fn();
    await homeBox({ onSend });
    // no voice button, in any state
    expect(document.querySelector('[aria-label="Talk to Oby"]')).toBeNull();
    expect(document.querySelector('.composer-voice')).toBeNull();
    // empty: send is there, disabled, and a person (and a screen reader) can see it
    expect(send()).not.toBeNull();
    expect(send()!.disabled).toBe(true);
    expect(send()!.getAttribute('aria-hidden')).toBeNull();
    expect(send()!.classList.contains('is-always')).toBe(true);
    // words typed: send is lit and enabled, and Enter sends them
    await type('Tidy the notes');
    expect(send()!.disabled).toBe(false);
    expect(send()!.classList.contains('is-ready')).toBe(true);
    expect(document.querySelector('[aria-label="Talk to Oby"]')).toBeNull();
    await act(async () => { document.querySelector('form.composer [data-box]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); });
    await vi.waitFor(() => expect(onSend).toHaveBeenCalledTimes(1));
    expect(onSend.mock.calls[0][0]).toBe('Tidy the notes');
  });

  it('composer_send_stop_still_takes_sends_place_while_oby_answers', async () => {
    const onStop = vi.fn();
    const { render } = await homeBox({ onStop });
    // answering, nothing to send: Stop where send is, and no voice button beside it
    expect(send()).toBeNull();
    const stop = document.querySelector<HTMLButtonElement>('form.composer button[aria-label="Stop"]')!;
    expect(stop).not.toBeNull();
    expect(document.querySelector('[aria-label="Talk to Oby"]')).toBeNull();
    await act(async () => stop.click());
    expect(onStop).toHaveBeenCalledTimes(1);
    // words typed meanwhile: send again, to queue them
    await render({ onStop: undefined });
    await type('and the footer');
    expect(send()!.disabled).toBe(false);
  });
});
