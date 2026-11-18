// @vitest-environment happy-dom
import { act, useEffect } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// docs/contracts/sphere-app.md: the app's side of the Obysphere for launch, each test named after its
// scenario. The real screens, main's bridge stood in for (window.app), obid answering through it.

vi.mock('../Oby', () => ({ default: ({ onDrawn }: { onDrawn?: () => void }) => { useEffect(() => onDrawn?.(), []); return null; } }));

const LINK = `https://sphere.example.com/join#sphere=demo&name=Demo%20team&by=dmytro&id=9187a87a-3126-424c-8861-8811bba1a5ec&secret=${'A'.repeat(43)}`;
// what obid answers, by "METHOD path"; anything else 404
let answers: Record<string, unknown> = {};
// a status other than 200 for an answer, by "METHOD path"
let statuses: Record<string, number> = {};
const calls: string[] = [];
const bodies: unknown[] = [];
const call = vi.fn(async (method: string, path: string, body?: unknown) => {
  calls.push(`${method} ${path}`);
  bodies.push(body);
  const a = answers[`${method} ${path}`];
  return a === undefined ? { status: 404, text: '{"error":"Not found."}' } : { status: statuses[`${method} ${path}`] ?? 200, text: JSON.stringify(a) };
});
const sphereJoin = vi.fn();
const sphereHandoff = vi.fn();

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
  vi.stubGlobal('location', { search: '' });
  answers = { 'GET /v1/obid/sphere': { state: 'none', url: null, sphere: null, owner: 'wellington' }, 'GET /v1/repos': { repos: [] } };
  calls.length = 0; bodies.length = 0; statuses = {};
  sphereJoin.mockReset(); sphereHandoff.mockReset();
  (window as unknown as { app: unknown }).app = { obid: { call }, sphereJoin, sphereHandoff, openExternal: vi.fn() };
  localStorage.clear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

async function settle(n = 3) { for (let i = 0; i < n; i++) await act(async () => { await new Promise((r) => setTimeout(r, 20)); }); }
// typed as a person would: React reads the input event
async function type(input: HTMLInputElement, text: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}
const button = (text: string, within: ParentNode = document) => Array.from(within.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent === text);
const field = (label: string) => document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;

describe('SPHEREAPP-3: first run joins from a link', () => {
  async function sphereStep() {
    vi.stubGlobal('location', { search: '?onboarding=sphere' });
    const { default: FirstRun } = await import('../FirstRun');
    await act(async () => root.render(<FirstRun />));
    await settle();
  }

  it('sphereapp_3_first_run_s_obysphere_step_joins_from_a_pasted_link_showing_the_owner_id_first', async () => {
    await sphereStep();
    const input = field('Invite link');
    expect(input).toBeTruthy();
    expect(button('Join')!.disabled).toBe(true);
    // not a link: said, and Join stays off
    await type(input, 'https://example.com/hello');
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('This isn’t an Obysphere invite link.');
    expect(button('Join')!.disabled).toBe(true);
    // the link: its name and host, and the owner id it will join as, before any key is made
    await type(input, LINK);
    expect(host.querySelector('.join-link-under')!.textContent).toBe('Demo team, on sphere.example.com. You’ll join as wellington.');
    expect(sphereJoin).not.toHaveBeenCalled();
    // Join hands main the link; main's dialog and join answer
    sphereJoin.mockResolvedValueOnce({ kind: 'joined', name: 'Demo team' });
    await act(async () => button('Join')!.click());
    expect(sphereJoin).toHaveBeenCalledWith(LINK);
    expect(host.querySelector('[role="status"]')!.textContent).toBe('You’re in Demo team.');
    // and Let's go still goes on
    expect(button('Let’s go')).toBeTruthy();
  });

  it('sphereapp_3: why a join failed shows under the field, and the owner’s Cancel says nothing', async () => {
    await sphereStep();
    await type(field('Invite link'), LINK);
    sphereJoin.mockResolvedValueOnce({ kind: 'failed', name: 'Demo team', why: 'This link has expired, was used up or was cancelled. Ask whoever sent it for a new one.' });
    await act(async () => button('Join')!.click());
    expect(host.querySelector('[role="alert"]')!.textContent).toContain('This link has expired');
    await type(field('Invite link'), LINK);
    sphereJoin.mockResolvedValueOnce({ kind: 'cancelled' });
    await act(async () => button('Join')!.click());
    expect(host.querySelector('[role="alert"], [role="status"]')).toBeNull();
  });
});

describe('SPHEREAPP-9: the owner id, changed before the first join', () => {
  async function step() {
    vi.stubGlobal('location', { search: '?onboarding=sphere' });
    const { default: FirstRun } = await import('../FirstRun');
    await act(async () => root.render(<FirstRun />));
    await settle();
    await type(field('Invite link'), LINK);
  }

  it('sphereapp_9_without_obid_s_route_the_owner_id_shows_read_only', async () => {
    await step();
    expect(host.querySelector('.join-link-under')!.textContent).toBe('Demo team, on sphere.example.com. You’ll join as wellington.');
    expect(button('Change')).toBeUndefined();
    // obid was asked once, with a body that changes nothing
    expect(calls.filter((c) => c === 'PUT /v1/obid/sphere/owner')).toHaveLength(1);
    expect(bodies[calls.indexOf('PUT /v1/obid/sphere/owner')]).toEqual({});
  });

  it('sphereapp_9_with_obid_s_route_the_owner_changes_it_before_joining', async () => {
    answers['PUT /v1/obid/sphere/owner'] = { error: 'owner is required' };
    statuses['PUT /v1/obid/sphere/owner'] = 422;
    await step();
    await act(async () => button('Change')!.click());
    // not an id: words, and nothing sent
    await type(field('Owner id'), 'Dana Smith!');
    const sent = calls.length;
    await act(async () => button('Save')!.click());
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('An owner id is lowercase letters, digits and hyphens.');
    expect(calls.length).toBe(sent);
    // an id: obid keeps it, and the line says it
    answers['PUT /v1/obid/sphere/owner'] = { owner: 'dana-smith' };
    statuses['PUT /v1/obid/sphere/owner'] = 200;
    await type(field('Owner id'), 'Dana-Smith');
    await act(async () => button('Save')!.click());
    expect(bodies.at(-1)).toEqual({ owner: 'dana-smith' });
    expect(host.querySelector('.join-link-under')!.textContent).toBe('Demo team, on sphere.example.com. You’ll join as dana-smith. Change');
  });

  it('sphereapp_9: once joined, obid’s 409 says it can’t change', async () => {
    answers['PUT /v1/obid/sphere/owner'] = { error: 'owner is required' };
    statuses['PUT /v1/obid/sphere/owner'] = 422;
    await step();
    await act(async () => button('Change')!.click());
    answers['PUT /v1/obid/sphere/owner'] = { error: 'joined already' };
    statuses['PUT /v1/obid/sphere/owner'] = 409;
    await type(field('Owner id'), 'dana');
    await act(async () => button('Save')!.click());
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('It can’t change once you’ve joined an Obysphere.');
  });
});

describe('SPHEREAPP-4 and 5: Settings › Obysphere', () => {
  async function page(membership: unknown, members: unknown[] = []) {
    answers['GET /v1/obid/sphere'] = membership;
    answers['GET /v1/obid/sphere/members'] = members;
    (window as unknown as { app: Record<string, unknown> }).app.onSphere = () => () => {};
    const { SphereJoinPage } = await import('../SphereSettings');
    await act(async () => root.render(<SphereJoinPage />));
    await settle();
  }

  it('sphereapp_4_settings_obysphere_page_has_the_same_join_field', async () => {
    const { SETTINGS_PAGES } = await import('../SettingsView');
    expect(SETTINGS_PAGES.map((p) => p.label)).toContain('Obysphere');
    await page({ state: 'none', url: null, sphere: null, owner: 'wellington' });
    expect(host.querySelector('h2')!.textContent).toBe('Join an Obysphere');
    await type(field('Invite link'), LINK);
    sphereJoin.mockResolvedValueOnce({ kind: 'joined', name: 'Demo team' });
    await act(async () => button('Join')!.click());
    expect(sphereJoin).toHaveBeenCalledWith(LINK);
  });

  it('sphereapp_4: in one, the page says which and who you are in it, with no field', async () => {
    await page({ state: 'joined', url: 'https://sphere.example.com', sphere: 'demo', owner: 'wellington', obiId: 'obi-1' }, [{ owner: 'dana' }]);
    expect(host.textContent).toContain('Your Obysphere');
    expect(host.textContent).toContain('wellington');
    expect(document.querySelector('input[aria-label="Invite link"]')).toBeNull();
  });

  it('sphereapp_5_a_revoked_obysphere_says_so_with_the_field_to_come_back', async () => {
    await page({ state: 'revoked', url: 'https://sphere.example.com', sphere: 'demo', owner: 'wellington', obiId: 'obi-1' });
    expect(host.querySelector('[role="status"]')!.textContent).toBe('demo’s admin took your Oby out.');
    expect(field('Invite link')).toBeTruthy();
    // and its own settings say it too, without the rules or what it tells
    await act(async () => root.unmount());
    root = createRoot(host);
    const { default: SphereSettings } = await import('../SphereSettings');
    await act(async () => root.render(<SphereSettings id="live:demo" leaving={false} onClose={() => {}} onGone={() => {}} />));
    await settle();
    expect(host.textContent).toContain('demo’s admin took your Oby out.');
    expect(host.textContent).not.toContain('Who may ask whom');
    expect(host.textContent).not.toContain('What your Oby tells');
    expect(button('Leave')).toBeTruthy();
  });
});

describe('SPHEREAPP-6 and 7: a task handed to a member’s Oby', () => {
  async function modal() {
    const { HandOffModal } = await import('../HandOff');
    const closed = vi.fn();
    await act(async () => root.render(<HandOffModal owner="dana" name="Dana" onClose={closed} />));
    await settle();
    return closed;
  }

  it('sphereapp_6_the_form_hands_main_what_to_do_where_and_from_which_branch', async () => {
    answers['GET /v1/repos'] = { repos: [{ repo: 'acme/app', defaultBranch: 'trunk', listed: true }] };
    const closed = await modal();
    expect(document.querySelector('h2')!.textContent).toBe('Hand Dana’s Oby a task');
    // nothing filled: says what's missing, and nothing goes
    await act(async () => button('Hand it over')!.click());
    expect(document.querySelector('[role="alert"]')!.textContent).toBe('Say what to do.');
    expect(sphereHandoff).not.toHaveBeenCalled();
    await type(document.querySelector<HTMLInputElement>('#handoff-goal')!, 'Add a name option to the greeter');
    await type(document.querySelector<HTMLInputElement>('#handoff-repo')!, 'acme/app');
    // a repository obid knows starts from its own default branch
    expect(document.querySelector<HTMLInputElement>('#handoff-base')!.value).toBe('trunk');
    sphereHandoff.mockResolvedValueOnce(null);
    await act(async () => button('Hand it over')!.click());
    expect(sphereHandoff).toHaveBeenCalledWith({ to: 'dana', goal: 'Add a name option to the greeter', repo: 'acme/app', base: 'trunk' });
    expect(closed).toHaveBeenCalled();
    // the thread is read again, so the task shows there
    expect(calls).toContain('GET /v1/obid/sphere/peers/dana');
  });

  it('sphereapp_6: a refusal stays on the form in main’s words', async () => {
    const closed = await modal();
    await type(document.querySelector<HTMLInputElement>('#handoff-goal')!, 'Fix the flaky test');
    await type(document.querySelector<HTMLInputElement>('#handoff-repo')!, 'acme/app');
    sphereHandoff.mockResolvedValueOnce('This Obysphere’s rules don’t allow handing over tasks yet. An admin can change that.');
    await act(async () => button('Hand it over')!.click());
    expect(document.querySelector('[role="alert"]')!.textContent).toContain('rules don’t allow handing over tasks');
    expect(closed).not.toHaveBeenCalled();
  });

  it('sphereapp_7_what_comes_back_reads_under_the_task_in_words', async () => {
    const { handedLine, standing } = await import('../peers');
    const t = { name: 'Dana', owner: 'dana' };
    expect(handedLine('sent', t)).toBe('Handed over. It’s waiting on Dana.');
    expect(handedLine('accepted', t)).toBe('Dana said yes. Their Oby is on it.');
    expect(handedLine('done', t, null, true)).toBe('Dana’s Oby sent back its result.');
    expect(handedLine('declined', t, 'owner_declined')).toBe('Dana said no to this task.');
    expect(handedLine('declined', t, 'unavailable')).toBe('Dana’s Oby couldn’t start it.');
    expect(handedLine('declined', t, 'out_of_scope')).toBe('Dana’s Oby doesn’t take tasks.');
    expect(handedLine('declined', t, 'policy')).toContain('An admin can change them.');
    expect(handedLine('declined', t, 'something new')).toBe('Dana’s Oby declined without saying why.');
    // in a thread: the decline's reason, under the task, not a bare "Declined"
    const exchange = { id: 'x1', type: 'handoff', direction: 'outgoing', state: 'declined', waitingOnThem: false, quietAt: null };
    const posts = [
      { kind: 'handoff', text: 'Add a name option', reason: null, exchange },
      { kind: 'decline', text: null, reason: 'owner_declined', exchange },
    ];
    expect(standing(exchange as never, { ...t, posts } as never, Date.now())).toBe('Dana said no to this task.');
  });
});

describe('SPHEREAPP-8: hidden for launch', () => {
  it('sphereapp_8_the_room_is_never_looked_for_so_it_never_opens', async () => {
    answers['GET /v1/obid/sphere'] = { state: 'joined', url: 'https://sphere.example.com', sphere: 'demo', owner: 'wellington', obiId: 'obi-1' };
    answers['GET /v1/obid/sphere/members'] = [{ owner: 'dana' }];
    answers['GET /v1/spheres/demo/room?after=0'] = { room: 'available', posts: [] };
    (window as unknown as { app: Record<string, unknown> }).app.onSphere = () => () => {};
    const spheres = await import('../spheres');
    const room = await import('../room');
    await settle();
    expect(spheres.ROOM_SHOWS).toBe(false);
    expect(calls).toContain('GET /v1/obid/sphere/members');
    expect(calls.some((c) => c.includes('/room'))).toBe(false);
    expect(room.roomOpens(room.roomNow(), { slug: 'demo' })).toBe(false);
  });

  it('sphereapp_8: on obid the message box offers no other Obys to add', async () => {
    const { OBYS } = await import('../mock');
    expect(OBYS).toEqual([]);
  });
});

describe('SPHEREAPP-10: an admin invites teammates from Settings › Obysphere', () => {
  const JOINED = { state: 'joined', url: 'https://sphere.example.com', sphere: 'demo', owner: 'wellington', obiId: 'obi-1' };
  const MADE = `oby://join/sphere.example.com#sphere=demo&by=wellington&id=9187a87a-3126-424c-8861-8811bba1a5ec&secret=${'S'.repeat(43)}`;
  const OPEN = { id: 'inv-1', uses: 2, maxUses: 5, expiresAt: '2026-12-02T12:00:00Z' };
  const writeText = vi.fn(async () => {});
  const rules = (actions: string[]) => [{ id: 'members-ask', from_owner: '*', to_owner: '*', actions, repos: ['*'] }];
  const bridge = { spherePolicy: vi.fn(), sphereInvitations: vi.fn(), sphereInvite: vi.fn(), sphereWithdraw: vi.fn() };
  async function page(admin: boolean, actions = ['question', 'review_request']) {
    answers['GET /v1/obid/sphere'] = JOINED;
    answers['GET /v1/obid/sphere/members'] = [{ owner: 'dana' }];
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    for (const f of Object.values(bridge)) f.mockReset();
    bridge.spherePolicy.mockResolvedValue({ sphere: 'demo', policy: { version: 1, updated_at: '', rules: rules(actions) }, admin, refused: null, why: null });
    bridge.sphereInvitations.mockResolvedValue({ invitations: [OPEN], link: null, refused: null, why: null });
    Object.assign((window as unknown as { app: Record<string, unknown> }).app, { onSphere: () => () => {}, ...bridge });
    const { SphereJoinPage } = await import('../SphereSettings');
    await act(async () => root.render(<SphereJoinPage />));
    await settle();
  }

  it('sphereapp_10_a_member_who_isnt_admin_sees_no_invites', async () => {
    await page(false);
    expect(host.textContent).not.toContain('Invite teammates');
    expect(bridge.sphereInvitations).not.toHaveBeenCalled();
  });

  it('sphereapp_10_the_admin_makes_a_link_copies_it_and_withdraws_an_open_one', async () => {
    await page(true);
    expect(host.textContent).toContain('Invite teammates');
    // the open links: what's left of each, and until when
    expect(host.textContent).toContain('3 of 5 uses left');
    expect(host.textContent).toContain('Until Dec 2');
    // the members rule lacks handoff: nothing promised yet
    expect(host.textContent).not.toContain('hand off tasks');
    // Make a link: main makes it (and adds handoff to the members rule), the link shows with Copy
    bridge.sphereInvite.mockResolvedValueOnce({ invitations: [{ ...OPEN, id: 'inv-2', uses: 0 }, OPEN], link: MADE, refused: null, why: null, handoffs: true });
    await act(async () => button('Make a link')!.click());
    await settle();
    expect(bridge.sphereInvite).toHaveBeenCalledTimes(1);
    expect(host.querySelector('.invite-made')!.textContent).toBe(MADE);
    expect(host.textContent).toContain('Members can ask, request reviews and hand off tasks.');
    await act(async () => (host.querySelector('[aria-label="Copy the link"]') as HTMLButtonElement).click());
    expect(writeText).toHaveBeenCalledWith(MADE);
    // Withdraw: main withdraws that one, and the list it answers shows
    bridge.sphereWithdraw.mockResolvedValueOnce({ invitations: [{ ...OPEN, id: 'inv-2', uses: 0 }], link: null, refused: null, why: null });
    await act(async () => (host.querySelector('[aria-label="Withdraw the link with 3 of 5 uses left"]') as HTMLButtonElement).click());
    await settle();
    expect(bridge.sphereWithdraw).toHaveBeenCalledWith('inv-1');
    expect(host.textContent).not.toContain('3 of 5 uses left');
    expect(host.textContent).toContain('5 of 5 uses left');
  });

  it('sphereapp_10: a members rule with handoff already says so; a refusal says why and shows no link', async () => {
    await page(true, ['question', 'review_request', 'handoff']);
    expect(host.textContent).toContain('Members can ask, request reviews and hand off tasks.');
    bridge.sphereInvite.mockResolvedValueOnce({ invitations: [], link: null, refused: 'failed', why: 'Making a link takes your own click.' });
    await act(async () => button('Make a link')!.click());
    await settle();
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('Making a link takes your own click.');
    expect(host.querySelector('.invite-made')).toBeNull();
  });
});

describe('SPHEREAPP-11: Make admin, Remove admin, and the sole-admin notice', () => {
  const JOINED = { state: 'joined', url: 'https://sphere.example.com', sphere: 'demo', owner: 'wellington', obiId: 'obi-1' };
  const policyOf = (admins: string[]) => ({ sphere: 'demo', policy: { version: 1, updated_at: '', rules: [] }, admin: admins.includes('wellington'), refused: null, why: null, admins, me: 'wellington' });
  const sphereAdmin = vi.fn();
  async function settings(admins: string[]) {
    answers['GET /v1/obid/sphere'] = JOINED;
    answers['GET /v1/obid/sphere/members'] = [{ owner: 'dana' }, { owner: 'ernesto' }];
    sphereAdmin.mockReset();
    Object.assign((window as unknown as { app: Record<string, unknown> }).app, {
      onSphere: () => () => {}, spherePolicy: vi.fn(async () => policyOf(admins)), sphereAdmin,
      sphereInvitations: vi.fn(async () => ({ invitations: [], link: null, refused: null, why: null })),
    });
    await import('../spheres');
    await settle();
    const { default: SphereSettings } = await import('../SphereSettings');
    await act(async () => root.render(<SphereSettings id="live:demo" leaving={false} onClose={() => {}} onGone={() => {}} />));
    await settle();
  }
  const roleOf = (owner: string) => [...host.querySelectorAll('.set-member')].find((r) => r.querySelector('.set-label')?.textContent?.startsWith(owner))?.querySelector('.set-role')?.textContent;

  it('sphereapp_11_a_sole_admin_is_told_and_makes_a_member_admin', async () => {
    await settings(['wellington']);
    expect(host.textContent).toContain('You’re the only admin of demo. Make someone else an admin so it never hangs on one Mac.');
    expect(roleOf('wellington')).toBe('Admin');
    expect(roleOf('dana')).toBe('Member');
    sphereAdmin.mockResolvedValueOnce({ admins: ['dana', 'wellington'], refused: null, why: null });
    await act(async () => (host.querySelector('[aria-label="Make dana an admin"]') as HTMLButtonElement).click());
    await settle();
    expect(sphereAdmin).toHaveBeenCalledWith('dana', true);
    expect(roleOf('dana')).toBe('Admin');
    // no longer the only one: the notice goes, and dana's row offers Remove admin
    expect(host.textContent).not.toContain('only admin');
    sphereAdmin.mockResolvedValueOnce({ admins: ['wellington'], refused: null, why: null });
    await act(async () => (host.querySelector('[aria-label="Remove dana as admin"]') as HTMLButtonElement).click());
    await settle();
    expect(sphereAdmin).toHaveBeenLastCalledWith('dana', false);
    expect(roleOf('dana')).toBe('Member');
  });

  it('sphereapp_11: a refusal says why; a member who isn’t admin gets no buttons and no notice', async () => {
    await settings(['wellington']);
    sphereAdmin.mockResolvedValueOnce({ admins: null, refused: 'failed', why: 'ernesto hasn’t joined with their own key yet.' });
    await act(async () => (host.querySelector('[aria-label="Make ernesto an admin"]') as HTMLButtonElement).click());
    await settle();
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('ernesto hasn’t joined with their own key yet.');
    expect(roleOf('ernesto')).toBe('Member');
    await act(async () => root.unmount());
    root = createRoot(host);
    vi.resetModules();
    await settings(['dana']);
    expect(host.querySelector('[aria-label="Make ernesto an admin"]')).toBeNull();
    expect(host.textContent).not.toContain('only admin');
    expect(roleOf('dana')).toBe('Admin');
    expect(roleOf('wellington')).toBe('Member');
  });

  it('sphereapp_11: Settings › Obysphere tells a sole admin too, above Invite teammates', async () => {
    answers['GET /v1/obid/sphere'] = JOINED;
    answers['GET /v1/obid/sphere/members'] = [{ owner: 'dana' }];
    Object.assign((window as unknown as { app: Record<string, unknown> }).app, {
      onSphere: () => () => {}, spherePolicy: vi.fn(async () => policyOf(['wellington'])),
      sphereInvitations: vi.fn(async () => ({ invitations: [], link: null, refused: null, why: null })),
    });
    const { SphereJoinPage } = await import('../SphereSettings');
    await act(async () => root.render(<SphereJoinPage />));
    await settle();
    expect(host.textContent).toContain('You’re the only admin of demo.');
  });
});

describe('SPHEREAPP-12: Sign in to get back in, beside the invite link', () => {
  const sphereRebindOffer = vi.fn();
  const sphereRebind = vi.fn();
  async function step(offered: boolean) {
    sphereRebindOffer.mockReset(); sphereRebind.mockReset();
    sphereRebindOffer.mockImplementation(async (a: string) => (a.includes('demo') ? { sphere: 'demo', host: 'sphere.example.com', providers: offered ? [{ id: 'google', name: 'Google' }] : null } : null));
    Object.assign((window as unknown as { app: Record<string, unknown> }).app, { sphereRebindOffer, sphereRebind });
    vi.stubGlobal('location', { search: '?onboarding=sphere' });
    const { default: FirstRun } = await import('../FirstRun');
    await act(async () => root.render(<FirstRun />));
    await settle();
  }

  it('sphereapp_12_an_address_whose_obysphere_offers_it_shows_sign_in_and_gets_back_in', async () => {
    await step(true);
    // its address, not a link: no Join, no problem, and the way back in
    await type(field('Invite link'), 'sphere.example.com/demo');
    await settle();
    expect(button('Join')!.disabled).toBe(true);
    expect(host.querySelector('[role="alert"]')).toBeNull();
    expect(host.querySelector('.join-link-back')!.textContent).toBe('Lost the Mac you joined from? Sign in with Google to get back in.');
    sphereRebind.mockResolvedValueOnce(null);
    await act(async () => button('Sign in with Google')!.click());
    await settle();
    expect(sphereRebind).toHaveBeenCalledWith('sphere.example.com/demo', 'google');
    expect(host.querySelector('[role="status"]')!.textContent).toBe('You’re back in demo.');
  });

  it('sphereapp_12: an Obysphere that doesn’t offer it shows no sign-in; a refusal says why', async () => {
    await step(false);
    await type(field('Invite link'), LINK);
    await settle();
    expect(host.querySelector('.join-link-back')).toBeNull();
    await act(async () => root.unmount());
    root = createRoot(host);
    vi.resetModules();
    await step(true);
    await type(field('Invite link'), LINK);
    await settle();
    sphereRebind.mockResolvedValueOnce('That sign-in can’t get you back in. Use the account you joined with, or one from a domain this Obysphere allows.');
    await act(async () => button('Sign in with Google')!.click());
    await settle();
    expect(host.querySelector('[role="alert"]')!.textContent).toContain('That sign-in can’t get you back in.');
  });
});
