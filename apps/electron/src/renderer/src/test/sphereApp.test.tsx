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
const calls: string[] = [];
const call = vi.fn(async (method: string, path: string) => {
  calls.push(`${method} ${path}`);
  const a = answers[`${method} ${path}`];
  return a === undefined ? { status: 404, text: '{"error":"Not found."}' } : { status: 200, text: JSON.stringify(a) };
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
  calls.length = 0;
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
