// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';

// The message box the T3 Code way (docs/contracts/composer.md): its chips, the words waiting over it,
// Stop's "Stopping…", files dropped, @'s files and /'s commands. The real Composer on live.ts, main's
// bridge (window.app.obid.call) stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
// the box's words: a plain field holding them, as the real editor's handle gives them, telling of an
// @ being typed as the real one does
const editor = vi.hoisted(() => ({ mention: [] as string[], set: [] as string[], key: null as null | ((k: 'up' | 'down' | 'pick' | 'close') => boolean) }));
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, onMention, onMentionKey, handle }: { label: string; onText: (t: string) => void; onMention?: (q: string | null) => void; onMentionKey?: (k: 'up' | 'down' | 'pick' | 'close') => boolean; handle: React.Ref<unknown> }) {
    const field = useRef<HTMLTextAreaElement>(null);
    editor.key = onMentionKey ?? null;
    useImperativeHandle(handle, () => ({
      text: () => field.current?.value ?? '', clear: () => { if (field.current) field.current.value = ''; },
      setText: (t: string) => { editor.set.push(t); if (field.current) field.current.value = t; }, focus: () => {}, blur: () => {},
      mention: (name: string) => { editor.mention.push(name); },
    }));
    return <textarea ref={field} aria-label={label} data-box="" onChange={(e) => { const v = e.currentTarget.value; onText(v); onMention?.(/(?:^|\s)@([\w./-]*)$/.exec(v)?.[1] ?? null); }} />;
  },
}));

let root: Root | null = null;
// the app's first import is a cold transform a busy Mac can stretch past a test's own time (#1839):
// done here, under the hook's time, and each test gets room for the rest
vi.setConfig({ testTimeout: 120_000 });
beforeAll(async () => { await import('../Composer'); await import('../live'); }, 300_000);
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  vi.useRealTimers();
  document.body.innerHTML = '';
  localStorage.clear();
  editor.mention = [];
  editor.set = [];
});

type Call = { method: string; path: string; body?: unknown };
const at = '2026-11-17T10:00:00Z';
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities });
const convo = { id: 'c1', title: 'Composer', pinned: false, created: at, lastActivity: at, cursor: '', folders: ['/Users/me/oby'] };

function obid(files: { folder: string; path: string }[] = []) {
  const calls: Call[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'GET' && path.startsWith('/v1/conversations/c1/files')) return { status: 200, text: JSON.stringify({ files }) };
    if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  return { app: { obid: { call, ...bridge.obid }, onFullScreen: () => () => {} }, calls, bridge };
}

type Props = Partial<Parameters<typeof import('../Composer').default>[0]>;
async function box(props: Props = {}, capabilities = ['composer-v1', 'folders-v1']) {
  const o = obid(props.conversation ? [{ folder: '/Users/me/oby', path: 'src/app.ts' }] : []);
  (window as { app?: unknown }).app = o.app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const { default: Composer } = await import('../Composer');
  const live = await import('../live');
  live.connect();
  o.bridge.sync({ status: STATUS(capabilities), conversations: [convo] });
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  const render = (p: Props) => act(async () => root!.render(<Composer label="Reply to Oby" talk draft="conversation:c1" {...props} {...p} />));
  await render({});
  return { o, render };
}
const button = (label: RegExp | string) => [...document.querySelectorAll('button')].find((b) => (typeof label === 'string' ? b.getAttribute('aria-label') === label || b.textContent === label : label.test(b.getAttribute('aria-label') ?? b.textContent ?? '')))!;
const click = (b: Element) => act(async () => { b.dispatchEvent(new MouseEvent('click', { bubbles: true })); });
const type = (text: string) => act(async () => {
  const field = document.querySelector<HTMLTextAreaElement>('[data-box]')!;
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field, text);
  field.dispatchEvent(new Event('input', { bubbles: true }));
});

describe('the chips', () => {
  it('composer_1_the_mode_chip_is_the_boxs_own', async () => {
    const { o } = await box();
    const chip = button(/^Mode for tasks: Default/);
    expect(chip.className).toContain('is-default');
    await click(chip);
    await click(button('PlanPlans first, and works once you approve the plan'));
    expect(button(/^Mode for tasks: Plan/)).toBeTruthy();
    expect(JSON.parse(localStorage.getItem('oby.chips')!)['conversation:c1']).toMatchObject({ mode: 'plan' });
    // another box keeps its own; nothing went to Settings
    expect(JSON.parse(localStorage.getItem('oby.chips')!)['home']).toBeUndefined();
    expect(o.calls.filter((c) => c.method !== 'GET')).toEqual([]);
  });

  it('composer_3_the_chips_go_with_the_words', async () => {
    const onSend = vi.fn();
    localStorage.setItem('oby.chips', JSON.stringify({ 'conversation:c1': { mode: 'acceptEdits', agent: 'claude', model: 'claude-opus-5-5', effort: 'high', at: Date.now() } }));
    await box({ onSend });
    await type('Fix the login page');
    await act(async () => { document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
    // the mode alone: a tool, model and effort 0.2.23's model chip kept show nowhere, so go nowhere (COMPOSER-17)
    expect(onSend.mock.calls[0][1].defaults).toEqual({ mode: 'acceptEdits' });
    // they stay, for the next words
    expect(button(/^Mode for tasks: Accept edits/)).toBeTruthy();
  });

  it('composer_17_one_auto_the_accounts', async () => {
    await box();
    // no model chip: the mode chip, then the talk pill, whose Auto is the only one
    expect(button(/^Model for tasks/)).toBeUndefined();
    const talk = document.querySelector('.composer-talk')!;
    expect(talk.querySelectorAll('.task-chip')).toHaveLength(1);
    expect([...talk.querySelectorAll('button')].filter((b) => /Auto/.test(b.textContent ?? ''))).toHaveLength(1);
  });

  it('composer_18_the_usage_panel_is_the_account_menus_width_on_its_gauge', async () => {
    await box();
    const rects = vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (this: Element) {
      if (this.classList.contains('composer-field')) return new DOMRect(100, 600, 800, 46);
      if (this.classList.contains('composer-usage')) return new DOMRect(830, 606, 34, 34);
      return new DOMRect(0, 0, 0, 0);
    });
    await click(button(/^Usage/));
    const panel = document.querySelector<HTMLElement>('.usage-panel')!;
    // the account menu's 380, its right edge on the gauge's, 4px over it (Menu's gap), not the box's width
    expect(panel.style.width).toBe('380px');
    expect(panel.style.right).toBe(`${900 - 864}px`);
    expect(panel.style.bottom).toBe(`${646 - 606 + 4}px`);
    rects.mockRestore();
  });

  it('composer_3_an_obid_without_composer_v1_gets_no_chips', async () => {
    const onSend = vi.fn();
    localStorage.setItem('oby.chips', JSON.stringify({ 'conversation:c1': { mode: 'plan', at: Date.now() } }));
    await box({ onSend }, ['folders-v1']);
    expect(button(/^Mode for tasks/)).toBeUndefined();
    await type('Fix it');
    await act(async () => { document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
    expect(onSend.mock.calls[0][1]).not.toHaveProperty('defaults');
  });
});

describe('words that wait', () => {
  const queued = [{ id: 't2', text: 'Also the signup page', files: [] }];

  it('composer_9_cancel_drops_the_waiting_words', async () => {
    const onCancelQueued = vi.fn(async () => true);
    await box({ queued, onCancelQueued });
    expect(document.querySelector('.queued-words')?.textContent).toContain('Also the signup page');
    await click(button('Cancel “Also the signup page”'));
    expect(onCancelQueued).toHaveBeenCalledWith(queued[0]);
    expect(editor.set).toEqual([]);
  });

  it('composer_10_edit_puts_the_words_back', async () => {
    const dropped = vi.fn(async () => true);
    await box({ queued, onCancelQueued: dropped });
    await type('and the footer');
    await click(button('Edit “Also the signup page”'));
    expect(dropped).toHaveBeenCalledWith(queued[0]);
    expect(editor.set).toEqual(['Also the signup page\n\nand the footer']);
  });

  it('composer_10_words_a_turn_took_first_stay_where_they_are', async () => {
    await box({ queued, onCancelQueued: vi.fn(async () => false) });
    await click(button('Edit “Also the signup page”'));
    expect(editor.set).toEqual([]);
  });
});

describe('Stop', () => {
  it('composer_12_stop_says_stopping_until_the_turn_ends', async () => {
    const onStop = vi.fn();
    const { render } = await box({ onStop });
    await click(button('Stop'));
    expect(onStop).toHaveBeenCalledTimes(1);
    const stopping = button('Stopping…');
    expect(stopping.disabled).toBe(true);
    // the turn ended: send again
    await render({ onStop: undefined });
    expect(button('Stopping…')).toBeUndefined();
    expect(button('Send')).toBeTruthy();
  });
});

describe('drop, @ and /', () => {
  const drop = (items: { file: File; dir?: boolean }[]) => act(async () => {
    const field = document.querySelector('.composer-field')!;
    const e = new Event('drop', { bubbles: true, cancelable: true }) as Event & { dataTransfer: unknown };
    e.dataTransfer = { types: ['Files'], files: items.map((i) => i.file), items: items.map((i) => ({ kind: 'file', getAsFile: () => i.file, webkitGetAsEntry: () => ({ isDirectory: !!i.dir }) })) };
    field.dispatchEvent(e);
  });

  it('composer_13_dropped_files_are_attached', async () => {
    await box({ conversation: 'c1' });
    await drop([{ file: new File(['a'], 'notes.txt', { type: 'text/plain' }) }, { file: new File([''], 'src', { type: '' }), dir: true }]);
    expect([...document.querySelectorAll('.composer-tag-text')].map((t) => t.textContent)).toEqual(['notes.txt']);
    await drop([{ file: new File([''], 'docs'), dir: true }]);
    expect(document.querySelector('.composer-drop-note')?.textContent).toContain('Folders can’t be attached');
  });

  it('composer_14_at_lists_the_conversations_files', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { o } = await box({ conversation: 'c1' });
    await type('look at @ap');
    await act(async () => { await vi.advanceTimersByTimeAsync(200); });
    await vi.waitFor(() => expect(document.querySelector('.mention-file')).toBeTruthy());
    expect(o.calls.some((c) => c.path === '/v1/conversations/c1/files?q=ap')).toBe(true);
    await click(document.querySelector('.mention-file')!);
    expect(editor.mention).toEqual(['src/app.ts']);
  });

  it('composer_16_slash_says_what_is_missing', async () => {
    await box({ conversation: 'c1' });
    await type('/rev');
    expect(document.querySelector('.slash-panel')?.textContent).toContain('Claude Code’s own commands can’t be listed here yet');
    await type('/review this');
    expect(document.querySelector('.slash-panel')).toBeNull();
  });
});

// T3 Code's own composer tests (queuedMessageEdit, ComposerPrimaryActions, composerMenuHighlight), held to Oby's contract
describe('T3: the message box', () => {
  // queuedMessageEdit.test "keeps file-only edits when another client starts the queued run"; T3 cebdf39984, ceed427d79
  // known bug obi-ai-bot/oby#2726: it.fails until it's fixed, then the fix removes .fails
  it.fails('t3_queue_edit_brings_back_the_files_of_the_waiting_words_too (known bug #2726)', async () => {
    const queued = [{ id: 't2', text: 'Also this', files: ['shot.png'] }];
    await box({ queued, onCancelQueued: vi.fn(async () => true) });
    await click(button('Edit “Also this”'));
    expect(editor.set).toEqual(['Also this']);
    expect([...document.querySelectorAll('.composer-tag-text')].map((t) => t.textContent)).toContain('shot.png');
  });

  // known bug obi-ai-bot/oby#2726: it.fails until it's fixed, then the fix removes .fails
  it.fails('t3_queue_edit_of_files_alone_never_leaves_blank_lines_before_the_words_typed (known bug #2726)', async () => {
    const queued = [{ id: 't3', text: '', files: ['a.pdf'] }];
    await box({ queued, onCancelQueued: vi.fn(async () => true) });
    await type('and the footer');
    await click(button(/^Edit/));
    expect(editor.set.every((t) => !t.startsWith('\n'))).toBe(true);
  });

  // ComposerPrimaryActions.browser "allows retry after ten seconds and protects a late stop"; T3 cbc68c9dc1; COMPOSER-12
  it('t3_stop_stopping_gives_way_to_stop_after_ten_seconds_with_no_end', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: false, toFake: ['setTimeout', 'clearTimeout'] });
    const onStop = vi.fn();
    await box({ onStop });
    await click(button('Stop'));
    await act(async () => { await vi.advanceTimersByTimeAsync(9_999); });
    expect(button('Stopping…')?.disabled).toBe(true);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(button('Stopping…')).toBeUndefined();
    await click(button('Stop'));
    expect(onStop).toHaveBeenCalledTimes(2);
  });

  // composerMenuHighlight.test "clears the active result while async results are empty and resolves against restored results"
  it('t3_at_list_never_picks_from_the_answer_to_what_was_typed_before', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    await box({ conversation: 'c1' });
    await type('look at @ap');
    await act(async () => { await vi.advanceTimersByTimeAsync(200); });
    await vi.waitFor(() => expect(document.querySelector('.mention-file')).toBeTruthy());
    await type('look at @apz');
    // Tab before obid answered for "apz": the "ap" list is no answer to it
    await act(async () => { editor.key?.('pick'); });
    expect(editor.mention).toEqual([]);
  });
});
