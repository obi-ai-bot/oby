// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Task } from '../tasks';

// Every way to a task opens the conversation that started it, at its card (docs/contracts/live-task.md,
// LIVETASK-28): a notification's click, Waiting for you's Open, a nested row, the stack of asks. Home
// mounted on the mock, its views as markers that say which conversation is open and which card it was
// opened at; what each route hands Home is called as the route would call it.
const seen = vi.hoisted(() => ({
  sidebar: {} as Record<string, (...a: string[]) => void>,
  waiting: {} as Record<string, (id: string) => void>,
  stack: [] as ((id: string) => void)[],
  open: null as null | ((place: { job?: string; conversation?: string }) => void),
  chat: {} as Record<string, (id: string) => void>,
  back: {} as Record<string, () => void>,
  followUp: {} as Record<string, (id: string) => void>,
}));
vi.mock('../ChatView', async () => {
  const { useEffect } = await import('react');
  // gone once it leaves, as the real one says after its way out
  const ChatView = ({ id, kind, leaving, flash, onGone, onBack, onOpenTask, onFollowUp }: { id: string; kind: string; leaving: boolean; flash?: { task: string }; onGone: () => void; onBack: () => void; onOpenTask?: (id: string) => void; onFollowUp?: (id: string) => void }) => {
    useEffect(() => { if (leaving) onGone(); }, [leaving, onGone]);
    if (onOpenTask) seen.chat[id] = onOpenTask;
    seen.back[id] = onBack;
    if (onFollowUp) seen.followUp[id] = onFollowUp;
    return leaving ? null : <div data-chat={id} data-kind={kind} data-at={flash?.task} />;
  };
  return { default: ChatView, clock: () => '' };
});
vi.mock('../Sidebar', () => ({ default: (props: Record<string, (...a: string[]) => void>) => { seen.sidebar = props; return null; } }));
vi.mock('../WaitingView', () => ({ default: (props: Record<string, (id: string) => void>) => { seen.waiting = props; return <div data-waiting="" />; } }));
vi.mock('../AskStack', () => ({ default: ({ onOpen }: { onOpen: (id: string) => void }) => { seen.stack.push(onOpen); return null; } }));
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));
vi.mock('../TaskView', () => ({ default: ({ id }: { id: string }) => <div data-page={id} /> }));

let root: Root, host: HTMLElement;
beforeAll(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '?everything&chat' });
  window.app = { onOpen: (fn: typeof seen.open) => { seen.open = fn; return () => {}; }, showing: () => {}, onFullScreen: () => () => {} } as never;
});
afterAll(() => { vi.unstubAllGlobals(); });

// a conversation (c-route) whose answer started r1; r2 came in from GitHub, in no conversation
const made: Record<string, Task> = {};
async function mount() {
  const { replaceTasks } = await import('../tasks');
  const { addChat } = await import('../chats');
  const { AGENTS } = await import('../view');
  const task = (id: string, title: string): Task => ({
    id, type: 'issues', title, brand: AGENTS.claude.brand, status: 'needs',
    job: { id, type: 'issues', title, request: title, agent: 'claude', account: 'Personal', model: 'Opus', repo: 'acme/oby-scratch', branch: `oby/${id}`, base: 'main', status: 'running', step: 'Working', stages: [], created: 3 },
  } as Task);
  made.r2 = task('r2', 'From GitHub');
  replaceTasks([task('r1', 'Add divide'), made.r2]);
  addChat({ id: 'c-route', title: 'Route', said: [{ who: 'you', text: 'add divide', ago: 2 }, { who: 'oby', text: 'On it.', ago: 1, jobs: ['r1'] }] });
  const { default: Home } = await import('../Home');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<Home />));
}
const top = () => [...host.querySelectorAll<HTMLElement>('[data-chat]')].at(-1);
const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 300)); });

describe('every way to a task opens its conversation at its card', () => {
  // (Home's first import is slow on a busy Mac)
  beforeEach(async () => { root?.unmount(); host?.remove(); await mount(); }, 180_000);

  it('LIVETASK-28: a notification\'s click opens the task\'s conversation at its card, never a page of its own', async () => {
    await act(async () => seen.open!({ job: 'r1', conversation: 'c-route' }));
    await settle();
    expect(top()?.dataset).toMatchObject({ chat: 'c-route', at: 'r1' });
    expect(host.querySelector('[data-page]')).toBeNull();
  });

  // the conversation views open, bottom to top
  const trail = () => [...host.querySelectorAll<HTMLElement>('[data-chat]')].map((v) => `${v.dataset.kind}:${v.dataset.chat}`);

  it('LIVETASK-22: a nested row in the side panel opens the task\'s own thread, over the conversation that started it', async () => {
    await act(async () => seen.sidebar.onOpenAgent('r1'));
    await settle();
    expect(trail()).toEqual(['oby:c-route', 'task:r1']);
    expect(host.querySelector('[data-page]')).toBeNull();
  });

  it('LIVETASK-22: a task\'s card clicked in its conversation opens its own thread over it, so back returns to the conversation', async () => {
    await act(async () => seen.sidebar.onOpenTalk('c-route'));
    await settle();
    expect(trail()).toEqual(['oby:c-route']);
    await act(async () => seen.chat['c-route']('r1'));
    await settle();
    expect(trail()).toEqual(['oby:c-route', 'task:r1']);
    // back (the title bar's arrow, or Esc): the conversation again
    await act(async () => seen.back.r1());
    await settle();
    expect(trail()).toEqual(['oby:c-route']);
  });

  it('LIVETASK-22: opening a task\'s own thread shows its conversation\'s rows once, even folded by the owner before', async () => {
    try {
      localStorage.setItem('oby.kidsShown', JSON.stringify({ 'c-route': false, other: false }));
      await act(async () => seen.sidebar.onOpenAgent('r1'));
      await settle();
      expect(JSON.parse(localStorage.getItem('oby.kidsShown')!)).toEqual({ other: false });
      localStorage.setItem('oby.kidsShown', JSON.stringify({ 'c-route': false }));
      await act(async () => seen.sidebar.onOpenTalk('c-route'));
      await settle();
      await act(async () => seen.chat['c-route']('r1'));
      await settle();
      expect(JSON.parse(localStorage.getItem('oby.kidsShown')!)).toEqual({});
    } finally {
      localStorage.removeItem('oby.kidsShown');
    }
  });

  it('LIVETASK-22: a finished task\'s follow-up, started from its own thread, opens its conversation at the follow-up\'s card', async () => {
    await act(async () => seen.sidebar.onOpenAgent('r1'));
    await settle();
    expect(trail()).toEqual(['oby:c-route', 'task:r1']);
    // obid started the follow-up in r1's conversation (here r1 stands for it)
    await act(async () => seen.followUp.r1('r1'));
    await settle();
    expect(trail()).toEqual(['oby:c-route']);
    expect(top()?.dataset).toMatchObject({ chat: 'c-route', at: 'r1' });
  });

  it('LIVETASK-22: a task deleted while its own thread shows takes that thread with it, back to its conversation', async () => {
    await act(async () => seen.sidebar.onOpenAgent('r1'));
    await settle();
    expect(trail()).toEqual(['oby:c-route', 'task:r1']);
    // deleted (its ⋯'s Delete task, or from another device): obid's list no longer has it
    const { replaceTasks } = await import('../tasks');
    await act(async () => replaceTasks([made.r2]));
    await settle();
    expect(trail()).toEqual(['oby:c-route']);
  });

  it('LIVETASK-28: Waiting for you\'s Open opens the conversation at the card, over the list so back returns to it', async () => {
    await act(async () => seen.sidebar.onOpenWaiting());
    await settle();
    await act(async () => seen.waiting.onOpenTask('r1'));
    await settle();
    expect(top()?.dataset).toMatchObject({ chat: 'c-route', at: 'r1' });
    expect(host.querySelector('[data-waiting]')).not.toBeNull();
  });

  it('LIVETASK-28: the stack of asks and a task row\'s menu open the conversation at the card too', async () => {
    // the stack is on Home's stage, with nothing open (CHATFIRST-8)
    seen.stack.length = 0;
    await act(async () => seen.sidebar.onNewChat());
    await settle();
    expect(seen.stack.length).toBeGreaterThan(0);
    await act(async () => seen.stack.at(-1)!('r1'));
    await settle();
    expect(top()?.dataset).toMatchObject({ chat: 'c-route', at: 'r1' });
    await act(async () => seen.sidebar.onOpenTask('r1'));
    await settle();
    expect(top()?.dataset).toMatchObject({ chat: 'c-route', at: 'r1' });
    expect(host.querySelector('[data-page]')).toBeNull();
  });

  it('LIVETASK-28: a task in no conversation opens its own thread, the same card and what it did', async () => {
    await act(async () => seen.open!({ job: 'r2' }));
    await settle();
    expect(top()?.dataset).toMatchObject({ chat: 'r2', kind: 'task', at: 'r2' });
  });
});
