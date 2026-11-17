// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Chat } from '../chats';
import { liveBridge } from './liveBridge';
import { AGENTS_SHOWN } from '../panelFlags';

// The Agents side is hidden for now (panelFlags.ts): its panel's scenarios run again once it's shown
const shown = describe.skipIf(!AGENTS_SHOWN);

// Agents beside Projects in the side panel (docs/contracts/helpers.md, HELPERS-12 to 20): the real
// Sidebar, and Home with its conversation, with main's bridge (window.app.obid.call) stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
// the message boxes: what each is given, by its label
const boxes = vi.hoisted(() => ({}) as Record<string, { placeholder?: string; onSend: (text: string, att: object) => unknown }>);
vi.mock('../Composer', () => ({
  default: (props: { label: string; placeholder?: string; onSend: (text: string, att: object) => unknown }) => { boxes[props.label] = props; return <div data-box={props.label} data-placeholder={props.placeholder} />; },
  typingIn: () => false,
}));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));

let root: Root | null = null;
let host: HTMLElement;

async function reset() {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  for (const k of Object.keys(boxes)) delete boxes[k];
}
afterEach(async () => {
  await reset();
  localStorage.clear();
});

const DEFAULT = { id: 'a1b2c3d4-0000-4000-8000-000000000001', name: 'Default', color: '#3a5a8c', default: true, waiting: false };
const ACME = { id: 'a1b2c3d4-0000-4000-8000-000000000003', name: 'Acme', color: '#8c5a3a', default: false, waiting: false };
type Answer = { status: number; text: string };
const helper = (id: string, name: string, color: string, profile = DEFAULT.id) => ({
  id, profile, name, color, instructions: `Help with ${name}.`, tool: 'claude', connectors: ['web'], conversation: `conv-${id}`, created: '2026-11-15T15:30:00Z',
});
const INBOX = helper('h1', 'Inbox helper', '#8ee0b8'), RESEARCHER = helper('h2', 'Researcher', '#c2b5fd');
const NOTES = helper('h3', 'Release notes', '#ffb3c6'), ELSEWHERE = helper('h4', 'Acme replies', '#9fd3f0', ACME.id);

// main's bridge, as obid answers: its profiles and helpers (or an answer for them), a create (made, it's
// listed), words (taken); the rest 404
function obid(helpers: unknown[] | Answer, profiles: unknown[] | Answer = [DEFAULT, ACME], turns: Record<string, unknown[]> = {}) {
  const said = { helpers, create: undefined as Answer | undefined };
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'GET' && path === '/v1/profiles') return Array.isArray(profiles) ? { status: 200, text: JSON.stringify(profiles) } : profiles;
    if (method === 'GET' && path === '/v1/helpers') return Array.isArray(said.helpers) ? { status: 200, text: JSON.stringify(said.helpers) } : said.helpers;
    if (method === 'POST' && path === '/v1/helpers') {
      if (said.create) return said.create;
      const made = { id: 'h-new', profile: DEFAULT.id, ...(body as object), conversation: 'conv-h-new', created: '2026-11-16T10:00:00Z' };
      if (Array.isArray(said.helpers)) said.helpers = [...said.helpers, made];
      return { status: 201, text: JSON.stringify(made) };
    }
    if (method === 'POST' && path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'none', jobs: [] }) };
    if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: JSON.stringify(turns[path.slice(20).split('&')[0]] ?? []) };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  // main's follower of obid's events, reading a conversation's turns through the same obid
  const bridge = liveBridge(call);
  return { app: { obid: { call, ...bridge.obid }, onSphere: () => () => {}, sphereLeave: async () => null }, calls, said, bridge };
}
// obid's conversations, in the panel as live.ts puts them (at: minutes since they moved)
const talk = (id: string, title: string, at: number, more: Partial<Chat> = {}): Chat => ({ id, title, said: [], at, ...more });
const TALKS = [talk('c1', 'Talk one', 5), talk('c2', 'Talk two', 50)];

const opened: string[] = [];
async function show(search: string, app?: object, chats?: Chat[]) {
  if (app) (window as { app?: unknown }).app = app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search });
  const { default: Sidebar } = await import('../Sidebar');
  if (chats) (await import('../chats')).replaceChats(chats);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  opened.length = 0;
  const none = () => {};
  await act(async () => root!.render(
    <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none}
      onOpenTalk={(id) => opened.push(id)} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
  ));
}
// Home, the window's whole page: its side panel and the conversation opened from it, on what obid
// lists (its conversations, its inbox), through live.ts as the app reads it
const at = '2026-11-16T10:00:00Z';
const convo = (id: string, title: string, more: object = {}) => ({ id, title, pinned: false, created: at, lastActivity: at, cursor: '', ...more });
async function showHome(o: ReturnType<typeof obid>, listed: { conversations: unknown[]; inbox?: unknown[] }) {
  (window as { app?: unknown }).app = { ...o.app, onFullScreen: () => () => {} };
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  localStorage.setItem('oby.setUp', '1');
  localStorage.setItem('oby.panelSide', 'agents');
  const { default: Home } = await import('../Home');
  const live = await import('../live');
  live.connect();
  o.bridge.sync(listed);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Home />));
}

const until = async (check: () => void) => { await act(async () => { await vi.waitFor(check); }); };
const click = async (el: Element) => { await act(async () => (el as HTMLElement).click()); };
const tabs = () => [...host.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
const tab = (name: string) => tabs().find((t) => t.textContent === name)!;
const button = (text: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === text || b.getAttribute('aria-label') === text);
const talks = () => [...host.querySelectorAll('.convo-row .connector-name')].map((n) => n.textContent);
const agents = () => [...host.querySelectorAll('.agent-row')];
const agentNames = () => agents().map((r) => r.querySelector('.connector-name')!.textContent);
const agent = (name: string) => agents().find((r) => r.querySelector('.connector-name')!.textContent === name)!;
const heads = () => [...host.querySelectorAll('.sidebar-convo-head')].map((h) => [...h.childNodes].map((n) => n.textContent));
const ownLine = () => host.querySelector('.chat-own .connector-about')?.textContent;
const dialog = () => host.querySelector('[role="dialog"]');
// the conversation showing (one it opened over may still be on its way out)
const layer = () => host.querySelector('.chat-layer:not(.is-leaving)');
async function fill(selector: string, text: string) {
  const field = host.querySelector<HTMLInputElement | HTMLTextAreaElement>(selector)!;
  const proto = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  await act(async () => {
    Object.getOwnPropertyDescriptor(proto, 'value')!.set!.call(field, text);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
}
async function toAgents() {
  await click(tab('Agents'));
  await until(() => expect(button('New agent')).toBeDefined());
}
async function openSheet() {
  await click(button('New agent')!);
  expect(dialog()!.querySelector('h2')!.textContent).toBe('New agent');
}
const posted = (calls: { method: string; path: string; body?: unknown }[]) => calls.filter((c) => c.method === 'POST' && c.path === '/v1/helpers');

describe('agentRows', () => {
  it('HELPERS-13: the open profile’s, those that need you first, each group most recently moved first, one not listed last in obid’s order', async () => {
    vi.stubGlobal('location', { search: '' });
    const { agentRows } = await import('../AgentsList');
    const a = helper('a', 'A', '#8ee0b8'), b = helper('b', 'B', '#c2b5fd'), c = helper('c', 'C', '#ffb3c6'), d = helper('d', 'D', '#9fd3f0'), e = helper('e', 'E', '#ffe089'), f = helper('f', 'F', '#f1f1ee', ACME.id);
    const chats = [talk('conv-a', 'A', 30), talk('conv-b', 'B', 5), talk('conv-c', 'C', 60, { needs: true }), talk('conv-d', 'D', 10)];
    // d's tasks: one needs you; b at work; e's conversation not listed
    const rows = agentRows([a, b, c, d, e, f], chats, new Set(['conv-d']), new Set(['conv-b']), DEFAULT.id);
    expect(rows.needs.map((r) => r.helper.name)).toEqual(['D', 'C']);
    expect(rows.rest.map((r) => r.helper.name)).toEqual(['B', 'A', 'E']);
    expect(rows.rest.map((r) => r.working)).toEqual([true, false, false]);
    expect(rows.rest[2].chat).toBeUndefined();
    // no profile list: every one
    expect(agentRows([a, f], [], new Set(), new Set()).rest.map((r) => r.helper.name)).toEqual(['A', 'F']);
    // two not listed: obid's order
    expect(agentRows([e, a, b], [], new Set(), new Set(), DEFAULT.id).rest.map((r) => r.helper.name)).toEqual(['E', 'A', 'B']);
    // typing alone is working; the chat's own needs alone is needing
    expect(agentRows([b], [talk('conv-b', 'B', 1, { typing: true })], new Set(), new Set(), DEFAULT.id).rest[0].working).toBe(true);
  });
});

shown('HELPERS-12', () => {
  it('HELPERS-12: helpers_12_the_switch', async () => {
    await show('', obid([INBOX]).app, TALKS);
    expect(tabs().map((t) => t.textContent)).toEqual(['Projects', 'Agents']);
    // under the title, the first thing after it
    expect(host.querySelector('.sidebar-title-row')!.nextElementSibling!.getAttribute('role')).toBe('tablist');
    expect(tab('Projects').getAttribute('aria-selected')).toBe('true');
    expect(tab('Agents').getAttribute('aria-selected')).toBe('false');
    expect(host.querySelector('.chat-own')).not.toBeNull();
    expect(talks()).toEqual(['Talk one', 'Talk two']);
    expect(button('New agent')).toBeUndefined();
    // the chosen side is the one tab stop, and its panel is what shows
    expect(tabs().map((t) => t.tabIndex)).toEqual([0, -1]);
    expect(host.querySelector('[role="tabpanel"]')!.getAttribute('aria-labelledby')).toBe(tab('Projects').id);
    expect(button('Search')).toBeDefined();

    // an open search closes on Agents, which has none
    await click(button('Search')!);
    await toAgents();
    expect(tab('Agents').getAttribute('aria-selected')).toBe('true');
    expect(host.querySelector('[role="tabpanel"]')!.getAttribute('aria-labelledby')).toBe(tab('Agents').id);
    expect(button('Search')).toBeUndefined();
    expect(host.querySelector('.sidebar-search-wrap.is-open')).toBeNull();
    expect(host.querySelector('.chat-own')).toBeNull();
    expect(button('New conversation')).toBeUndefined();
    expect(talks()).toEqual([]);
    await until(() => expect(agentNames()).toEqual(['Inbox helper']));
    expect(localStorage.getItem('oby.panelSide')).toBe('agents');

    // the arrow keys move between the two, and focus with them
    const arrow = async (key: string) => { await act(async () => { document.activeElement!.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true })); }); };
    tab('Agents').focus();
    await arrow('ArrowLeft');
    expect(tab('Projects').getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(tab('Projects'));
    expect(talks()).toEqual(['Talk one', 'Talk two']);
    await arrow('ArrowRight');
    expect(tab('Agents').getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(tab('Agents'));
    expect(localStorage.getItem('oby.panelSide')).toBe('agents');

    // the app again: still Agents
    await reset();
    localStorage.setItem('oby.panelSide', 'agents');
    await show('', obid([INBOX]).app, TALKS);
    expect(tab('Agents').getAttribute('aria-selected')).toBe('true');
    await until(() => expect(agentNames()).toEqual(['Inbox helper']));
    await click(tab('Projects'));
    expect(localStorage.getItem('oby.panelSide')).toBe('projects');
    expect(talks()).toEqual(['Talk one', 'Talk two']);
  });
});

shown('HELPERS-13', () => {
  it('HELPERS-13: helpers_13_needs_you_first_in_the_open_profile', async () => {
    const chats = [...TALKS, talk(INBOX.conversation, 'Inbox helper', 30, { needs: true }), talk(RESEARCHER.conversation, 'Researcher', 3, { typing: true, step: 'Reading the pricing page' }),
      talk(NOTES.conversation, 'Release notes', 120, { said: [{ who: 'oby', text: 'Drafted the notes.', ago: 120 }] }), talk(ELSEWHERE.conversation, 'Acme replies', 1)];
    await show('', obid([INBOX, RESEARCHER, NOTES, ELSEWHERE]).app, chats);
    await toAgents();
    await until(() => expect(agentNames()).toEqual(['Inbox helper', 'Researcher', 'Release notes']));
    expect(heads()).toEqual([['Needs you', '1'], ['Agents']]);
    // the headings over their rows
    const list = [...host.querySelectorAll('.sidebar-convo-head, .agent-row')].map((e) => e.querySelector('.connector-name')?.textContent ?? e.firstChild!.textContent);
    expect(list).toEqual(['Needs you', 'Inbox helper', 'Agents', 'Researcher', 'Release notes']);
    expect(agent('Inbox helper').querySelector('.status-dot.is-needs')!.getAttribute('aria-label')).toBe('Needs you');
    expect(agent('Researcher').querySelector('.status-dot.is-working')!.getAttribute('aria-label')).toBe('Working');
    expect(agent('Researcher').querySelector('.connector-about')!.textContent).toBe('Reading the pricing page');
    expect(agent('Release notes').querySelector('.status-dot')).toBeNull();
    expect(agent('Release notes').querySelector('.sidebar-item-ago')!.textContent).toBe('2h');
    expect(agent('Release notes').querySelector('.connector-about')!.textContent).toBe('Drafted the notes.');
    // each in its colour; no menu
    expect(agent('Researcher').querySelector('svg path')!.getAttribute('fill')).toBe('#c2b5fd');
    expect(agents().some((r) => r.querySelector('.sidebar-item-more'))).toBe(false);
    expect(agentNames()).not.toContain('Acme replies');
    // the Chats icon lit by the agent that needs you; Oby's row isn't
    expect(button('Chats')!.className).toContain('is-needs');
    await click(tab('Projects'));
    expect(host.querySelector('.chat-own .status-dot')).toBeNull();
    await click(tab('Agents'));
    await click(agent('Researcher'));
    expect(opened).toEqual([RESEARCHER.conversation]);
  });
});

describe('HELPERS-14', () => {
  it('HELPERS-14: helpers_14_a_helper_s_conversation_is_not_among_the_chats', async () => {
    const theirs = talk(INBOX.conversation, 'Inbox helper', 1, { helper: INBOX.id, said: [{ who: 'oby', text: 'Dana asked about Thursday.', ago: 1 }] });
    await show('', obid([INBOX]).app, [...TALKS, theirs]);
    expect(talks()).toEqual(['Talk one', 'Talk two']);
    // the latest moved, but not Oby's row's line
    expect(ownLine()).not.toContain('Dana');
    await click(button('Search')!);
    await fill('.sidebar-search input', 'Dana');
    expect(talks()).toEqual([]);
    await fill('.sidebar-search input', 'Talk');
    expect(talks()).toEqual(['Talk one', 'Talk two']);
    const { latestChat } = await import('../chats');
    expect(latestChat([...TALKS, theirs])?.id).toBe('c1');
  });
});

shown('HELPERS-15', () => {
  it('HELPERS-15: helpers_15_the_sheet_sends_what_it_shows', async () => {
    const o = obid([INBOX]);
    await show('', o.app, TALKS);
    await until(() => expect(o.calls.some((c) => c.path === '/v1/profiles')).toBe(true));
    await toAgents();
    await until(() => expect(agentNames()).toEqual(['Inbox helper']));
    await openSheet();
    expect(dialog()!.textContent).toContain('An agent is an Oby of its own: one conversation with you, one kind of work. It stays in this profile.');
    const swatches = [...dialog()!.querySelectorAll('.agent-swatches [role="radio"]')];
    expect(swatches.map((s) => s.getAttribute('aria-label'))).toEqual(['#8ee0b8', '#c2b5fd', '#ffb3c6', '#9fd3f0', '#ffe089', '#f1f1ee']);
    // the first colour no agent of this profile has, and Claude Code
    expect(dialog()!.querySelector('.agent-swatches [aria-checked="true"]')!.getAttribute('aria-label')).toBe('#c2b5fd');
    expect(button('Claude Code')!.getAttribute('aria-checked')).toBe('true');
    expect(host.querySelector('#agent-name')!.getAttribute('maxlength')).toBe('100');
    expect(host.querySelector('#agent-instructions')!.getAttribute('maxlength')).toBe('4000');
    const create = () => button('Create agent')!;
    expect(create().disabled).toBe(true);
    await fill('#agent-name', '  Researcher  ');
    expect(create().disabled).toBe(true);
    await fill('#agent-instructions', '   ');
    expect(create().disabled).toBe(true);
    await fill('#agent-instructions', ' Compare prices. ');
    await click(button('#ffb3c6')!);
    await click(button('GitHub')!);
    await click(button('The web')!);
    await click(button('Codex')!);
    await click(create());
    await until(() => expect(dialog()).toBeNull());
    expect(posted(o.calls)).toHaveLength(1);
    expect(posted(o.calls)[0].body).toEqual({ name: 'Researcher', color: '#ffb3c6', instructions: 'Compare prices.', tool: 'codex', connectors: ['github', 'web'], profile: DEFAULT.id });
    await until(() => expect(agentNames()).toContain('Researcher'));
    // its conversation opens once obid's event brings it
    expect(opened).toEqual([]);
    await act(async () => (await import('../chats')).replaceChats([...TALKS, talk('conv-h-new', 'Researcher', 0, { helper: 'h-new' })]));
    await until(() => expect(opened).toEqual(['conv-h-new']));

    // no profile list: none sent
    await reset();
    const bare = obid([], { status: 404, text: '{"error":"Not found."}' });
    await show('', bare.app, TALKS);
    await toAgents();
    await openSheet();
    await fill('#agent-name', 'Notes');
    await fill('#agent-instructions', 'Draft notes.');
    await click(button('Create agent')!);
    await until(() => expect(dialog()).toBeNull());
    expect(posted(bare.calls)[0].body).toEqual({ name: 'Notes', color: '#8ee0b8', instructions: 'Draft notes.', tool: 'claude', connectors: [] });
  });
});

shown('HELPERS-16', () => {
  it('HELPERS-16: helpers_16_gmail_and_slack_are_never_sent_on_obid', async () => {
    const o = obid([]);
    await show('', o.app, TALKS);
    await until(() => expect(o.calls.some((c) => c.path === '/v1/profiles')).toBe(true));
    await toAgents();
    await openSheet();
    for (const name of ['Gmail', 'Slack']) {
      expect(button(name)!.disabled).toBe(true);
      expect(button(name)!.getAttribute('aria-disabled')).toBe('true');
      await click(button(name)!);
      expect(button(name)!.getAttribute('aria-pressed')).toBe('false');
    }
    expect(button('GitHub')!.disabled).toBe(false);
    await click(button('GitHub')!);
    expect(button('GitHub')!.getAttribute('aria-pressed')).toBe('true');
    await fill('#agent-name', 'Inbox helper');
    await fill('#agent-instructions', 'Sort my inbox.');
    await click(button('Create agent')!);
    await until(() => expect(dialog()).toBeNull());
    expect(posted(o.calls)[0].body).toEqual({ name: 'Inbox helper', color: '#8ee0b8', instructions: 'Sort my inbox.', tool: 'claude', connectors: ['github'], profile: DEFAULT.id });

    // the mock: all four can be chosen, and the made one keeps them
    await reset();
    await show('?everything&chat');
    await toAgents();
    await openSheet();
    for (const name of ['Gmail', 'Slack', 'GitHub', 'The web']) {
      expect(button(name)!.disabled).toBe(false);
      await click(button(name)!);
    }
    await fill('#agent-name', 'Everything');
    await fill('#agent-instructions', 'All of it.');
    await click(button('Create agent')!);
    await until(() => expect(dialog()).toBeNull());
    const { useHelpers } = await import('../helpers');
    let list: ReturnType<typeof useHelpers> = [];
    const Read = () => { list = useHelpers(); return null; };
    await act(async () => createRoot(document.createElement('div')).render(<Read />));
    expect(list.at(-1)!.connectors).toEqual(['gmail', 'slack', 'github', 'web']);
    await until(() => expect(agentNames()).toContain('Everything'));
    expect(opened).toEqual([list.at(-1)!.conversation]);
  });
});

shown('HELPERS-17', () => {
  it('HELPERS-17: helpers_17_a_refusal_keeps_the_sheet', async () => {
    const o = obid([INBOX]);
    o.said.create = { status: 422, text: '{"error":"A Cursor agent needs The web: Cursor’s own web search can’t be taken away.","reason":"cursor-needs-web"}' };
    await show('', o.app, TALKS);
    await toAgents();
    await until(() => expect(agentNames()).toEqual(['Inbox helper']));
    await openSheet();
    await fill('#agent-name', 'Scout');
    await fill('#agent-instructions', 'Find things.');
    await click(button('Cursor')!);
    await click(button('GitHub')!);
    await click(button('#9fd3f0')!);
    await click(button('Create agent')!);
    await until(() => expect(dialog()?.querySelector('[role="alert"]')?.textContent).toBe('A Cursor agent needs The web: Cursor’s own web search can’t be taken away.'));
    expect(dialog()).not.toBeNull();
    expect(host.querySelector<HTMLInputElement>('#agent-name')!.value).toBe('Scout');
    expect(host.querySelector<HTMLTextAreaElement>('#agent-instructions')!.value).toBe('Find things.');
    expect(button('Cursor')!.getAttribute('aria-checked')).toBe('true');
    expect(button('GitHub')!.getAttribute('aria-pressed')).toBe('true');
    expect(dialog()!.querySelector('.agent-swatches [aria-checked="true"]')!.getAttribute('aria-label')).toBe('#9fd3f0');
    expect(agentNames()).toEqual(['Inbox helper']);
    expect(opened).toEqual([]);
    // and it can be pressed again
    expect(button('Create agent')!.disabled).toBe(false);
    await click(button('The web')!);
    o.said.create = undefined;
    await click(button('Create agent')!);
    await until(() => expect(dialog()).toBeNull());
    expect(posted(o.calls).map((c) => (c.body as { connectors: string[] }).connectors)).toEqual([['github'], ['github', 'web']]);
  });
});

shown('HELPERS-18', () => {
  it('HELPERS-18: helpers_18_an_agent_s_conversation', async () => {
    const turns = { [RESEARCHER.conversation]: [
      { id: 't1', who: 'user', text: 'Compare what we pay.', at, jobs: [] }, { id: 't2', who: 'oby', text: 'On it. Reading each pricing page.', at, jobs: [] },
    ] };
    const o = obid([INBOX, RESEARCHER], [DEFAULT, ACME], turns);
    await showHome(o, { conversations: [convo('c1', 'Talk one'), convo(RESEARCHER.conversation, 'Researcher', { helper: RESEARCHER.id })] });
    // (Inbox helper's conversation isn't listed: last)
    await until(() => expect(agentNames()).toEqual(['Researcher', 'Inbox helper']));
    await click(agent('Researcher'));
    await until(() => expect(layer()?.textContent).toContain('On it. Reading each pricing page.'));
    // beside its words, its face in its colour
    expect(layer()!.querySelector('.msg-face svg path')!.getAttribute('fill')).toBe('#c2b5fd');
    // in the bar: its face and its name, and no ⋮
    const bar = host.querySelector('.crumbs')!;
    expect(bar.querySelector('svg path')!.getAttribute('fill')).toBe('#c2b5fd');
    expect(bar.querySelector('.crumb.is-here')!.textContent).toBe('Researcher');
    expect(button('Conversation options')).toBeUndefined();
    expect(host.querySelector('.bar-schedule')).toBeNull();
    // its row is the one showing
    expect(agent('Researcher').getAttribute('aria-current')).toBe('true');
    expect(agent('Researcher').className).toContain('is-active');
    // the box names it, and its words go to its conversation
    expect(boxes['Message Researcher'].placeholder).toBe('Message Researcher…');
    await act(async () => { boxes['Message Researcher'].onSend('And Codex?', {}); });
    await until(() => expect(o.calls.filter((c) => c.path === '/v1/plans').map((c) => c.body)).toEqual([expect.objectContaining({ said: 'And Codex?', conversation: RESEARCHER.conversation })]));

    // Oby's own conversation is as it was, and the agent's isn't among the chats
    await click(tab('Projects'));
    expect(talks()).toEqual(['Talk one']);
    await click([...host.querySelectorAll('.convo-row')].find((r) => r.textContent!.includes('Talk one'))!);
    await until(() => expect(host.querySelector('.crumb.is-here')!.textContent).toBe('Talk one'));
    expect(button('Conversation options')).toBeDefined();
    expect(boxes['Reply to Oby'].placeholder).toBe('Reply to Oby…');
  });
});

shown('HELPERS-19', () => {
  const cards = () => [...(layer()?.querySelectorAll('.tc-ask') ?? [])].map((c) => ({
    kind: c.querySelector('.tc-ask-kind')!.textContent, head: c.querySelector('.tc-ask-head')!.textContent, options: [...c.querySelectorAll('.tc-option')].map((b) => b.textContent),
  }));

  it('HELPERS-19: helpers_19_an_agent_waiting_on_two_questions', async () => {
    // on obid: two Confirms in its conversation
    const confirm = (id: string, headline: string) => ({ id, kind: 'confirm', job: '', title: headline, text: 'Its words', options: ['Confirm', 'Cancel'], details: null, created: at, state: 'open', later: null, headline, recommended: null, conversation: INBOX.conversation });
    const o = obid([INBOX, RESEARCHER]);
    await showHome(o, { conversations: [convo('c1', 'Talk one'), convo(INBOX.conversation, 'Inbox helper', { helper: INBOX.id }), convo(RESEARCHER.conversation, 'Researcher', { helper: RESEARCHER.id })],
      inbox: [confirm('i1', 'Send this reply to Dana?'), confirm('i2', 'Reply to Acme?')] });
    await until(() => expect(agentNames()).toEqual(['Inbox helper', 'Researcher']));
    expect(heads()).toEqual([['Needs you', '1'], ['Agents']]);
    expect(agent('Inbox helper').querySelector('.status-dot.is-needs')!.getAttribute('aria-label')).toBe('Needs you');
    // what waits in it has its row: Oby's isn't lit for it
    await click(tab('Projects'));
    expect(host.querySelector('.chat-own .status-dot')).toBeNull();
    expect(button('Chats')!.className).toContain('is-needs');
    await click(tab('Agents'));
    await click(agent('Inbox helper'));
    await until(() => expect(cards()).toHaveLength(2));
    expect(cards().map((c) => [c.head, c.options])).toEqual([['Send this reply to Dana?', ['Confirm', 'Cancel', 'Later']], ['Reply to Acme?', ['Confirm', 'Cancel', 'Later']]]);
    await reset();

    // the mock's Inbox helper (screen 09): two Question cards
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('location', { search: '?everything&chat' });
    localStorage.setItem('oby.panelSide', 'agents');
    const { default: Home } = await import('../Home');
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<Home />));
    expect(heads()[0]).toEqual(['Needs you', '1']);
    expect(agentNames()[0]).toBe('Inbox helper');
    expect(agent('Inbox helper').querySelector('.status-dot.is-needs')).not.toBeNull();
    await click(agent('Inbox helper'));
    await until(() => expect(cards()).toHaveLength(2));
    expect(cards()).toEqual([
      { kind: 'Question', head: 'Send this reply to Dana?', options: ['Send it', 'Edit first', 'Don’t reply', 'Later'] },
      { kind: 'Question', head: 'What is the order number for Acme?', options: ['Give the number', 'Read the draft', 'Later'] },
    ]);
    expect(layer()!.querySelector('.msg-face svg path')!.getAttribute('fill')).toBe('#8ee0b8');
    expect(host.querySelector('.bar-schedule')!.textContent).toBe('Every weekday at 9:00 AM');
    expect(boxes['Message Inbox helper'].placeholder).toBe('Message Inbox helper…');
  });
});

shown('HELPERS-20', () => {
  it('HELPERS-20: helpers_20_an_obid_without_helpers', async () => {
    for (const answer of [{ status: 404, text: '{"error":"Not found."}' }, { status: 0, text: 'Oby’s engine isn’t running.' }]) {
      const o = obid(answer);
      await show('', o.app, TALKS);
      await until(() => expect(o.calls.some((c) => c.path === '/v1/helpers')).toBe(true));
      expect(talks()).toEqual(['Talk one', 'Talk two']);
      expect(host.querySelector('.chat-own')).not.toBeNull();
      await toAgents();
      expect(agents()).toHaveLength(0);
      expect(heads()).toEqual([]);
      expect(host.querySelector('.sidebar-empty')!.textContent).toBe('No agents yet');
      expect(host.querySelector('[role="alert"]')).toBeNull();
      await click(tab('Projects'));
      expect(talks()).toEqual(['Talk one', 'Talk two']);
      expect(button('New conversation')).toBeDefined();
      await reset();
    }
  });
});

describe('the face', () => {
  it('HELPERS-18: ObyFace and ObyFaceLive draw Oby’s own white, or an agent’s colour', async () => {
    vi.stubGlobal('location', { search: '' });
    const { renderToStaticMarkup } = await import('react-dom/server');
    const { ObyFace } = await import('../ObyFace');
    const { default: ObyFaceLive } = await import('../ObyFaceLive');
    expect(renderToStaticMarkup(<ObyFace />)).toContain('fill="#f1f1ee"');
    expect(renderToStaticMarkup(<ObyFace color="#c2b5fd" />)).toContain('fill="#c2b5fd"');
    expect(renderToStaticMarkup(<ObyFace color="#c2b5fd" />)).not.toContain('#f1f1ee');
    expect(renderToStaticMarkup(<ObyFaceLive live={false} />)).toContain('fill="#f1f1ee"');
    expect(renderToStaticMarkup(<ObyFaceLive live={false} color="#c2b5fd" />)).toContain('fill="#c2b5fd"');
    expect(renderToStaticMarkup(<ObyFaceLive live={false} color="#c2b5fd" />)).not.toContain('#f1f1ee');
  });
});
