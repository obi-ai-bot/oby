// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Task } from '../tasks';

// A task in its conversation, clicked (docs/contracts/live-task.md): opened at it, its card scrolled into
// view and lit (LIVETASK-28); its card's menu putting its chip on the conversation's box, which tells that
// job alone, and Stop (LIVETASK-30); what Later put off back once it's opened at a task (LIVETASK-26).
// The box (Composer) is a marker that keeps what it's given, by its label; what it sends to is spied on.
type Props = { placeholder: string; label: string; onSend: (text: string, att: object) => void; onStop?: () => void };
const boxes = vi.hoisted(() => ({}) as Record<string, Props>);
vi.mock('../Composer', () => ({
  default: (props: Props) => { boxes[props.label] = props; return <input aria-label={props.label} placeholder={props.placeholder} />; },
  typingIn: () => false,
}));
vi.mock('../Menu', async (original) => ({ ...await original<typeof import('../Menu')>(), Menu: ({ children }: { children: React.ReactNode }) => <div role="menu">{children}</div> }));
vi.mock('../tasks', async (original) => ({ ...await original<typeof import('../tasks')>(), tell: vi.fn(), stop: vi.fn() }));
vi.mock('../chats', async (original) => ({ ...await original<typeof import('../chats')>(), tellChat: vi.fn() }));

const scrolled = vi.fn();
let root: Root, host: HTMLElement;
beforeAll(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  Element.prototype.scrollIntoView = scrolled;
});
afterAll(() => { vi.unstubAllGlobals(); });

const ask = { kind: 'permission' as const, headline: 'Run npm test?', text: 'It wants to run the tests.', options: ['Allow', 'Deny'] };
// a conversation (c1) whose answer started j1, at work, and j2, waiting on a permission
async function seed() {
  const { replaceTasks } = await import('../tasks');
  const { addChat } = await import('../chats');
  const { AGENTS } = await import('../view');
  const { SESSIONS } = await import('../session');
  const task = (id: string, title: string, status: 'running' | 'needs', more = {}): Task => ({
    id, type: 'issues', title, brand: AGENTS.claude.brand, status,
    job: { id, type: 'issues', title, request: title, agent: 'claude', account: 'Personal', model: 'Opus', repo: 'acme/oby-scratch', branch: `oby/${id}`, base: 'main', status: 'running', step: 'Working', stages: [], created: 3, ...more },
  } as Task);
  SESSIONS.j1 = [{ kind: 'tool', tool: 'Read', verb: 'Read', target: 'a.ts', state: 'done', seconds: 1 }];
  replaceTasks([task('j1', 'Fix the login test', 'running'), task('j2', 'Run the tests', 'needs', { ask })]);
  addChat({ id: 'c1', title: 'Two fixes', said: [{ who: 'you', text: 'fix both', ago: 5 }, { who: 'oby', text: 'On it.', ago: 4, jobs: ['j1', 'j2'] }] });
}
async function show(flash?: { task: string; n: number }, onFlashed = () => {}) {
  const { default: ChatView } = await import('../ChatView');
  await act(async () => root.render(<ChatView kind="oby" id="c1" leaving={false} flash={flash} onFlashed={onFlashed} onBack={() => {}} onGone={() => {}} />));
}
const click = (el: Element | null | undefined) => act(async () => { (el as HTMLElement).click(); });
const button = (name: string) => [...host.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === name || b.textContent === name);

describe('a task in its conversation', () => {
  beforeAll(seed);
  beforeEach(() => {
    root?.unmount();
    host?.remove();
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    for (const k of Object.keys(boxes)) delete boxes[k];
    vi.clearAllMocks();
  });

  it('LIVETASK-28: opened at a task, its card is scrolled into view and lit once', async () => {
    const flashed = vi.fn();
    await show({ task: 'j2', n: 1 }, flashed);
    const at = host.querySelector('[data-task="j2"]')!;
    expect(at.className).toContain('is-lit');
    expect(host.querySelector('[data-task="j1"]')!.className).not.toContain('is-lit');
    expect(scrolled).toHaveBeenCalledTimes(1);
    expect(scrolled.mock.contexts[0]).toBe(at);
    expect(flashed).toHaveBeenCalledTimes(1);
    // drawn again with the same opening: not scrolled again
    await show({ task: 'j2', n: 1 }, flashed);
    expect(scrolled).toHaveBeenCalledTimes(1);
  });

  it('LIVETASK-22: a click on a task\'s card opens its own thread; its ⋯ only opens its menu', async () => {
    const { default: ChatView } = await import('../ChatView');
    const opened = vi.fn();
    await act(async () => root.render(<ChatView kind="oby" id="c1" leaving={false} onOpenTask={opened} onBack={() => {}} onGone={() => {}} />));
    await click(button('More for Fix the login test'));
    expect(opened).not.toHaveBeenCalled();
    // a part of the card that is no button of its own: the card's own click
    await click(host.querySelector('[data-task="j1"] .msg-task-meta'));
    expect(opened).toHaveBeenCalledTimes(1);
    expect(opened).toHaveBeenCalledWith('j1');
  });

  it('LIVETASK-22: from the keyboard, the card\'s title is a button that opens its own thread, once', async () => {
    const { default: ChatView } = await import('../ChatView');
    const opened = vi.fn();
    await act(async () => root.render(<ChatView kind="oby" id="c1" leaving={false} onOpenTask={opened} onBack={() => {}} onGone={() => {}} />));
    const open = button('Open Fix the login test')!;
    expect(open.tagName).toBe('BUTTON');
    expect(open.closest('[data-task="j1"]')).not.toBeNull();
    open.focus();
    expect(document.activeElement).toBe(open);
    // Enter or Space on a focused button is its click
    await click(open);
    expect(opened).toHaveBeenCalledTimes(1);
    expect(opened).toHaveBeenCalledWith('j1');
  });

  it('LIVETASK-22: in a task\'s own thread the agent\'s words show whole; in the conversation they fold past a few lines', async () => {
    const { SESSIONS } = await import('../session');
    const kept = SESSIONS.j1;
    const long = Array.from({ length: 30 }, (_, i) => `Line ${i + 1} of what I found.`).join('\n\n');
    SESSIONS.j1 = [{ kind: 'said', text: long, at: '10:04 AM' }];
    try {
      const { default: ChatView } = await import('../ChatView');
      const words = () => host.querySelector('[data-task="j1"] .lt-log .ss-said')!;
      // in the conversation, cut to a few lines (its Show more once laid out: a real browser's, taskLogLayout)
      await show();
      expect(words().querySelector('.lt-fold:not(.is-open)')).not.toBeNull();
      // its own thread: open from the start, all of it, nothing to fold
      await act(async () => root.render(<ChatView kind="task" id="j1" leaving={false} onBack={() => {}} onGone={() => {}} />));
      expect(words().querySelector('.lt-fold.is-open > .lt-fold-body')).not.toBeNull();
      expect(words().querySelector('.lt-log-more')).toBeNull();
      expect(words().textContent).toContain('Line 30 of what I found.');
    } finally { SESSIONS.j1 = kept; }
  });

  it('LIVETASK-22: a task\'s own thread shows all it did, not only its last lines', async () => {
    const { SESSIONS } = await import('../session');
    const kept = SESSIONS.j1;
    SESSIONS.j1 = ['a', 'b', 'c', 'd', 'e', 'f'].map((f) => ({ kind: 'tool' as const, tool: 'Read', verb: 'Read', target: `${f}.ts`, state: 'done' as const, seconds: 1 }));
    try {
      const { default: ChatView } = await import('../ChatView');
      const tools = () => host.querySelectorAll('[data-task="j1"] .lt-log .ss-tool').length;
      // in the conversation, its last 4 and Show 2 more
      await show();
      expect(tools()).toBe(4);
      expect(button('Show 2 more')).toBeDefined();
      // its own thread: every line, nothing folded
      await act(async () => root.render(<ChatView kind="task" id="j1" leaving={false} onBack={() => {}} onGone={() => {}} />));
      expect(tools()).toBe(6);
      expect(button('Show 2 more')).toBeUndefined();
    } finally { SESSIONS.j1 = kept; }
  });

  it('LIVETASK-30: the card\'s Tell Claude Code puts its chip on the box, which tells that job alone, then talks to Oby again', async () => {
    const tasks = await import('../tasks'), chats = await import('../chats');
    await show();
    expect(Object.keys(boxes)).toEqual(['Reply to Oby']);
    await click(button('More for Fix the login test'));
    await click(button('Tell Claude Code…'));
    expect(host.querySelector('.composer-to')!.textContent).toBe('To Claude Code · Fix the login test');
    const box = boxes['Tell the agent about Fix the login test'];
    expect(box.placeholder).toBe('Tell Claude Code…');
    // at work: Stop in the box
    expect(box.onStop).toBeTypeOf('function');
    await act(async () => { box.onSend('go on', {}); });
    expect(tasks.tell).toHaveBeenCalledTimes(1);
    expect(tasks.tell).toHaveBeenCalledWith('j1', 'go on', {});
    expect(chats.tellChat).not.toHaveBeenCalled();
    // sent: the box is Oby's again
    expect(host.querySelector('.composer-to')).toBeNull();
    expect(host.querySelector('input')!.getAttribute('aria-label')).toBe('Reply to Oby');
    // the chip's × goes back to Oby too, with nothing sent
    await click(button('More for Fix the login test'));
    await click(button('Tell Claude Code…'));
    await click(button('Back to Oby'));
    expect(host.querySelector('.composer-to')).toBeNull();
    expect(tasks.tell).toHaveBeenCalledTimes(1);
    // refused by obid, the chip stays with the words (ATTACH-20); taken, it goes
    await click(button('More for Fix the login test'));
    await click(button('Tell Claude Code…'));
    vi.mocked(tasks.tell).mockReturnValueOnce(Promise.resolve(false));
    await act(async () => { await boxes['Tell the agent about Fix the login test'].onSend('again', {}); });
    expect(host.querySelector('.composer-to')).not.toBeNull();
    vi.mocked(tasks.tell).mockReturnValueOnce(Promise.resolve(true));
    await act(async () => { await boxes['Tell the agent about Fix the login test'].onSend('again', {}); });
    expect(host.querySelector('.composer-to')).toBeNull();
  });

  it('LIVETASK-30: Stop task in the card\'s menu stops it', async () => {
    const tasks = await import('../tasks');
    await show();
    await click(button('More for Fix the login test'));
    await click(button('Stop task'));
    expect(tasks.stop).toHaveBeenCalledWith('j1');
  });

  it('LIVETASK-26: opened at a task, what Later put off is back over the box', async () => {
    await show();
    expect(host.querySelector('.decision-bar')).not.toBeNull();
    await act(async () => { await new Promise((r) => setTimeout(r, 600)); });
    await click(button('Later'));
    expect(host.querySelector('.decision-bar')).toBeNull();
    await show({ task: 'j2', n: 2 });
    expect(host.querySelector('.decision-bar')).not.toBeNull();
    expect(host.querySelector('.decision-bar')!.textContent).toContain('Run npm test?');
  });
});
