// @vitest-environment happy-dom
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { act, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Chat } from '../chats';
import { liveBridge } from './liveBridge';

// The window's keyboard shortcuts (docs/contracts/shortcuts.md, SHORTCUTS-1 to 11) and the side
// panel's search over every conversation (docs/contracts/chat-first.md, SEARCH-1 to 3): the real
// Home, Sidebar and find bar, with main's bridge (window.app.obid.call) stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));
vi.mock('../SettingsView', async (original) => ({ ...await original<typeof import('../SettingsView')>(), default: ({ leaving }: { leaving: boolean }) => <div data-settings={leaving ? 'leaving' : 'shown'} /> }));

beforeAll(() => { Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true }); });

let root: Root | null = null;
let host: HTMLElement;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
});

const at = '2026-11-17T10:00:00Z';
const convo = (id: string, title: string, more: object = {}) => ({ id, title, pinned: false, created: at, lastActivity: at, cursor: '', ...more });
// obid: what its search finds (by q), the conversation's turns, the rest 404
function obid(found: Record<string, unknown[]> = {}) {
  const calls: string[] = [];
  const call = vi.fn(async (method: string, path: string) => {
    calls.push(`${method} ${path}`);
    if (method === 'GET' && path.startsWith('/v1/conversations?')) {
      const q = new URLSearchParams(path.split('?')[1]).get('q');
      if (q !== null) return { status: 200, text: JSON.stringify(found[q] ?? []) };
    }
    if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  return { app: { obid: { call, ...bridge.obid }, onSphere: () => () => {}, onFullScreen: () => () => {} }, calls, bridge };
}

// each look lets React put on the page what came meanwhile (obid's answer), then checks
const until = (check: () => void) => vi.waitFor(async () => { await act(async () => {}); check(); }, { timeout: 15_000, interval: 50 });
const press = async (key: string, more: KeyboardEventInit = {}) => {
  const e = new KeyboardEvent('keydown', { key, metaKey: true, bubbles: true, cancelable: true, ...more });
  await act(async () => { dispatchEvent(e); });
  return e;
};
const type = async (input: HTMLInputElement, value: string) => {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
};
const talks = () => [...host.querySelectorAll('.convo-row .connector-name')].map((n) => n.textContent);
type Item = { label?: string; role?: string; accelerator?: string; submenu?: Item[] };
const items = (list: Item[]): Item[] => list.flatMap((i) => [i, ...(Array.isArray(i.submenu) ? items(i.submenu) : [])]);
const abouts = () => [...host.querySelectorAll('.convo-row .connector-about')].map((n) => n.textContent);

async function showHome(o: ReturnType<typeof obid>, conversations: unknown[]) {
  (window as { app?: unknown }).app = o.app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  localStorage.setItem('oby.setUp', '1');
  const { default: Home } = await import('../Home');
  const live = await import('../live');
  live.connect();
  o.bridge.sync({ conversations });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Home />));
}
async function showSidebar(o: ReturnType<typeof obid>, chats: Chat[], props: Record<string, unknown> = {}) {
  (window as { app?: unknown }).app = o.app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const { default: Sidebar } = await import('../Sidebar');
  (await import('../chats')).replaceChats(chats);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  const none = () => {};
  await act(async () => root!.render(
    <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none}
      onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} {...props} />,
  ));
}
const talk = (id: string, title: string, more: Partial<Chat> = {}): Chat => ({ id, title, said: [], at: 5, ...more });

// Home and the side panel render slowly on a busy Mac: each test may take minutes
describe('SHORTCUTS: the window answers ⌘N, ⌘K, ⌘F, ⌘,, ⌘B, ⌘1 to ⌘9, ⌘⇧[ ⌘⇧] and Esc', { timeout: 180_000 }, () => {
  it('shortcuts_1_2_4_new_conversation_search_and_settings_from_anywhere', async () => {
    const o = obid();
    await showHome(o, [convo('c1', 'Release notes')]);
    // ⌘, : Settings opens, and the page took the key, so the menu bar's item doesn't run it again
    const comma = await press(',');
    expect(comma.defaultPrevented).toBe(true);
    await until(() => expect(host.querySelector('[data-settings="shown"]')).not.toBeNull());
    // ⌘N : a new conversation, Settings gone, Home's box in front
    await press('n');
    await until(() => expect(host.querySelector('[data-settings="shown"]')).toBeNull());
    // ⌘K : the side panel opens (shut or not) on the chats' search, its field taking the keys
    await press('k');
    await until(() => expect(host.querySelector('.sidebar-search-wrap.is-open')).not.toBeNull());
    await until(() => expect(document.activeElement?.getAttribute('aria-label')).toBe('Search'));
    // with ⇧ or ⌥ it's not the shortcut, and nothing is taken
    expect((await press('n', { shiftKey: true })).defaultPrevented).toBe(false);
  });

  it('shortcuts_1_ctrl_stands_for_cmd_off_a_mac_and_an_ime_word_is_left_alone', async () => {
    const s = await import('../shortcuts');
    const key = (k: string, more: object = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...more });
    expect(s.commandOf(key('n', { metaKey: true }), true)).toBe('chat.new');
    expect(s.commandOf(key('N', { ctrlKey: true }), false)).toBe('chat.new');
    expect(s.commandOf(key('n', { ctrlKey: true }), true)).toBeNull();
    expect(s.commandOf(key('k', { metaKey: true, altKey: true }), true)).toBeNull();
    const ran = vi.fn();
    const off = s.registerCommands({ 'chat.new': ran });
    const e = new KeyboardEvent('keydown', { key: 'n', metaKey: true, isComposing: true, cancelable: true });
    dispatchEvent(e);
    expect(ran).not.toHaveBeenCalled();
    dispatchEvent(new KeyboardEvent('keydown', { key: 'n', metaKey: true, cancelable: true }));
    expect(ran).toHaveBeenCalledOnce();
    off();
  });

  it('shortcuts_3_cmd_f_finds_the_words_in_the_conversation_and_esc_closes_it', async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    const { default: FindBar } = await import('../FindBar');
    const ref = createRef<HTMLElement>();
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(
      <section ref={ref}>
        <FindBar root={ref} shown={1} />
        <div className="chat-thread"><p>The <b>retry</b> jitter.</p><p>Retry once more, then retry again.</p><p>Nothing here.</p></div>
      </section>,
    ));
    expect(host.querySelector('.find-bar')).toBeNull();
    await press('f');
    const field = host.querySelector<HTMLInputElement>('.find-field')!;
    expect(field).not.toBeNull();
    await type(field, 'retry');
    const count = () => host.querySelector('.find-count')?.textContent;
    expect(count()).toBe('1 of 3');
    await act(async () => { field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); });
    expect(count()).toBe('2 of 3');
    await act(async () => { field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true, bubbles: true })); });
    await act(async () => { field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true, bubbles: true })); });
    expect(count()).toBe('3 of 3');
    await type(field, 'nowhere');
    expect(count()).toBe('No results');
    await act(async () => { field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); });
    expect(host.querySelector('.find-bar')).toBeNull();
  });

  it('shortcuts_3_with_a_page_over_the_conversation_cmd_f_is_not_its', async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    const { default: FindBar } = await import('../FindBar');
    const ref = createRef<HTMLElement>();
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<section ref={ref}><FindBar root={ref} shown={1} off /></section>));
    expect((await press('f')).defaultPrevented).toBe(false);
    expect(host.querySelector('.find-bar')).toBeNull();
  });

  it('shortcuts_5_esc_closes_an_open_menu', async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    const { Menu } = await import('../Menu');
    const closed = vi.fn();
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<Menu at={new DOMRect(10, 10, 20, 20)} onClose={closed}><button role="menuitem">Rename</button></Menu>));
    await press('Escape', { metaKey: false });
    expect(closed).toHaveBeenCalledOnce();
  });

  it('shortcuts_6_the_menus_show_the_keys_the_window_answers', async () => {
    const { appMenu } = await import('../../../shared/menu');
    const { KEYBINDINGS, accelerator } = await import('../../../shared/shortcuts');
    const all = items(appMenu({ dev: false, help: { revealLog() {}, copyDiagnostics() {} } }) as Item[]);
    // every key the window answers is an item of the Mac's menu bar, with that very key
    for (const b of KEYBINDINGS) expect(all.find((i) => i.label === b.label)?.accelerator, b.command).toBe(accelerator(b.command));
    expect(all.filter((i) => i.accelerator).map((i) => [i.label, i.accelerator])).toEqual([
      ['Settings…', 'CmdOrCtrl+,'], ['New Conversation', 'CmdOrCtrl+N'], ['Search', 'CmdOrCtrl+K'], ['Find in Conversation', 'CmdOrCtrl+F'],
      ['Toggle Sidebar', 'CmdOrCtrl+B'], ['Previous Conversation', 'CmdOrCtrl+Shift+['], ['Next Conversation', 'CmdOrCtrl+Shift+]'],
      ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => [`Conversation ${n}`, `CmdOrCtrl+${n}`]),
    ]);
    // and the side panel's: New conversation's key, Settings' and Search's tips
    await showSidebar(obid(), [talk('c1', 'Release notes')]);
    // drawn by CSS, so the row's own text stays its label
    expect(host.querySelector('.chat-new')?.getAttribute('data-kbd')).toBe('⌘N');
    expect(host.querySelector('.chat-new')?.textContent).toBe('New conversation');
    expect(host.querySelector('.rail-settings')?.getAttribute('data-tip')).toBe('Settings ⌘,');
    expect(host.querySelector('[aria-label="Search"]')?.getAttribute('title')).toBe('Search ⌘K');
    // ⇧ in a label as the Mac writes it
    const s = await import('../shortcuts');
    expect([s.shortcutLabel('chat.next', true), s.shortcutLabel('chat.next', false), s.ariaKeys('chat.previous', true)]).toEqual(['⇧⌘]', 'Ctrl+Shift+]', 'Meta+Shift+[']);
  });

  it('shortcuts_7_cmd_b_shuts_and_opens_the_side_panel', async () => {
    await showHome(obid(), [convo('c1', 'Release notes')]);
    const shown = () => host.querySelector('.app')!.classList.contains('has-sidebar');
    const before = shown();
    const b = await press('b');
    expect(b.defaultPrevented).toBe(true);
    await until(() => expect(shown()).toBe(!before));
    await press('b');
    await until(() => expect(shown()).toBe(before));
    // ⇧⌘B isn't it
    expect((await press('b', { shiftKey: true })).defaultPrevented).toBe(false);
  });

  it('shortcuts_8_cmd_1_to_9_open_the_conversations_in_the_panels_order', async () => {
    const opened = vi.fn();
    // the panel lists the pinned one first: Backoff, Release notes, Flaky test
    await showSidebar(obid(), [talk('c1', 'Release notes'), talk('c2', 'Backoff', { pinned: true }), talk('c3', 'Flaky test')], { onOpenTalk: opened });
    expect(talks()).toEqual(['Backoff', 'Release notes', 'Flaky test']);
    expect((await press('1')).defaultPrevented).toBe(true);
    await press('3');
    await press('2');
    expect(opened.mock.calls.map((c) => c[0])).toEqual(['c2', 'c3', 'c1']);
    // past the last row, nothing; with ⇧ or ⌥, not the shortcut
    await press('9');
    await press('1', { shiftKey: true });
    await press('1', { altKey: true });
    expect(opened).toHaveBeenCalledTimes(3);
    // by project, as the Projects side groups them, a folded project's rows left out
    const { panelOrder, projectGroups, pinnedFirst } = await import('../panelOrder');
    const rows = pinnedFirst([talk('a', 'A', { project: 'p2' }), talk('b', 'B'), talk('c', 'C', { project: 'p1', pinned: true }), talk('d', 'D', { project: 'p1' })]);
    const groups = projectGroups(rows, [{ id: 'p1', name: 'One' }, { id: 'p2', name: 'Two' }], true, false);
    expect(panelOrder(rows, groups, new Set())).toEqual(['c', 'd', 'a', 'b']);
    expect(panelOrder(rows, groups, new Set(['p1']))).toEqual(['a', 'b']);
    expect(panelOrder(rows, [], new Set())).toEqual(['c', 'a', 'b', 'd']);
  });

  it('shortcuts_9_cmd_shift_brackets_go_to_the_previous_and_next_conversation', async () => {
    const opened = vi.fn();
    await showSidebar(obid(), [talk('c1', 'Release notes'), talk('c2', 'Backoff'), talk('c3', 'Flaky test')], { onOpenTalk: opened, activeTalk: 'c2' });
    // ⇧] reads as "}" on a US keyboard, as "]" with ⌘ held on some: both are the key
    expect((await press('}', { shiftKey: true })).defaultPrevented).toBe(true);
    await press('{', { shiftKey: true });
    await press(']', { shiftKey: true });
    expect(opened.mock.calls.map((c) => c[0])).toEqual(['c3', 'c1', 'c3']);
    // without ⇧, ⌘] isn't it
    expect((await press(']')).defaultPrevented).toBe(false);
    // at an end, nowhere; none open, the first going on and the last going back (T3 Code's)
    const { adjacent } = await import('../panelOrder');
    expect([adjacent(['a', 'b'], 'a', 'previous'), adjacent(['a', 'b'], 'b', 'next'), adjacent(['a', 'b'], null, 'next'), adjacent(['a', 'b'], null, 'previous'), adjacent([], null, 'next')])
      .toEqual([null, null, 'a', 'b', null]);
  });

  it('shortcuts_10_a_packaged_build_has_no_reload_nor_developer_tools', async () => {
    const { appMenu } = await import('../../../shared/menu');
    const help = { revealLog() {}, copyDiagnostics() {} };
    const roles = (dev: boolean) => items(appMenu({ dev, help }) as Item[]).map((i) => i.role).filter(Boolean);
    const DEV = ['reload', 'forceReload', 'toggleDevTools'];
    // the menu a release gets: no Reload, Force Reload, Toggle Developer Tools, nor Electron's View menu that has them
    for (const role of [...DEV, 'viewMenu']) expect(roles(false), role).not.toContain(role);
    // the rest of Electron's View menu stays
    expect(roles(false)).toEqual(expect.arrayContaining(['resetZoom', 'zoomIn', 'zoomOut', 'togglefullscreen']));
    // a run from source keeps them
    expect(roles(true)).toEqual(expect.arrayContaining(DEV));
    // main tells it which it is: a run from source has the dev server's address
    const index = readFileSync(join(process.cwd(), 'src/main/index.ts'), 'utf8');
    expect(index).toContain('const dev = !!process.env.ELECTRON_RENDERER_URL;');
    expect(index).toContain('appMenu({ dev, help: helpActions })');
  });

  it('shortcuts_11_cmd_k_finds_settings_pages_and_actions_under_their_own_headings', async () => {
    const newChat = vi.fn(), setting = vi.fn();
    const o = obid();
    const help = vi.fn(async () => null);
    (o.app as Record<string, unknown>).help = help;
    await showSidebar(o, [talk('c1', 'Release notes'), talk('c2', 'Settings sync')], { onNewChat: newChat, onOpenSetting: setting });
    await act(async () => (host.querySelector('[aria-label="Search"]') as HTMLElement).click());
    const field = host.querySelector<HTMLInputElement>('input[aria-label="Search"]')!;
    const heads = () => [...host.querySelectorAll('.sidebar-convo-head')].map((h) => h.textContent);
    const picks = () => [...host.querySelectorAll('.search-pick')].map((p) => p.textContent);
    // a Settings page by its name
    await type(field, 'gener');
    expect(heads()).toEqual(['Settings']);
    expect(picks()).toEqual(['General']);
    expect(host.querySelector('.sidebar-empty')).toBeNull();
    // the conversations first, then each group under its heading
    await type(field, 'settings');
    expect(talks()).toEqual(['Settings sync']);
    expect(heads()).toEqual(['Settings', 'Actions']);
    expect(picks()).toEqual(['General', 'Agents', 'Obysphere', 'Connectors', 'About', 'Open Settings']);
    // the actions: New conversation, Reveal obid's Log (main's), Open Settings
    await act(async () => (host.querySelector('[aria-label="Search"]') as HTMLElement).click());
    await type(host.querySelector<HTMLInputElement>('input[aria-label="Search"]')!, 'log');
    expect(picks()).toEqual(['Reveal obid’s Log']);
    await act(async () => (host.querySelector('[data-pick="action:reveal-log"]') as HTMLElement).click());
    expect(help).toHaveBeenCalledWith('reveal-log');
    await act(async () => (host.querySelector('[aria-label="Search"]') as HTMLElement).click());
    await type(host.querySelector<HTMLInputElement>('input[aria-label="Search"]')!, 'new conv');
    await act(async () => (host.querySelector('[data-pick="action:chat.new"]') as HTMLElement).click());
    expect(newChat).toHaveBeenCalledOnce();
    // a page picked opens it (the panel on Settings, as its account menu's Settings)
    await act(async () => (host.querySelector('[aria-label="Search"]') as HTMLElement).click());
    await type(host.querySelector<HTMLInputElement>('input[aria-label="Search"]')!, 'connectors');
    await act(async () => (host.querySelector('[data-pick="setting:connectors"]') as HTMLElement).click());
    expect(setting).toHaveBeenLastCalledWith('connectors');
    // nothing typed, no group; words match whatever their case
    const { pickGroups, ACTIONS } = await import('../search');
    expect(pickGroups('  ', [{ id: 'general', label: 'General' }])).toEqual({ settings: [], actions: [] });
    expect(pickGroups('OPEN set', [], ACTIONS).actions.map((a) => a.id)).toEqual(['settings.open']);
  });
});

describe('SEARCH: the side panel searches every conversation, through obid', { timeout: 180_000 }, () => {
  it('search_1_obid_finds_the_words_in_a_conversation_never_opened_here', async () => {
    // c2 was never opened: the app holds none of its words, obid's search does
    const o = obid({ jitter: [{ ...convo('c2', 'Backoff'), turn: { id: 't9', who: 'oby', at, text: '…added retry jitter to the client…' } }] });
    await showSidebar(o, [talk('c1', 'Release notes'), talk('c2', 'Backoff')]);
    await act(async () => (host.querySelector('[aria-label="Search"]') as HTMLElement).click());
    await type(host.querySelector<HTMLInputElement>('input[aria-label="Search"]')!, 'jitter');
    // nothing on the page holds it, until obid answers
    expect(talks()).toEqual([]);
    await until(() => expect(talks()).toEqual(['Backoff']));
    // under it, the words obid found it by
    expect(abouts()).toEqual(['…added retry jitter to the client…']);
    expect(o.calls).toContain('GET /v1/conversations?q=jitter&limit=50');
  });

  it('search_2_a_tasks_title_or_request_finds_its_conversation', async () => {
    const { tasksMatching } = await import('../search');
    const task = (title: string, request: string, conversation?: string) => ({ title, job: { request, conversation } }) as never;
    expect([...tasksMatching([task('Fix CI obi #812', 'Make it pass', 'c1'), task('Review', 'Look at the RETRY jitter', 'c2'), task('Loose', 'retry', undefined)], 'retry')]).toEqual(['c2']);
    expect([...tasksMatching([task('Fix CI obi #812', '', 'c1')], '  ')]).toEqual([]);
  });

  it('search_3_obid_is_asked_once_the_typing_stops_and_the_latest_words_win', async () => {
    const o = obid({ ret: [convo('c1', 'Release notes')], retry: [convo('c2', 'Backoff')] });
    await showSidebar(o, [talk('c1', 'Release notes'), talk('c2', 'Backoff')]);
    await act(async () => (host.querySelector('[aria-label="Search"]') as HTMLElement).click());
    const field = host.querySelector<HTMLInputElement>('input[aria-label="Search"]')!;
    await type(field, 'ret');
    await type(field, 'retry');
    await until(() => expect(talks()).toEqual(['Backoff']));
    // one question, of the words as they stood
    expect(o.calls.filter((c) => c.includes('?q='))).toEqual(['GET /v1/conversations?q=retry&limit=50']);
    // at most 200 characters go
    const { searchObid } = await import('../search');
    await searchObid(`  ${'x'.repeat(250)}  `);
    expect(o.calls.at(-1)).toBe(`GET /v1/conversations?q=${'x'.repeat(200)}&limit=50`);
    // cleared, obid's finds go with it
    await type(field, '');
    expect(talks()).toEqual(['Release notes', 'Backoff']);
  });
});
