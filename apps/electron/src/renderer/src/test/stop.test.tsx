import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Chat } from '../chats';

// Stop in the message box (docs/contracts/chat-first.md, CHATFIRST-33): the real ChatView on live's
// state, its Composer a marker holding what it's given; the real Composer for the button itself.
const page = vi.hoisted(() => ({ chats: [] as unknown[], box: {} as { onStop?: () => void }, real: false }));
vi.mock('react', async (original) => {
  const real = await original<typeof import('react')>();
  return { ...real, useEffect: () => {}, useLayoutEffect: () => {}, useSyncExternalStore: (_: unknown, get: () => unknown) => get() };
});
vi.mock('../Composer', async (original) => {
  const real = await original<typeof import('../Composer')>();
  return { ...real, default: (props: { onStop?: () => void; label: string }) => { page.box = props; return page.real ? real.default(props as never) : <div />; } };
});
vi.mock('../MessageEditor', () => ({ default: () => <div /> }));
vi.mock('../chats', async (original) => ({ ...await original<typeof import('../chats')>(), useChats: () => page.chats }));

const convo = { id: 'c1', title: 'Archive the old ones', pinned: false, created: '2026-11-10T12:00:00Z', lastActivity: '2026-11-10T12:00:09Z', cursor: '' };
const call = vi.fn(async () => ({ status: 200, text: '{"stopped":true}' }));
let live: typeof import('../live');
let ChatView: typeof import('../ChatView').default;
const none = () => {};
function shown(working: Record<string, string>) {
  page.chats = [live.chatOf({ ...live.liveState(), conversations: [convo], working } as never, convo) as Chat];
  return renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={none} onGone={none} onOpenTask={none} onOpenMember={none} />);
}

beforeAll(async () => {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', { app: { obid: { call, follow: () => () => {} } } });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('addEventListener', () => {});
  vi.stubGlobal('removeEventListener', () => {});
  vi.stubGlobal('innerWidth', 1000);
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: () => {}, removeEventListener: () => {} }));
  vi.stubGlobal('requestAnimationFrame', () => 0);
  live = await import('../live');
  ChatView = (await import('../ChatView')).default;
});
beforeEach(() => { call.mockClear(); page.real = false; });

describe('Stop in the message box', () => {
  it('CHATFIRST-33: while Oby answers, Stop ends the turn under way in that conversation', async () => {
    shown({ c1: 'Thinking' });
    expect(page.box.onStop).toBeTypeOf('function');
    page.box.onStop!();
    await vi.waitFor(() => expect(call).toHaveBeenCalledWith('POST', '/v1/conversation/stop', { conversation: 'c1' }));
  });

  it('CHATFIRST-33: no turn under way, no Stop', () => {
    shown({});
    expect(page.box.onStop).toBeUndefined();
  });

  it('CHATFIRST-33: the box empty, send is Stop; with words or a file in it, Send', async () => {
    page.real = true;
    const empty = shown({ c1: 'Thinking' });
    expect(empty).toContain('aria-label="Stop"');
    expect(empty).not.toContain('aria-label="Send"');
    const idle = shown({});
    expect(idle).toContain('aria-label="Send"');
    expect(idle).not.toContain('aria-label="Stop"');
    const { stopShows } = await import('../Composer');
    expect([stopShows(true, false, 0), stopShows(true, true, 0), stopShows(true, false, 1), stopShows(false, false, 0)]).toEqual([true, false, false, false]);
  });
});
