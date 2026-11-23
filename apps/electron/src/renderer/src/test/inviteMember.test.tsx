// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// docs/contracts/sphere-app.md, SPHEREAPP-20 to 24: Invite member in the side panel's Obysphere, real
// on the live one for its admins (Sangam's InviteModal, a link main makes on the click, Send
// invitation as a draft in the person's own mail app), and the mock's as he made it. The real
// Sidebar, with main's bridge stood in for (window.app).

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));

const MADE = `oby://join/sphere.example.com#sphere=demo&by=wellington&id=9187a87a-3126-424c-8861-8811bba1a5ec&secret=${'A'.repeat(43)}`;
const JOINED = { state: 'joined', url: 'https://sphere.example.com', sphere: 'demo', owner: 'wellington', obiId: 'obi-1' };
const REVOKED = { ...JOINED, state: 'revoked', revokedAt: '2026-11-19T12:00:00Z', revokedReason: 'removed' };

let root: Root | null = null;
let host: HTMLElement;
const writeText = vi.fn(async () => {});

afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
});

// main's bridge: obid joined (or as `membership` says), the Sphere's word on admin, and what main
// answers Invite member's click and Send invitation
function bridge({ admin = true, membership = JOINED as object, made = { invitations: [], link: MADE, refused: null, why: null } as unknown } = {}) {
  const call = vi.fn(async (_method: string, path: string) => {
    if (path === '/v1/obid/sphere') return { status: 200, text: JSON.stringify(membership) };
    if (path === '/v1/obid/sphere/members') return { status: 200, text: JSON.stringify([{ obi: 'obi-dana', owner: 'dana', accepts: ['question'], active: true }]) };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const app = {
    obid: { call }, onSphere: () => () => {}, sphereLeave: vi.fn(),
    spherePolicy: vi.fn(async () => ({ sphere: 'demo', policy: { version: 1, updated_at: '', rules: [] }, admin, refused: null, why: null, admins: admin ? ['wellington'] : ['dana'], me: 'wellington' })),
    sphereInvite: vi.fn(async () => made),
    sphereInviteMail: vi.fn(async (): Promise<string | null> => null),
  };
  return app;
}

async function show(search: string, app?: object) {
  if (app) (window as { app?: unknown }).app = app;
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  writeText.mockClear();
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
const settle = async () => { await act(async () => { await new Promise((r) => setTimeout(r, 20)); }); };
async function open(tile: string, heading = tile) {
  await act(async () => { await vi.waitFor(() => expect(button(tile)).toBeDefined()); });
  await click(tile);
  expect(host.querySelector('.sidebar-title')?.textContent).toBe(heading);
  await settle();
}
const modal = () => host.querySelector<HTMLFormElement>('[role="dialog"]');
const send = () => [...modal()!.querySelectorAll('button')].find((b) => b.type === 'submit')!;
// addresses typed into the modal's Email field, as a person would
async function typeEmails(text: string) {
  const input = modal()!.querySelector<HTMLInputElement>('#invite-emails')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

describe('SPHEREAPP-20', () => {
  it('sphereapp_20_invite_member_shows_on_the_live_obysphere_only_to_its_admins', async () => {
    const app = bridge();
    await show('', app);
    await open('demo');
    await act(async () => { await vi.waitFor(() => expect(button('Invite member')).toBeDefined()); });
    expect(app.spherePolicy).toHaveBeenCalled();
    // showing it asks main for no link
    expect(app.sphereInvite).not.toHaveBeenCalled();
  });

  it('sphereapp_20: a member who isn’t admin sees no Invite member', async () => {
    const app = bridge({ admin: false });
    await show('', app);
    await open('demo');
    expect(app.spherePolicy).toHaveBeenCalled();
    expect(button('Invite member')).toBeUndefined();
  });

  it('sphereapp_20: an Oby taken out sees no Invite member, whatever the rules’ read said', async () => {
    const app = bridge({ membership: REVOKED });
    await show('', app);
    await open('demo');
    expect(host.textContent).toContain('demo’s admin took your Oby out.');
    expect(button('Invite member')).toBeUndefined();
  });

  it('sphereapp_20: the mock’s Obysphere and the tour’s sample still show it', async () => {
    await show('?everything');
    await open('Deskree');
    expect(button('Invite member')).toBeDefined();
  });
});

describe('SPHEREAPP-21', () => {
  it('sphereapp_21_invite_member_s_click_has_main_make_one_link_the_modal_shows_and_copies', async () => {
    const app = bridge();
    await show('', app);
    await open('demo');
    await act(async () => { await vi.waitFor(() => expect(button('Invite member')).toBeDefined()); });
    await click('Invite member');
    // on that click, one link, through sphere-invite (SPHEREAPP-10's gating, in main), one main made
    // this run reused while the Sphere still lists it open
    expect(app.sphereInvite).toHaveBeenCalledTimes(1);
    expect(app.sphereInvite).toHaveBeenCalledWith(true);
    await settle();
    expect(modal()!.querySelector('#invite-title')!.textContent).toBe('Invite to demo’s Obysphere');
    expect(modal()!.querySelector('.invite-url')!.textContent).toBe(MADE);
    await act(async () => (modal()!.querySelector('[aria-label="Copy link"]') as HTMLButtonElement).click());
    expect(writeText).toHaveBeenCalledWith(MADE);
    expect(app.sphereInvite).toHaveBeenCalledTimes(1);
  });
});

describe('SPHEREAPP-22', () => {
  async function made() {
    const app = bridge();
    await show('', app);
    await open('demo');
    await act(async () => { await vi.waitFor(() => expect(button('Invite member')).toBeDefined()); });
    await click('Invite member');
    await settle();
    return app;
  }

  it('sphereapp_22_send_invitation_has_main_open_a_mail_draft_to_the_typed_addresses_with_the_link', async () => {
    const app = await made();
    expect(send().disabled).toBe(true);
    await typeEmails('ana@example.com, ben@example.com');
    expect(send().disabled).toBe(false);
    await act(async () => send().click());
    expect(app.sphereInviteMail).toHaveBeenCalledTimes(1);
    expect(app.sphereInviteMail).toHaveBeenCalledWith(['ana@example.com', 'ben@example.com'], MADE);
    await settle();
    // nothing went out from Oby: the draft is in the mail app; nobody is listed as invited
    expect(send().textContent).toBe('Draft opened');
    expect(host.textContent).not.toContain('Invited');
    expect(host.textContent).not.toContain('ana@example.com, ben@example.com');
    // a moment to read it, then it closes
    await act(async () => { await new Promise((r) => setTimeout(r, 1300)); });
    expect(modal()).toBeNull();
    expect(host.textContent).not.toContain('Invited');
  });

  it('sphereapp_22: main’s refusal says why, and the modal stays to try again', async () => {
    const app = await made();
    app.sphereInviteMail.mockResolvedValueOnce('Sending an invitation takes your own click.');
    await typeEmails('ana@example.com');
    await act(async () => send().click());
    await settle();
    expect(modal()!.querySelector('[role="alert"]')!.textContent).toBe('Sending an invitation takes your own click.');
    expect(send().textContent).toBe('Send invitation');
    expect(send().disabled).toBe(false);
  });
});

describe('SPHEREAPP-22: no more addresses than the link lets in', () => {
  async function opened(left?: number) {
    const app = bridge({ made: { invitations: [], link: MADE, refused: null, why: null, left } });
    await show('', app);
    await open('demo');
    await act(async () => { await vi.waitFor(() => expect(button('Invite member')).toBeDefined()); });
    await click('Invite member');
    await settle();
    return app;
  }
  const note = () => modal()!.querySelector('.sphere-note')?.textContent;

  it('sphereapp_22_more_addresses_than_the_link_has_joins_left_say_so_and_send_waits', async () => {
    const app = await opened(2);
    await typeEmails('a@example.com b@example.com');
    expect(note()).toBeUndefined();
    expect(send().disabled).toBe(false);
    await typeEmails('a@example.com b@example.com c@example.com');
    expect(note()).toBe('This link lets 2 people in. Take some out to send it.');
    expect(send().disabled).toBe(true);
    await act(async () => send().click());
    expect(app.sphereInviteMail).not.toHaveBeenCalled();
  });

  it('sphereapp_22: a new link lets 5 in', async () => {
    await opened();
    await typeEmails('a@example.com b@example.com c@example.com d@example.com e@example.com');
    expect(note()).toBeUndefined();
    await typeEmails('a@example.com b@example.com c@example.com d@example.com e@example.com f@example.com');
    expect(note()).toBe('This link lets 5 people in. Take some out to send it.');
    expect(send().disabled).toBe(true);
  });
});

describe('SPHEREAPP-23', () => {
  const refused = (why: string) => ({ invitations: [], link: null, refused: 'not_admin', why });
  async function opened(made: unknown) {
    const app = bridge({ made });
    await show('', app);
    await open('demo');
    await act(async () => { await vi.waitFor(() => expect(button('Invite member')).toBeDefined()); });
    await click('Invite member');
    await settle();
    return app;
  }
  const said = () => modal()!.querySelector('.invite-link [role="alert"]')?.textContent;

  it('sphereapp_23_a_refusal_shows_in_the_link_field_in_words_with_copy_and_send_off', async () => {
    await opened(refused('Only the Obysphere’s admins can invite.'));
    expect(said()).toBe('Only the Obysphere’s admins can invite.');
    expect(host.textContent).not.toContain('oby://');
    expect((modal()!.querySelector('[aria-label="Copy link"]') as HTMLButtonElement).disabled).toBe(true);
    await typeEmails('ana@example.com');
    expect(send().disabled).toBe(true);
  });

  it('sphereapp_23: not in an Obysphere, and no click main saw, say so too', async () => {
    await opened({ invitations: [], link: null, refused: 'not_joined', why: 'This Oby isn’t in an Obysphere.' });
    expect(said()).toBe('This Oby isn’t in an Obysphere.');
    await act(async () => root!.unmount());
    root = null;
    vi.resetModules();
    document.body.innerHTML = '';
    await opened({ invitations: [], link: null, refused: 'failed', why: 'Making a link takes your own click.' });
    expect(said()).toBe('Making a link takes your own click.');
  });

  it('sphereapp_23: an Obysphere Oby couldn’t reach says so', async () => {
    const app = await opened(null);
    expect(said()).toBe('Oby couldn’t reach your Obysphere.');
    expect(app.sphereInviteMail).not.toHaveBeenCalled();
  });
});

describe('SPHEREAPP-24', () => {
  it('sphereapp_24_the_live_modal_has_no_domain_toggle', async () => {
    const app = bridge();
    await show('', app);
    await open('demo');
    await act(async () => { await vi.waitFor(() => expect(button('Invite member')).toBeDefined()); });
    await click('Invite member');
    await settle();
    expect(modal()!.querySelector('[role="switch"]')).toBeNull();
    expect(modal()!.textContent).not.toContain('Only allow');
  });

  it('sphereapp_24: the mock’s modal keeps its toggle, its pretend link and Sent, and lists who was asked in', async () => {
    await show('?everything');
    await open('Deskree');
    await click('Invite member');
    expect(modal()!.querySelector('[role="switch"]')!.getAttribute('aria-label')).toBe('Only allow @deskree.com emails to join with the link');
    expect(modal()!.querySelector('.invite-url')!.textContent).toMatch(/^deskree\.obysphere\.com\/join\/[a-z0-9]+$/);
    await typeEmails('ana@deskree.com');
    await act(async () => send().click());
    expect(send().textContent).toBe('Sent');
    await act(async () => { await new Promise((r) => setTimeout(r, 1300)); });
    expect(modal()).toBeNull();
    expect(host.textContent).toContain('Invited');
    expect(host.textContent).toContain('ana@deskree.com');
  });
});
