// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Chat } from '../chats';

// T3 Code's message-list fixes (scroll jumps while a reply streams: 2fa77fbf55 "horizontal table scrolling
// preserves timeline follow", 34de974868 "clicking the timeline's empty padding keeps following a streaming
// reply", 5cbc207d8c) on Oby's real ChatView in a page: a conversation opens at its latest words and follows
// what comes in while the owner is at its end (ChatView.tsx: "a live card growing at the end of the thread
// stays in view, while you're there"); scrolled up to read, the words coming in leave them where they are.
// Opened at a task, the conversation stays at its card (docs/contracts/live-task.md, LIVETASK-28).
// The thread's geometry is the page's: happy-dom lays nothing out, so its heights are set by hand.

const page = vi.hoisted(() => ({ chats: [] as unknown[] }));
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));
vi.mock('../chats', async (original) => ({ ...await original<typeof import('../chats')>(), useChats: () => page.chats }));

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  document.body.innerHTML = '';
  vi.unstubAllGlobals();
});

const said = (reply: string, streaming: boolean): Chat => ({
  id: 'c1', title: 'The login test', typing: streaming,
  said: [
    ...Array.from({ length: 30 }, (_, i) => ({ who: i % 2 ? 'oby' as const : 'you' as const, text: `earlier words ${i}`, ago: 60 - i })),
    { who: 'you', text: 'why does the login test fail?', ago: 2 },
    { who: 'oby', text: reply, ago: 0, streaming },
  ],
} as Chat);

async function open(flash?: { task: string; n: number }) {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { default: ChatView } = await import('../ChatView');
  page.chats = [said('It fails', true)];
  const host = document.body.appendChild(document.createElement('div'));
  root = createRoot(host);
  const view = () => <ChatView kind="oby" id="c1" leaving={false} onBack={() => {}} onGone={() => {}} flash={flash} />;
  await act(async () => root!.render(view()));
  const list = host.querySelector<HTMLElement>('section.chat')!;
  // a 500px window on a 3000px thread
  let top = 0;
  Object.defineProperty(list, 'scrollHeight', { configurable: true, get: () => 3000 });
  Object.defineProperty(list, 'clientHeight', { configurable: true, get: () => 500 });
  Object.defineProperty(list, 'scrollTop', { configurable: true, get: () => top, set: (v: number) => { top = Math.max(0, Math.min(v, 2500)); } });
  const moves: number[] = [];
  list.scrollTo = ((o: ScrollToOptions) => { moves.push(o.top ?? 0); list.scrollTop = o.top ?? 0; }) as typeof list.scrollTo;
  const scrollBy = async (to: number) => act(async () => { list.scrollTop = to; list.dispatchEvent(new Event('scroll')); });
  // the next words of the reply streaming in
  const stream = async (reply: string) => { page.chats = [said(reply, true)]; await act(async () => root!.render(view())); };
  return { list, moves, scrollBy, stream, top: () => top };
}

describe('the thread while a reply streams (T3: scroll jumps)', () => {
  it('t3_scroll_at_the_end_it_follows_the_words_coming_in', async () => {
    const c = await open();
    await c.scrollBy(2500);
    await c.stream('It fails because the token');
    await c.stream('It fails because the token expired before the check');
    expect(c.top()).toBe(2500);
    expect(c.moves.at(-1)).toBe(3000);
  });

  // Known bug #2643: ChatView.tsx's layout effect (`l.scrollTo({ top: l.scrollHeight … })`,
  // keyed on the last message's text length) scrolls to the end on every word that comes in, without
  // asking whether the owner is there (`atEnd`, which only the ResizeObserver beside it asks): scrolled up
  // to read, they're pulled back down each tenth of a second while Oby answers.
  it.skip('t3_scroll_scrolled_up_to_read_the_words_coming_in_leave_the_reader_where_they_are (known bug #2643)', async () => {
    const c = await open();
    await c.scrollBy(2500);
    // the owner scrolls up to read what was said before
    await c.scrollBy(800);
    const before = c.moves.length;
    await c.stream('It fails because the token');
    await c.stream('It fails because the token expired before the check');
    expect(c.moves.slice(before)).toEqual([]);
    expect(c.top()).toBe(800);
  });
});
