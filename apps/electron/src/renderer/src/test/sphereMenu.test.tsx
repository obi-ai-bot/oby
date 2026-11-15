// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// The Obysphere in the side panel (docs/contracts/obysphere.md, SPHERE-14 to 16): Invite member only
// on the mock's and the sample, and the ⋮ menu's Leave through Settings' own confirmation. The real
// Sidebar, with main's bridge (window.app.obid.call, onSphere, sphereLeave) stood in for.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));

let root: Root | null = null;
let host: HTMLElement;

afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
});

// main's bridge: joined to Demo until sphereLeave, then not
function joined() {
  let state = 'joined';
  const sphereLeave = vi.fn(async () => { state = 'left'; return null; });
  const call = vi.fn(async (_method: string, path: string) => {
    if (path === '/v1/obid/sphere') return { status: 200, text: JSON.stringify(state === 'joined' ? { state, url: 'https://demo.example', sphere: 'demo', owner: 'owner-a', obiId: 'obi-1' } : { state: 'none' }) };
    if (path === '/v1/obid/sphere/members') return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  return { app: { obid: { call }, onSphere: () => () => {}, sphereLeave }, sphereLeave };
}

async function show(search: string, app?: object) {
  if (app) (window as { app?: unknown }).app = app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search });
  const { default: Sidebar } = await import('../Sidebar');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  const none = () => {};
  await act(async () => root!.render(
    <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none}
      onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
  ));
}

const button = (label: string) => [...host.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === label || b.textContent === label);
const click = async (label: string) => { await act(async () => button(label)!.click()); };
const title = () => host.querySelector('.sidebar-title')?.textContent;
const dialog = () => host.querySelector('[role="alertdialog"]');
// the panel on an Obysphere: its tile in the icon strip, then its name as the panel's title
async function open(tile: string, heading = tile) {
  await act(async () => { await vi.waitFor(() => expect(button(tile)).toBeDefined()); });
  await click(tile);
  expect(title()).toBe(heading);
}

describe('SPHERE-14', () => {
  it('SPHERE-14: a joined Obysphere’s panel shows no Invite member', async () => {
    await show('', joined().app);
    await open('demo');
    expect(button('Knowledge')).toBeDefined();
    expect(host.textContent).not.toContain('Invite member');
  });

  it('SPHERE-14: the mock’s Obysphere shows Invite member, with ?everything', async () => {
    await show('?everything');
    await open('Deskree');
    expect(button('Knowledge')).toBeDefined();
    expect(button('Invite member')).toBeDefined();
  });

  it('SPHERE-14: the tour’s sample shows Invite member', async () => {
    const { dev } = await import('../onboarding');
    dev!.set(() => ({ sphere: 'inside' }));
    await show('');
    await open('Deskree, a sample Obysphere', 'DeskreeSample');
    expect(button('Invite member')).toBeDefined();
  });
});

describe('SPHERE-15', () => {
  it('SPHERE-15: Leave in the ⋮ menu confirms as Settings does, and leaves once', async () => {
    const b = joined();
    await show('', b.app);
    await open('demo');
    await click('demo options');
    await click('Leave demo');
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Leave demo?');
    expect(button('Cancel')).toBeDefined();
    await click('Cancel');
    expect(dialog()).toBeNull();
    await click('demo options');
    await click('Leave demo');
    await act(async () => { dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })); });
    expect(dialog()).toBeNull();
    expect(b.sphereLeave).not.toHaveBeenCalled();
    expect(title()).toBe('demo');

    await click('demo options');
    await click('Leave demo');
    await act(async () => { (dialog()!.querySelector('button[type="submit"]') as HTMLButtonElement).click(); });
    await act(async () => { await vi.waitFor(() => expect(button('demo')).toBeUndefined()); });
    expect(b.sphereLeave).toHaveBeenCalledTimes(1);
    expect(dialog()).toBeNull();
    expect(title()).toBe('Chats');
    expect(button('Chats')!.getAttribute('aria-pressed')).toBe('true');
  });
});

describe('SPHERE-16', () => {
  it('SPHERE-16: an admin’s ⋮ menu has Settings and no Leave', async () => {
    await show('?everything');
    await open('Deskree');
    await click('Deskree options');
    const menu = host.querySelector('[role="menu"]') ?? host;
    expect([...menu.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent)).toEqual(['Settings']);
    expect(host.textContent).not.toContain('Leave');
  });
});
