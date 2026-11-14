// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Job, Status } from '../obid';

// An agent this build doesn't know (docs/contracts/providers.md, PROVIDERS-5 and 6): its account and
// jobs show under its id with a gray lettered mark, the pickers list it disabled, and nothing is sent
// for it, nor any choice kept for it touched.

vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));
const state = vi.hoisted(() => ({ status: null as unknown }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: state.status, repos: [] }) }));

const row = (id: string, agent: string, name: string, more: Record<string, unknown> = {}) =>
  ({ id, agent, name, signedIn: true, fiveHour: null, sevenDay: null, sevenDayResets: null, running: 0, ...more });
const gem = row('g', 'gemini', 'Gem');
const personal = row('default', 'claude', 'Personal');
// obid's status: it keeps Agents' choices (defaultAccount), or, older, doesn't
const statusOf = (accounts: unknown[], kept: Record<string, unknown> | null = { defaultAccount: null, talkAccount: null }) =>
  ({ accounts, availableLogins: [], models: {}, ...kept }) as unknown as Status;

// every request the app sends obid
const call = vi.fn(async (method: string, path: string) => (method === 'GET' && path === '/v1/status' ? { status: 200, text: JSON.stringify(state.status) } : { status: 404, text: '{}' }));
// what would choose, tune or sign in: anything but a read, and a sign-in's page
const sent = () => call.mock.calls.filter(([method, path]) => method !== 'GET' || path.includes('sign-in'));

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  (window as unknown as { app: unknown }).app = { obid: { call } };
  localStorage.clear();
  call.mockClear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

// the app on obid with these accounts
async function app(accounts: unknown[], kept?: Record<string, unknown> | null) {
  state.status = statusOf(accounts, kept);
  const settings = await import('../SettingsView');
  settings.replaceAccounts(accounts.map((a) => settings.accountRow(a as never, Date.now())));
  return settings;
}
const buttons = (within: ParentNode = document) => Array.from(within.querySelectorAll<HTMLButtonElement>('button'));
const button = (text: string, within: ParentNode = document) => buttons(within).find((b) => b.textContent?.includes(text));
const click = async (b: HTMLElement) => { await act(async () => b.click()); };
async function pill() {
  const { default: TalkPill } = await import('../TalkPill');
  await act(async () => root.render(<TalkPill />));
  return host.querySelector<HTMLButtonElement>('button.talk-pick')!;
}
async function agents() {
  const { Agents } = await import('../SettingsView');
  await act(async () => root.render(<Agents />));
}
const card = (title: string) => Array.from(host.querySelectorAll('section.set-group')).find((s) => s.querySelector('h2')?.textContent === title);
const option = (select: string, id: string) => host.querySelector<HTMLOptionElement>(`select[aria-label="${select}"] option[value="${id}"]`);

describe('PROVIDERS-5: an agent this build doesn’t know shows under its id', () => {
  it('providers_5_unknown_agent_shows_its_id_and_a_lettered_mark', async () => {
    const { agentOf } = await import('../mock');
    const { BrandMark, BRANDS } = await import('../brands');
    const gemini = agentOf('gemini');
    expect(gemini.name).toBe('gemini');
    expect(gemini.brand.letter).toBe('G');
    // the gray circle, its letter on it, drawn mono too
    for (const mono of [false, true]) {
      const mark = renderToStaticMarkup(<BrandMark brand={gemini.brand} mono={mono} />);
      expect(mark).toContain(`d="${BRANDS.circle.parts[0].d}" fill="#6e6e74"`);
      expect(mark).toMatch(/<text[^>]*>G<\/text>/);
    }
    expect(renderToStaticMarkup(<BrandMark brand={agentOf('codex').brand} />)).not.toContain('<text');
    // an empty id: the circle, no letter
    const empty = renderToStaticMarkup(<BrandMark brand={agentOf('').brand} mono />);
    expect(empty).toContain('fill="#6e6e74"');
    expect(empty).not.toContain('<text');

    // its job, as WIRE-4 draws it: card and row
    await app([personal, gem]);
    const live = await import('../live');
    const { default: TaskCard } = await import('../TaskCard');
    const { ThreadTask } = await import('../ChatView');
    const at = '2026-11-14T10:00:00Z';
    const job = {
      id: 'j1', request: 'Fix the login', agent: 'gemini', account: 'g', repo: 'o/r', branch: 'oby/fix-1', base: 'dev', issue: null, kind: 'task',
      status: 'done', step: 'Working', summary: '', created: at, finished: at, commits: null, published: null, rounds: 1, round: 1,
      check: null, model: null, dismissed: null, permission: null, pipeline: null, title: 'Fix the login',
    } as unknown as Job;
    const s = live.liveState();
    s.jobs = { j1: job };
    s.seen.add('j1');
    const task = live.taskOf(s, job);
    const read = (html: string) => html.replaceAll('<!-- -->', '');
    for (const html of [read(renderToStaticMarkup(<TaskCard task={task} />)), read(renderToStaticMarkup(<ThreadTask task={task} current onOpen={() => {}} />))]) {
      expect(html).toContain('gemini · ');
      expect(html).not.toContain('Claude Code');
    }

    // the message box's picker lists it, after the tools this build knows
    await click(await pill());
    const rows = buttons().filter((b) => b.getAttribute('role') === 'menuitemradio').map((b) => b.querySelector('.talk-choice-name')?.textContent);
    expect(rows).toEqual(['Auto', 'Claude', 'gemini']);
    const g = button('gemini')!;
    expect(g.querySelector('text')?.textContent).toBe('G');
    expect(g.textContent).not.toContain('Claude Code');
    await act(async () => root.unmount());
    root = createRoot(host);

    // Settings › Agents: its card, under its name, with its id and mark
    await agents();
    const gemCard = card('Gem')!;
    expect(gemCard).toBeTruthy();
    expect(gemCard.textContent).toContain('gemini');
    expect(gemCard.textContent).not.toContain('Claude Code');
    expect(gemCard.querySelector('svg text')?.textContent).toBe('G');
    // after the tools this build knows
    expect(Array.from(host.querySelectorAll('section.set-group h2')).map((h) => h.textContent)).toEqual(['Default agent', 'Personal', 'Gem']);
  });
});

describe('PROVIDERS-6: an agent this build doesn’t know is listed disabled, and nothing is sent for it', () => {
  it('providers_6_unknown_agent_is_listed_disabled_and_nothing_is_sent', async () => {
    await app([personal, gem]);
    // the picker: listed, but nothing to choose, no models to open
    await click(await pill());
    const g = button('gemini')!;
    expect(g.disabled).toBe(true);
    expect(g.getAttribute('aria-disabled')).toBe('true');
    expect(g.getAttribute('aria-haspopup')).toBeNull();
    await click(g);
    await act(async () => g.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })));
    expect(document.querySelector('.row-submenu')).toBeNull();
    expect(sent()).toEqual([]);
    // its usage, no model's name
    expect(g.querySelector('.talk-choice-about')?.textContent).toBe('No usage');
    await act(async () => root.unmount());
    root = createRoot(host);

    // Settings › Agents: no model, effort or sign-in; both choices list it, disabled
    await agents();
    let gemCard = card('Gem')!;
    expect(gemCard.textContent).not.toContain('Model');
    expect(gemCard.textContent).not.toContain('Effort');
    expect(button('Sign In', gemCard)).toBeUndefined();
    expect(button('Remove', gemCard)).toBeTruthy();
    expect(option('Default agent', 'g')?.disabled).toBe(true);
    expect(option('New conversations talk through', 'g')?.disabled).toBe(true);
    // the one this build knows, as before
    expect(option('Default agent', 'default')?.disabled).toBe(false);
    expect(host.querySelector<HTMLSelectElement>('select[aria-label="Default agent"]')?.value).toBe('default');
    // signed out: still no Sign In of its own
    const settings = await import('../SettingsView');
    await act(async () => settings.replaceAccounts([personal, row('g', 'gemini', 'Gem', { signedIn: false })].map((a) => settings.accountRow(a as never, Date.now()))));
    gemCard = card('Gem')!;
    expect(button('Sign In', gemCard)).toBeUndefined();
    // (the app did reach obid: it read its status)
    expect(call).toHaveBeenCalledWith('GET', '/v1/status', undefined);
    expect(sent()).toEqual([]);
  });

  it('providers_6_unknown_agent_is_listed_disabled_and_nothing_is_sent: a choice kept for it stays', async () => {
    // obid keeps them: the default and the talk-through are its account
    await app([personal, gem], { defaultAccount: 'g', talkAccount: 'g' });
    const shown = await pill();
    expect(shown.textContent).toContain('gemini');
    expect(shown.querySelector('svg text')?.textContent).toBe('G');
    await click(shown); await click(shown);
    await act(async () => root.unmount());
    root = createRoot(host);
    await agents();
    expect(host.querySelector<HTMLSelectElement>('select[aria-label="Default agent"]')?.value).toBe('g');
    expect(host.querySelector<HTMLSelectElement>('select[aria-label="New conversations talk through"]')?.value).toBe('g');
    expect(sent()).toEqual([]);
    await act(async () => root.unmount());

    // an older obid: kept here, and left as it was
    vi.resetModules();
    const kept = { 'oby.setting.agent.default': '"g"', 'oby.setting.talk.default': '"g"', 'oby.setting.models.mine': '{"g":{"model":"gem-pro"}}' };
    for (const [k, v] of Object.entries(kept)) localStorage.setItem(k, v);
    root = createRoot(host);
    await app([personal, gem], null);
    await click(await pill()); await click(host.querySelector<HTMLButtonElement>('button.talk-pick')!);
    await act(async () => root.unmount());
    root = createRoot(host);
    await agents();
    expect(host.querySelector<HTMLSelectElement>('select[aria-label="Default agent"]')?.value).toBe('g');
    for (const [k, v] of Object.entries(kept)) expect(localStorage.getItem(k)).toBe(v);
    expect(sent()).toEqual([]);
  });
});
