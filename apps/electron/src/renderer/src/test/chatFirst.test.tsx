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
      onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} />,
  );
  return html.replaceAll('<!-- -->', '');
}

describe('chat first', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

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

  it('CHATFIRST-3: the panel lists Oby, then the Obysphere and its teammates’ Obys, then usage', async () => {
    const html = await panel();
    const at = (words: string) => html.indexOf(words);
    expect(at('>Oby</span>')).toBeGreaterThan(-1);
    expect(at('>Oby</span>')).toBeLessThan(at('>Deskree</span>'));
    expect(at('>Deskree</span>')).toBeLessThan(at('Dima’s Oby'));
    expect(at('Dima’s Oby')).toBeLessThan(at('Accounts and how much of each is used'));
  });

  it('CHATFIRST-3: a usage row shows the account with the most room, a full one as 100, none reported as no number', async () => {
    await flag('');
    const { usageRows } = await import('../talk');
    const a = (agent: string, used?: number, more = {}) => ({ id: `${agent}${used}`, name: 'A', agent, used, ...more }) as never;
    expect(usageRows([a('claude', 40), a('claude', 12), a('codex'), a('cursor', 10, { full: true })]).map((u) => [u.agent, u.used]))
      .toEqual([['claude', 12], ['codex', null], ['cursor', 100]]);
    expect(usageRows([a('codex', 5, { signedIn: false })])).toEqual([]);
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
});
