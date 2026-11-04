import { afterEach, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';

// ASK-24, through Home: what a question's card is given opens that person's thread. Home as it first
// renders on the mock, its heavy pieces as markers; what it sets is read off its setters.
const page = vi.hoisted(() => ({ chat: null as null | { onOpenMember?: (owner: string) => void }, set: [] as unknown[] }));
vi.mock('react', async (original) => {
  const real = await original<typeof import('react')>();
  return {
    ...real, useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
    useState: (first: unknown) => { const [now] = real.useState(first); return [now, (v: unknown) => { page.set.push(v); }]; },
  };
});
vi.mock('../ChatView', () => ({ default: (props: NonNullable<typeof page.chat>) => { page.chat = props; return <div data-chat="" />; }, ThreadTask: () => null, clock: () => '' }));
vi.mock('../Composer', () => ({ default: () => <div />, typingIn: () => false }));
vi.mock('../AskStack', () => ({ default: () => <section /> }));
vi.mock('../Oby', () => ({ default: () => <div /> }));
afterEach(() => { vi.unstubAllGlobals(); });

it('ask_24_homes_chat_opens_the_thread_a_card_asks_for', async () => {
  vi.stubGlobal('location', { search: '?everything&chat' });
  vi.stubGlobal('window', {});
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('innerWidth', 1000);
  const { default: Home } = await import('../Home');
  renderToStaticMarkup(<Home />);
  expect(page.chat).not.toBeNull();
  page.set = [];
  page.chat!.onOpenMember!('aaron');
  const update = page.set.find((v): v is (all: unknown[]) => { kind: string; id: string }[] => typeof v === 'function');
  expect(update!([]).at(-1)).toMatchObject({ kind: 'member', id: 'aaron' });
}, 120_000);
