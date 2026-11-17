// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// The side panel's title is the open profile, and its menu switches or makes one (docs/contracts/profiles.md,
// PROFILES-19 to 25). The real Sidebar, with main's bridge (window.app.obid.call) stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));

let root: Root | null = null;
let host: HTMLElement;

async function reset() {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
}
afterEach(async () => {
  await reset();
  localStorage.clear();
});

const DEFAULT = { id: 'a1b2c3d4-0000-4000-8000-000000000001', name: 'Default', color: '#3a5a8c', default: true, waiting: false };
const DESKREE = { id: 'a1b2c3d4-0000-4000-8000-000000000002', name: 'Deskree', color: '#2f7a6a', default: false, waiting: false };
const ACME = { id: 'a1b2c3d4-0000-4000-8000-000000000003', name: 'Acme', color: '#8c5a3a', default: false, waiting: false };
type Answer = { status: number; text: string };

// main's bridge, as obid answers: its profiles (or an answer for them), a create (made, it's listed); the rest 404.
// The profile the page tells main is open (obid.profile) is kept in told; main answers changed
function obid(profiles: unknown[] | Answer, changed = false) {
  const said = { profiles, create: undefined as Answer | undefined };
  const told: (string | null)[] = [];
  const profile = vi.fn(async (id: string | null) => { told.push(id); return changed; });
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'GET' && path === '/v1/profiles') return Array.isArray(said.profiles) ? { status: 200, text: JSON.stringify(said.profiles) } : said.profiles;
    if (method === 'POST' && path === '/v1/profiles') {
      if (said.create) return said.create;
      const made = { id: 'a1b2c3d4-0000-4000-8000-0000000000ff', ...(body as object), default: false, waiting: false };
      if (Array.isArray(said.profiles)) said.profiles = [...said.profiles, made];
      return { status: 201, text: JSON.stringify(made) };
    }
    return { status: 404, text: '{"error":"Not found."}' };
  });
  return { app: { obid: { call, profile }, onSphere: () => () => {}, sphereLeave: async () => null }, calls, said, told };
}
// obid's conversations, in the panel as live.ts puts them
const TALKS = [{ id: 'c1', title: 'Talk one', said: [], at: 5 }, { id: 'c2', title: 'Talk two', said: [], at: 50 }, { id: 'c3', title: 'Talk three', said: [], at: 500 }];

async function show(search: string, app?: object, talks?: typeof TALKS) {
  if (app) (window as { app?: unknown }).app = app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search });
  const { default: Sidebar } = await import('../Sidebar');
  if (talks) (await import('../chats')).replaceChats(talks);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  const none = () => {};
  await act(async () => root!.render(
    <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none}
      onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
  ));
}

const title = () => host.querySelector('.sidebar-title')?.textContent;
const titleButton = () => host.querySelector<HTMLButtonElement>('.sidebar-title button');
const until = async (check: () => void) => { await act(async () => { await vi.waitFor(check); }); };
const menu = () => host.querySelector('[role="menu"]');
const rows = () => [...host.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')];
// a row's name: its own words, not its mark's initial
const nameOf = (row: Element) => [...row.childNodes].filter((n) => n.nodeType === Node.TEXT_NODE).map((n) => n.textContent).join('');
const row = (name: string) => rows().find((r) => nameOf(r) === name)!;
const checked = () => rows().filter((r) => r.getAttribute('aria-checked') === 'true').map(nameOf);
const dotted = () => rows().filter((r) => r.querySelector('.status-dot.is-needs[aria-label]')).map(nameOf);
const talks = () => [...host.querySelectorAll('.convo-row .connector-name')].map((n) => n.textContent);
const click = async (el: Element) => { await act(async () => (el as HTMLElement).click()); };
const openMenu = async () => { await click(titleButton()!); await until(() => expect(menu()).not.toBeNull()); };
const button = (text: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === text || b.getAttribute('aria-label') === text);
async function type(text: string) {
  const field = host.querySelector<HTMLInputElement>('#profile-name')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(field, text);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

describe('PROFILES-19', () => {
  it('PROFILES-19: profiles_19_one_profile', async () => {
    await show('', obid([DEFAULT]).app);
    await until(() => expect(titleButton()).not.toBeNull());
    expect(title()).toBe('Default');
    expect(titleButton()!.querySelector('svg')).not.toBeNull();
    expect(titleButton()!.getAttribute('aria-haspopup')).toBe('menu');
    expect(titleButton()!.getAttribute('aria-expanded')).toBe('false');
    await openMenu();
    expect(titleButton()!.getAttribute('aria-expanded')).toBe('true');
    expect(rows().map(nameOf)).toEqual(['Default']);
    expect(checked()).toEqual(['Default']);
    expect(row('Default').querySelector('.menu-check')).not.toBeNull();
    expect(row('Default').querySelector<HTMLElement>('.owner-avatar')!.style.background).toMatch(/#3a5a8c|rgb\(58, 90, 140\)/);
    // the rows, a rule, then New profile…
    const items = [...menu()!.children];
    expect(items.map((e) => e.tagName)).toEqual(['BUTTON', 'HR', 'BUTTON']);
    expect(items[2].getAttribute('role')).toBe('menuitem');
    expect(items[2].textContent).toBe('New profile…');
  });
});

describe('PROFILES-20', () => {
  it('PROFILES-20: profiles_20_three_profiles_and_a_choice', async () => {
    const o = obid([DEFAULT, DESKREE, ACME]);
    await show('', o.app, TALKS);
    await until(() => expect(titleButton()).not.toBeNull());
    const before = talks();
    expect(before).toEqual(['Talk one', 'Talk two', 'Talk three']);
    await openMenu();
    expect(rows().map(nameOf)).toEqual(['Default', 'Deskree', 'Acme']);
    expect(checked()).toEqual(['Default']);

    const sent = o.calls.length;
    await click(row('Acme'));
    expect(menu()).toBeNull();
    expect(title()).toBe('Acme');
    expect(localStorage.getItem('oby.profile')).toBe(ACME.id);
    expect(talks()).toEqual(before);
    await openMenu();
    expect(checked()).toEqual(['Acme']);
    // nothing asked of obid but its list again; main is told Acme is open (B00-40, PROFILES-41)
    const since = o.calls.slice(sent);
    expect(since.every((c) => c.method === 'GET' && c.path === '/v1/profiles' && c.body === undefined)).toBe(true);
    expect(JSON.stringify(o.calls)).not.toContain(ACME.id);
    expect(o.told.at(-1)).toBe(ACME.id);
  });
});

describe('PROFILES-41', () => {
  it('PROFILES-41: profiles_41_main_is_told_the_open_profile', async () => {
    // the app opens on its kept profile: main is told it, and changed to it, the page reads everything again
    localStorage.setItem('oby.profile', DESKREE.id);
    const o = obid([DEFAULT, DESKREE, ACME], true);
    const reload = vi.fn();
    (window as { app?: unknown }).app = o.app;
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('location', { search: '', reload });
    // profiles.ts reads obid's list as it loads
    await import('../profiles');
    await until(() => expect(o.told).toEqual([DESKREE.id]));
    await until(() => expect(reload).toHaveBeenCalledTimes(1));
    // the same profile again: main doesn't change, nothing is read again
    const same = obid([DEFAULT, DESKREE, ACME], false);
    (window as { app?: unknown }).app = same.app;
    const { chooseProfile } = await import('../profiles');
    chooseProfile(DESKREE.id);
    await until(() => expect(same.told).toEqual([DESKREE.id]));
    expect(reload).toHaveBeenCalledTimes(1);
  });
});

describe('PROFILES-21', () => {
  it('PROFILES-21: profiles_21_one_waiting', async () => {
    const o = obid([DEFAULT, { ...DESKREE, waiting: true }, ACME]);
    await show('', o.app);
    await until(() => expect(titleButton()).not.toBeNull());
    await openMenu();
    expect(dotted()).toEqual(['Deskree']);
    await click(titleButton()!);
    expect(menu()).toBeNull();
    // obid's list is read again as the menu opens
    o.said.profiles = [DEFAULT, DESKREE, ACME];
    await openMenu();
    await until(() => expect(dotted()).toEqual([]));
    expect(rows().map(nameOf)).toEqual(['Default', 'Deskree', 'Acme']);
  });
});

describe('PROFILES-22', () => {
  it('PROFILES-22: profiles_22_creating', async () => {
    const o = obid([DEFAULT]);
    await show('', o.app);
    await until(() => expect(titleButton()).not.toBeNull());
    await openMenu();
    await click(button('New profile…')!);
    expect(menu()).toBeNull();
    expect(host.querySelector('[role="dialog"] h2')!.textContent).toBe('New profile');
    const create = () => button('Create profile')!;
    expect(create().disabled).toBe(true);
    await type('   ');
    expect(create().disabled).toBe(true);
    // the first colour no profile has is picked already
    expect(host.querySelector('[role="radio"][aria-checked="true"]')!.getAttribute('aria-label')).toBe('#7a4a8c');
    await type('Acme');
    await click(button('#8c5a3a')!);
    await click(create());
    await until(() => expect(host.querySelector('[role="dialog"]')).toBeNull());
    expect(o.calls.filter((c) => c.method === 'POST')).toEqual([{ method: 'POST', path: '/v1/profiles', body: { name: 'Acme', color: '#8c5a3a' } }]);
    expect(title()).toBe('Acme');
    await openMenu();
    expect(rows().map(nameOf)).toEqual(['Default', 'Acme']);
    expect(checked()).toEqual(['Acme']);
    const kept = localStorage.getItem('oby.profile');
    expect(kept).toBe('a1b2c3d4-0000-4000-8000-0000000000ff');

    // refused: obid's reason in the form, which stays; nothing else changes
    o.said.create = { status: 400, text: '{"error":"A profile’s name is 1 to 40 characters."}' };
    await click(button('New profile…')!);
    await type('Beta');
    await click(create());
    await until(() => expect(host.querySelector('[role="dialog"] [role="alert"]')?.textContent).toBe('A profile’s name is 1 to 40 characters.'));
    expect(host.querySelector('[role="dialog"]')).not.toBeNull();
    expect(title()).toBe('Acme');
    expect(localStorage.getItem('oby.profile')).toBe(kept);
    await click(button('Cancel')!);
    expect(host.querySelector('[role="dialog"]')).toBeNull();
    await openMenu();
    expect(rows().map(nameOf)).toEqual(['Default', 'Acme']);
  });
});

describe('PROFILES-23', () => {
  it('PROFILES-23: profiles_23_kept_across_a_restart', async () => {
    await show('', obid([DEFAULT, DESKREE, ACME]).app);
    await until(() => expect(titleButton()).not.toBeNull());
    await openMenu();
    await click(row('Acme'));
    expect(title()).toBe('Acme');

    // the app again, obid answering the same
    await reset();
    await show('', obid([DEFAULT, DESKREE, ACME]).app);
    await until(() => expect(title()).toBe('Acme'));

    // one obid no longer lists: the default, the kept id left as it was
    await reset();
    localStorage.setItem('oby.profile', 'a1b2c3d4-0000-4000-8000-0000000000aa');
    await show('', obid([DESKREE, DEFAULT, ACME]).app);
    await until(() => expect(titleButton()).not.toBeNull());
    expect(title()).toBe('Default');
    expect(localStorage.getItem('oby.profile')).toBe('a1b2c3d4-0000-4000-8000-0000000000aa');
  });
});

describe('PROFILES-24', () => {
  it('PROFILES-24: profiles_24_an_obid_without_profiles', async () => {
    for (const answer of [{ status: 404, text: '{"error":"Not found."}' }, { status: 0, text: 'Oby’s engine isn’t running.' }]) {
      const o = obid(answer);
      await show('', o.app, TALKS);
      await until(() => expect(o.calls.some((c) => c.path === '/v1/profiles')).toBe(true));
      expect(title()).toBe('Default');
      expect(titleButton()).toBeNull();
      expect(host.querySelector('[role="alert"]')).toBeNull();
      expect(talks()).toEqual(['Talk one', 'Talk two', 'Talk three']);
      expect(button('New conversation')).toBeDefined();
      await reset();
    }
  });
});

describe('PROFILES-25', () => {
  it('PROFILES-25: profiles_25_the_mock_shows_each_profiles_own', async () => {
    await show('?everything&chat');
    expect(title()).toBe('Default');
    const theirs = ['Who owns the Vault migration?', 'Rate limits on the Tetrix API', 'Summarize #398 discussion'];
    const mine = talks();
    expect(mine.length).toBeGreaterThan(0);
    expect(mine.some((t) => theirs.includes(t!))).toBe(false);
    await openMenu();
    expect(rows().map(nameOf)).toEqual(['Default', 'Deskree', 'Acme']);
    expect(dotted()).toEqual(['Deskree']);
    await click(row('Deskree'));
    expect(title()).toBe('Deskree');
    expect(talks()).toEqual(theirs);
    await openMenu();
    await click(row('Acme'));
    expect(talks()).toEqual(['Avenga contract renewal date', 'Draft reply to Avenga security']);
  });
});
