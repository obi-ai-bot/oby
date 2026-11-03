import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';

// The app opens on your Oby's conversation (docs/contracts/chat-first.md). Where it lands is driven on
// the built app (scripts/test-chat-first.mjs); here, what decides it and what the side panel lists.
vi.mock('react', async (original) => ({
  ...await original<typeof import('react')>(),
  // the stores as they stand: a server render has no snapshot of its own
  useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
}));

async function flag(search: string, kept?: string) {
  vi.resetModules();
  vi.stubGlobal('location', { search });
  vi.stubGlobal('window', {});
  vi.stubGlobal('localStorage', { getItem: (k: string) => (k === 'oby.home' ? kept ?? null : null), setItem: () => {} });
  return (await import('../lean')).chatFirst;
}

// The side panel as it renders on the mock, with ?everything&chat's Deskree and its people
async function panel() {
  vi.resetModules();
  vi.stubGlobal('location', { search: '?everything&chat' });
  vi.stubGlobal('window', {});
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  const { default: Sidebar } = await import('../Sidebar');
  const none = () => {};
  const html = renderToStaticMarkup(
    <Sidebar open atHome={false} width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenOwn={none} onOpenGroup={none}
      onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
  );
  return html.replaceAll('<!-- -->', '');
}

// Home as it first renders, on the mock: its heavy pieces as markers, to read what it chose to show
vi.mock('../ChatView', () => ({ default: ({ id }: { id: string }) => <div data-chat={id} />, ThreadTask: () => null, clock: () => '' }));
vi.mock('../Composer', () => ({ default: ({ home, hidden }: { home?: boolean; hidden?: boolean }) => <div data-box={home && !hidden ? 'empty-chat' : 'other'} />, typingIn: () => false }));
vi.mock('../AskStack', () => ({ default: () => <section data-asks="" /> }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));
async function home(search: string) {
  vi.resetModules();
  vi.stubGlobal('location', { search });
  vi.stubGlobal('window', {});
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('innerWidth', 1000);
  const { default: Home } = await import('../Home');
  return renderToStaticMarkup(<Home />).replaceAll('<!-- -->', '');
}

describe('chat first', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it('CHATFIRST-1: Home opens on the latest conversation, with no way back from it and no chips', async () => {
    const html = await home('?everything&chat');
    expect(html).toMatch(/data-chat="[^"]+"/);
    expect(html).toContain('stage is-hidden');
    expect(html).not.toContain('aria-label="Back"');
    expect(html).not.toContain('class="chip"');
  });

  it('CHATFIRST-1: an old conversation with a card still open has not just moved', async () => {
    await flag('');
    const { latestChat } = await import('../chats');
    const open = { id: 'three-days', title: 'Old, its Confirm open', said: [{ who: 'oby' as const, text: '', ago: 0 }], at: 4320 };
    const talked = { id: 'five-minutes', title: 'Talked', said: [{ who: 'you' as const, text: 'x', ago: 5 }], at: 5 };
    expect(latestChat([open, talked])?.id).toBe('five-minutes');
    expect(latestChat([talked, open])?.id).toBe('five-minutes');
  });

  it('ASK-18: a question from another Oby never decides where the app opens', async () => {
    await flag('');
    const { latestChat, restingChat } = await import('../chats');
    const asked = { id: 'asked', title: 'Questions from other Obys', said: [{ who: 'oby' as const, text: 'x', ago: 0 }], at: 0, fromObys: true };
    const talked = { id: 'mine', title: 'Talked', said: [{ who: 'you' as const, text: 'x', ago: 50 }], at: 50 };
    expect(latestChat([asked, talked])?.id).toBe('mine');
    expect(latestChat([asked])).toBeUndefined();
    expect(restingChat('gone', [asked, talked])).toBe('mine');
  });

  it('CHATFIRST-2: with none, Home is the empty chat: the character and its box, no chips', async () => {
    const html = await home('');
    expect(html).not.toContain('data-chat');
    expect(html).toContain('data-oby');
    expect(html).toContain('data-box="empty-chat"');
    expect(html).not.toContain('class="chip"');
  });

  it('CHATFIRST-8: what Oby asks shows over your conversation', async () => {
    expect(await home('?everything&chat')).toMatch(/class="asks-over"><section data-asks/);
  });

  it('CHATFIRST-8: an ask with no task is on the page on Home and over a conversation; a job’s only on its own', async () => {
    vi.doMock('electron', () => ({ app: { getPath: () => '', isPackaged: false }, BrowserWindow: {}, Notification: {}, ipcMain: {}, powerMonitor: {}, shell: {} }));
    const { onPage } = await import('../../../main/notify');
    expect(onPage({}, {})).toBe(true);
    expect(onPage({ conversation: 'c1' }, {})).toBe(true);
    expect(onPage({ job: 'j1' }, {})).toBe(false);
    expect(onPage({ conversation: 'c1' }, { conversation: 'c2' })).toBe(false);
    expect(onPage({ conversation: 'c1' }, { job: 'j1' })).toBe(false);
    expect(onPage({ job: 'j1' }, { job: 'j1' })).toBe(true);
  });

  it('CHATFIRST-7: the Tasks icon’s dot shows GitHub’s work waiting on you', async () => {
    const dotted = async (work: unknown[]) => {
      vi.resetModules();
      vi.stubGlobal('location', { search: '' });
      vi.stubGlobal('window', {});
      vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
      vi.stubGlobal('document', { createElement: () => ({}) });
      const { replaceTasks } = await import('../tasks');
      replaceTasks([], { asks: [], work: work as never });
      const { default: Sidebar } = await import('../Sidebar');
      const none = () => {};
      const html = renderToStaticMarkup(
        <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenOwn={none} onOpenGroup={none}
          onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
      );
      return /<button[^>]*class="rail-button[^"]*has-dot is-needs[^"]*"[^>]*aria-label="Tasks"/.test(html);
    };
    expect(await dotted([])).toBe(false);
    expect(await dotted([{ id: 'g1', type: 'review', who: 'Ernesto', title: '#417', repo: 'r', request: 'Review', updated: 1 }])).toBe(true);
  });

  it('CHATFIRST-9: back rests where you were: your conversation, or the empty chat', async () => {
    await flag('');
    const { restingChat } = await import('../chats');
    const chats = [{ id: 'mine', title: 'Mine', said: [], at: 90 }, { id: 'newest', title: 'Newest', said: [], at: 1 }];
    expect(restingChat('mine', chats)).toBe('mine');
    expect(restingChat(null, chats)).toBeNull();
  });

  it('CHATFIRST-12: one deleted falls back to your latest, and with none left to the empty chat', async () => {
    await flag('');
    const { restingChat } = await import('../chats');
    const chats = [{ id: 'older', title: 'Older', said: [], at: 90 }, { id: 'newest', title: 'Newest', said: [], at: 1 }];
    expect(restingChat('deleted', chats)).toBe('newest');
    expect(restingChat('deleted', [])).toBeNull();
  });

  it('CHATFIRST-1: it opens on the conversation that moved last', async () => {
    await flag('');
    const { latestChat } = await import('../chats');
    const said = (ago: number) => ({ who: 'you' as const, text: 'x', ago });
    const old = { id: 'old', title: 'Old', said: [said(90)] };
    const moved = { id: 'moved', title: 'Moved', said: [said(500), said(2)] };
    const listed = { id: 'listed', title: 'Listed, none of it loaded', said: [], at: 30 };
    expect(latestChat([old, listed, moved])?.id).toBe('moved');
    expect(latestChat([old, listed])?.id).toBe('listed');
  });

  it('CHATFIRST-2: with none there is nothing to open', async () => {
    await flag('');
    const { latestChat } = await import('../chats');
    expect(latestChat([])).toBeUndefined();
  });

  it('CHATFIRST-3: the panel lists Oby, then the Obysphere and its teammates’ Obys, with no usage', async () => {
    const html = await panel();
    const at = (words: string) => html.indexOf(words);
    expect(at('>Oby</span>')).toBeGreaterThan(-1);
    expect(at('>Oby</span>')).toBeLessThan(at('>Deskree</span>'));
    expect(at('>Deskree</span>')).toBeLessThan(at('Dima’s Oby'));
    expect(at('Dima’s Oby')).toBeGreaterThan(-1);
    expect(html).not.toContain('panel-usage');
  });

  it('CHATFIRST-4: New conversation is in the panel', async () => {
    expect(await panel()).toContain('New conversation');
  });

  it('CHATFIRST-5: a teammate’s Oby opens from its row', async () => {
    expect(await panel()).toContain('aria-label="Chat with Dima’s Oby');
  });

  it('CHATFIRST-11: it is the default; ?home, the oby.home default and ?everything keep Sangam’s Home', async () => {
    expect(await flag('')).toBe(true);
    expect(await flag('?home')).toBe(false);
    expect(await flag('', '1')).toBe(false);
    expect(await flag('?everything')).toBe(false);
    expect(await flag('?everything&chat')).toBe(true);
    expect(await flag('?everything&chat&home')).toBe(false);
  });

  it('CHATFIRST-11: Sangam’s Home shows its chips and opens on no conversation', async () => {
    const html = await home('?everything');
    expect(html).not.toContain('data-chat');
    expect(html).toContain('class="chip"');
    expect(html).not.toContain('asks-over');
  });
});
