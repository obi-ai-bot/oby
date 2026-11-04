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
    <Sidebar open atHome={false} width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none}
      onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
  );
  return html.replaceAll('<!-- -->', '');
}

// Home as it first renders, on the mock: its heavy pieces as markers, to read what it chose to show
vi.mock('../ChatView', () => ({ default: ({ id }: { id: string }) => <div data-chat={id} />, ThreadTask: () => null, clock: () => '' }));
vi.mock('../Composer', () => ({ default: ({ home, hidden, placeholder }: { home?: boolean; hidden?: boolean; placeholder?: string }) => <div data-box={home && !hidden ? 'empty-chat' : 'other'} data-placeholder={placeholder} />, typingIn: () => false }));
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

  it('CHATFIRST-3: the panel lists Oby, then New conversation, with no usage', async () => {
    const html = await panel();
    const at = (words: string) => html.indexOf(words);
    expect(at('>Oby</span>')).toBeGreaterThan(-1);
    expect(at('>Oby</span>')).toBeLessThan(at('New conversation'));
    expect(html).not.toContain('panel-usage');
  });

  it('CHATFIRST-4: New conversation is in the panel', async () => {
    expect(await panel()).toContain('New conversation');
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

  // The left panel on obid's own shape of things, with no demo data: Sidebar and Home rendered from the stores
  type Seen = { chats?: unknown[]; tasks?: unknown[]; asks?: unknown[]; work?: unknown[]; spheres?: boolean; search?: string };
  async function seen({ chats = [], tasks = [], asks = [], work = [], spheres = false, search = '' }: Seen) {
    vi.resetModules();
    vi.stubGlobal('location', { search });
    vi.stubGlobal('window', {});
    vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
    vi.stubGlobal('document', { createElement: () => ({}) });
    vi.stubGlobal('innerWidth', 1000);
    const { replaceTasks } = await import('../tasks');
    const { replaceChats } = await import('../chats');
    replaceTasks(tasks as never, { asks: asks as never, work: work as never });
    replaceChats(chats as never);
    if (spheres) {
      const { addSphere } = await import('../spheres');
      addSphere({ name: 'Deskree', slug: 'deskree-live', invited: [] });
    }
    const { default: Sidebar } = await import('../Sidebar');
    const { default: Home } = await import('../Home');
    const none = () => {};
    const side = renderToStaticMarkup(
      <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none}
        onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
    ).replaceAll('<!-- -->', '');
    return { side, home: () => renderToStaticMarkup(<Home />).replaceAll('<!-- -->', '') };
  }
  const chat = (n: number, more = {}) => ({ id: `c${n}`, title: `Conversation ${n}`, said: [], at: n, ...more });
  const job = (n: number, conversation: string | undefined, status: string, more = {}) => ({
    id: `t${n}`, type: 'tasks', title: `Task ${n}`, brand: { viewBox: '0 0 1 1', parts: [] }, status, answer: undefined,
    job: { id: `t${n}`, conversation, status: status === 'needs' ? 'running' : status, created: 1, agent: 'claude', ask: status === 'needs' ? { kind: 'permission', headline: 'Run this?', text: '', options: ['Allow', 'Deny'] } : undefined }, ...more,
  });
  const rowOf = (html: string, title: string) => html.slice(html.indexOf(`${title}</span>`), html.indexOf('</div>', html.indexOf(`${title}</span>`)));
  const chatsIcon = (html: string) => /<button[^>]*class="[^"]*has-dot is-needs[^"]*"[^>]*aria-label="Chats"/.test(html);
  const ownRow = (html: string) => html.slice(html.indexOf('chat-own'), html.indexOf('New conversation'));

  it('CHATFIRST-13: rail_1_foot_order: Chats, then each Obysphere’s tile, Settings at the foot, and no Home, Tasks, Connectors, Automations or profile circle', async () => {
    const { side } = await seen({ spheres: true });
    const rail = side.slice(side.indexOf('class="rail"'), side.indexOf('sidebar-content'));
    const labels = [...rail.matchAll(/<button[^>]*aria-label="([^"]+)"/g)].map((m) => m[1]);
    expect(labels).toEqual(['Chats', 'Deskree', 'Settings']);
    expect(rail.indexOf('aria-label="Chats"')).toBeLessThan(rail.indexOf('rail-sep'));
    expect(rail.indexOf('rail-sep')).toBeLessThan(rail.indexOf('aria-label="Deskree"'));
  });

  it('CHATFIRST-14: rail_3_no_line_without_a_sphere', async () => {
    const { side } = await seen({});
    expect(side).not.toContain('rail-sep');
    expect([...side.slice(side.indexOf('class="rail"'), side.indexOf('sidebar-content')).matchAll(/<button[^>]*aria-label="([^"]+)"/g)].map((m) => m[1])).toEqual(['Chats', 'Settings']);
  });

  it('CHATFIRST-15: tasks_list_1_hidden_on_the_left: no Tasks icon or list, even with tasks; ?everything&chat and ?home keep them', async () => {
    const tasks = [job(1, 'c1', 'running'), job(2, 'c1', 'done')];
    const { side } = await seen({ chats: [chat(1)], tasks });
    expect(side).not.toContain('aria-label="Tasks"');
    expect(side).not.toContain('task-row');
    expect(side).not.toContain('Task 1');
    expect((await seen({ search: '?everything&chat' })).side).toContain('aria-label="Tasks"');
    expect((await seen({ tasks, search: '?home' })).side).toContain('aria-label="Home"');
  });

  it('CHATFIRST-16: chats_1_whole_list_scrolls and tasks_list_4_no_show_more: sixty conversations, every one a row, no Show more', async () => {
    const { side } = await seen({ chats: Array.from({ length: 60 }, (_, i) => chat(i + 1)) });
    expect(side.match(/convo-row/g)).toHaveLength(60);
    expect(side).toContain('Conversation 60');
    expect(side).not.toMatch(/Show \d+ more/);
  });

  it('CHATFIRST-17: chats_2_no_sphere_under_it: the Sphere and its people aren’t under the chats', async () => {
    const { side } = await seen({ chats: [chat(1)], spheres: true });
    const column = side.slice(side.indexOf('sidebar-content'));
    expect(column).toContain('Conversation 1');
    expect(column).not.toContain('Deskree');
    expect(column).not.toContain('Members');
  });

  it('CHATFIRST-18: Pinned first, as it was', async () => {
    const { side } = await seen({ chats: [chat(1), chat(2, { pinned: true })] });
    expect(side.indexOf('Conversation 2')).toBeLessThan(side.indexOf('Conversation 1'));
  });

  it('CHATFIRST-19: with none, the column says so and Oby’s row is quiet', async () => {
    const { side } = await seen({});
    expect(side).toContain('No conversations yet');
    expect(ownRow(side)).toContain('All quiet');
    expect(ownRow(side)).not.toContain('Needs you');
  });

  it('CHATFIRST-20: Archived is under the chats once something’s in it, and only then', async () => {
    expect((await seen({ chats: [chat(1)], tasks: [job(1, 'c1', 'running')] })).side).not.toContain('sidebar-archived');
    const { side } = await seen({ chats: [chat(1)], tasks: [job(1, 'c1', 'done', { job: { id: 't1', conversation: 'c1', status: 'done', archived: true, created: 1, agent: 'claude' } })] });
    expect(side).toContain('sidebar-archived');
    expect(side).toContain('Archived');
  });

  it('CHATFIRST-21: dots_1_row_when_its_task_needs_you, however far down, and no other row; dots_2_chats_icon', async () => {
    const chats = Array.from({ length: 60 }, (_, i) => chat(i + 1));
    const { side } = await seen({ chats, tasks: [job(1, 'c60', 'needs')] });
    expect(rowOf(side, 'Conversation 60')).toContain('aria-label="Needs you"');
    expect(rowOf(side, 'Conversation 59')).not.toContain('aria-label="Needs you"');
    expect(chatsIcon(side)).toBe(true);
    expect(ownRow(side)).not.toContain('Needs you');
    const quiet = await seen({ chats, tasks: [job(1, 'c60', 'running')] });
    expect(chatsIcon(quiet.side)).toBe(false);
    expect(rowOf(quiet.side, 'Conversation 60')).toContain('aria-label="Running"');
    expect(rowOf(quiet.side, 'Conversation 60')).not.toContain('aria-label="Needs you"');
  });

  it('CHATFIRST-22: a conversation asking by itself lights its row and the icon; what has no row lights Oby’s', async () => {
    const asked = await seen({ chats: [chat(1), chat(2, { needs: true })] });
    expect(rowOf(asked.side, 'Conversation 2')).toContain('aria-label="Needs you"');
    expect(chatsIcon(asked.side)).toBe(true);
    const confirm = { id: 'i1', title: 'Can I share?', ask: { kind: 'confirm', headline: 'x', text: '', options: ['Confirm', 'Cancel'] } };
    const rowless = await seen({ chats: [chat(1)], asks: [confirm] });
    expect(ownRow(rowless.side)).toContain('Needs you');
    expect(chatsIcon(rowless.side)).toBe(true);
    const inOne = await seen({ chats: [chat(1, { needs: true })], asks: [{ ...confirm, talk: 'c1' }] });
    expect(ownRow(inOne.side)).not.toContain('Needs you');
    const orphan = await seen({ chats: [chat(1)], tasks: [job(1, undefined, 'needs')] });
    expect(ownRow(orphan.side)).toContain('Needs you');
    expect(chatsIcon(orphan.side)).toBe(true);
    // GitHub's work no one's started has no row and no card: nothing to light
    const work = await seen({ chats: [chat(1)], work: [{ id: 'g1', type: 'review', who: 'Ernesto', title: '#417', repo: 'r', request: 'Review', updated: 1 }] });
    expect(chatsIcon(work.side)).toBe(false);
  });

  it('CHATFIRST-24: hello_1_no_name_invented: the empty chat asks what to work on, with no name, whatever is running', async () => {
    for (const tasks of [[], [job(1, undefined, 'running')], [job(1, undefined, 'needs')]]) {
      const html = (await seen({ tasks })).home();
      expect(html).toContain('>What do you want me to work on?</span>');
      expect(html).toContain('data-placeholder="Message Oby…"');
    }
    expect(await home('?home')).not.toContain('What do you want me to work on?');
  });
});
