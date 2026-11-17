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
const editor = vi.hoisted(() => ({ mention: [] as string[], set: [] as string[] }));
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, onMention, handle }: { label: string; onText: (t: string) => void; onMention?: (q: string | null) => void; handle: React.Ref<unknown> }) {
    const field = useRef<HTMLTextAreaElement>(null);
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
  commandsSeen = null;
});

type Call = { method: string; path: string; body?: unknown };
const at = '2026-11-17T10:00:00Z';
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities });
const convo = { id: 'c1', title: 'Composer', pinned: false, created: at, lastActivity: at, cursor: '', folders: ['/Users/me/oby'] };

// obid's GET /v1/commands: the owner's own Claude Code's commands, or a 404 (an older obid)
let commandsSeen: { name: string; skill: boolean }[] | null | 'none' = null;
function obid(files: { folder: string; path: string }[] = []) {
  const calls: Call[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'GET' && path === '/v1/commands' && commandsSeen !== 'none') {
      return { status: 200, text: JSON.stringify({ claude: commandsSeen && { seenAt: at, commands: commandsSeen } }) };
    }
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

  it('composer_2_the_model_chip_sets_tool_model_and_effort', async () => {
    const { o } = await box();
    await click(button(/^Model for tasks: Auto/));
    await click(button('Opus 5.5'));
    expect(button(/^Model for tasks: Claude Code Opus 5\.5/)).toBeTruthy();
    await click(button(/^Model for tasks:/));
    // (in the menu: the chip itself now reads Opus 5.5 too)
    const inMenu = [...document.querySelectorAll('.task-models button')].find((b) => b.textContent === 'Opus 5.5')!;
    await act(async () => { inMenu.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })); });
    await click(button('High'));
    expect(button(/^Model for tasks: Claude Code Opus 5\.5 · High/)).toBeTruthy();
    expect(o.calls.filter((c) => c.method === 'PATCH')).toEqual([]);
  });

  it('composer_3_the_chips_go_with_the_words', async () => {
    const onSend = vi.fn();
    localStorage.setItem('oby.chips', JSON.stringify({ 'conversation:c1': { mode: 'acceptEdits', agent: 'claude', model: 'claude-opus-5-5', effort: 'high', at: Date.now() } }));
    await box({ onSend });
    await type('Fix the login page');
    await act(async () => { document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
    expect(onSend.mock.calls[0][1].defaults).toEqual({ mode: 'acceptEdits', agent: 'claude', model: 'claude-opus-5-5', effort: 'high' });
    // they stay, for the next words
    expect(button(/^Mode for tasks: Accept edits/)).toBeTruthy();
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

  it('composer_16_slash_lists_the_agents_commands', async () => {
    commandsSeen = [{ name: 'code-review', skill: true }, { name: 'simplify', skill: true }, { name: 'compact', skill: false }, { name: 'security-review', skill: false }];
    const { o } = await box({ conversation: 'c1' });
    await type('/');
    const rows = () => [...document.querySelectorAll('.slash-panel [role="option"]')].map((r) => r.textContent);
    expect(rows()).toEqual(['/code-reviewSkill', '/simplifySkill', '/compact', '/security-review']);
    expect(o.calls.filter((c) => c.path === '/v1/commands')).toHaveLength(1);
    // what's typed narrows it: a name starting with it, then one with it after a -
    await type('/rev');
    expect(rows()).toEqual(['/code-reviewSkill', '/security-review']);
    await type('/review');
    expect(rows()).toEqual(['/code-reviewSkill', '/security-review']);
    await type('/zzz');
    expect(document.querySelector('.slash-panel')?.textContent).toBe('No command matches /zzz.');
    await type('/simplify the diff');
    expect(document.querySelector('.slash-panel')).toBeNull();
  });
  it('composer_16_slash_says_what_is_missing', async () => {
    // none seen yet
    await box({ conversation: 'c1' });
    await type('/rev');
    expect(document.querySelector('.slash-panel')?.textContent).toBe('Claude Code’s commands show here once Oby has talked through it on this Mac.');
    await type('/review this');
    expect(document.querySelector('.slash-panel')).toBeNull();
  });
  it('composer_16_an_older_obid_lists_no_command', async () => {
    commandsSeen = 'none';
    await box({ conversation: 'c1' });
    await type('/');
    expect(document.querySelector('.slash-panel')?.textContent).toBe('Claude Code’s commands show here once Oby has talked through it on this Mac.');
  });
  it('composer_17_picking_a_command_puts_it_in_the_box', async () => {
    commandsSeen = [{ name: 'simplify', skill: true }, { name: 'compact', skill: false }];
    await box({ conversation: 'c1' });
    await type('/com');
    const row = document.querySelector('.slash-panel [role="option"]')!;
    expect(row.textContent).toBe('/compact');
    await click(row);
    expect(editor.set).toEqual(['/compact ']);
  });
});
