// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { OwnerPin, PeerPost, RememberedDecision, RoomPost } from '../obid';
import { owners } from '../../../shared/words';

// Whom the owner trusts in their Obysphere, and what their Oby does without asking, in Settings ›
// Obysphere (docs/contracts/obysphere.md, SPHERE-11 to 13), as it really renders, on an obid that
// answers from here: a member's fingerprint and Verify, the look-alike mark there and on their
// thread, and the remembered decisions with Forget.
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));
vi.mock('../Composer', () => ({ default: () => <div data-box />, typingIn: () => false }));

type Answer = { status: number; text: string };
const calls: { method: string; path: string; body?: unknown }[] = [];
const routes: Record<string, () => Answer> = {};
const ok = (value: unknown, status = 200) => () => ({ status, text: value === undefined ? '' : JSON.stringify(value) });
const posted = () => calls.filter((c) => c.method !== 'GET');

let trust: typeof import('../SphereSettings');
let PeerThreadView: typeof import('../PeerThread').default;
let RoomThread: typeof import('../RoomView').RoomThread;
let room: typeof import('../room');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  window.app = { obid: { call: async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    return routes[`${method} ${path}`]?.() ?? { status: 404, text: '{"error":"Not here."}' };
  } } } as never;
  trust = await import('../SphereSettings');
  PeerThreadView = (await import('../PeerThread')).default;
  RoomThread = (await import('../RoomView')).RoomThread;
  room = await import('../room');
});
afterAll(() => vi.unstubAllGlobals());

let host: HTMLElement, root: Root;
beforeEach(() => {
  calls.length = 0;
  for (const k of Object.keys(routes)) delete routes[k];
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; });

const show = async (node: React.ReactNode) => { await act(async () => root.render(node)); await settle(); };
const settle = () => act(async () => { for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0)); });
const click = async (el: Element | null | undefined) => {
  expect(el).toBeTruthy();
  await act(async () => (el as HTMLElement).click());
  await settle();
};
const button = (label: string, within: ParentNode = document.body) =>
  [...within.querySelectorAll('button')].find((b) => b.textContent === label || b.getAttribute('aria-label') === label);
const rowOf = (text: string) => [...host.querySelectorAll('.set-row')].find((r) => r.querySelector('.set-label')?.firstChild?.textContent === text);

const FP = 'ed25519:Kq3v9Lr2xW8mPz4TnY7cBd6Fh1Js5Ga0';
const pin = (more: Partial<OwnerPin> = {}): OwnerPin => ({ owner: 'dima', fingerprint: FP, firstSeen: '2026-11-03T12:00:00Z', changedTo: null, verified: false, ...more });
const post = (verified: boolean): PeerPost => ({
  id: 'p1', seq: 1, recordedAt: '2026-11-03T12:00:09Z', from: 'obi-dima', owner: 'dima', kind: 'answer', by: 'obi', createdAt: '2026-11-03T12:00:08Z', replyTo: null,
  text: 'Fixing the login', about: [], state: 'sent', error: null, verified, direction: 'received', status: null, reason: null,
  exchange: { id: 'x1', type: 'question', direction: 'outgoing', state: 'done', waitingOnThem: false, quietAt: null },
});
const roomPost = (verified: boolean): RoomPost => ({
  id: 'r1', seq: 4, recordedAt: '2026-11-03T12:00:09Z', from: 'obi-dima', owner: 'dima', kind: 'found', by: 'obi', createdAt: '2026-11-03T12:00:08Z', replyTo: null,
  text: 'Billing retries goes out first.', about: [], state: 'sent', error: null, verified,
});
const threadOf = (verified: boolean, more: object = {}) => ok({ owner: 'dima', name: 'Dima', member: true, takesQuestions: true, verified, posts: [post(verified)], ...more });

describe('a member’s key', () => {
  it('sphere_11_verify_drops_not_verified', async () => {
    routes['GET /v1/obid/sphere/pins'] = ok([pin()]);
    routes['GET /v1/obid/sphere/members'] = ok([{ obi: 'obi-dima', owner: 'dima', verified: false }]);
    routes['GET /v1/obid/sphere/peers/dima'] = threadOf(false);
    routes['GET /v1/spheres/deskree/room?after=0'] = ok({ room: 'available', posts: [roomPost(false)] });
    // their thread and the room, as read before: Not verified
    const threadHost = document.createElement('div');
    document.body.append(threadHost);
    const threadRoot = createRoot(threadHost);
    await act(async () => threadRoot.render(<PeerThreadView owner="dima" leaving={false} onClose={() => {}} onGone={() => {}} />));
    await room.lookAtRoom('deskree');
    await settle();
    expect(threadHost.textContent).toContain('Not verified');
    const roomHtml = () => { const h = document.createElement('div'); const r = createRoot(h); act(() => r.render(<RoomThread posts={room.roomNow().posts} />)); return h.textContent ?? ''; };
    expect(roomHtml()).toContain('Not verified');

    // Settings: the fingerprint to compare, whole, and Verify
    await show(<trust.MembersKeys />);
    const row = rowOf('dima')!;
    expect(row.querySelector('.set-fingerprint')!.textContent).toBe(FP);
    expect(row.textContent).toContain('Not verified');
    // Verify asks first, naming them; Cancel tells obid nothing
    await click(button('Verify dima'));
    const dialog = document.body.querySelector('[role="alertdialog"]')!;
    expect(dialog.querySelector('h2')!.textContent).toBe('Did dima read you this?');
    expect(dialog.querySelector('.set-fingerprint')!.textContent).toBe(FP);
    await click(button('Cancel', dialog));
    expect(document.body.querySelector('[role="alertdialog"]')).toBeNull();
    expect(posted()).toEqual([]);

    // they read it, and it's this one: obid gets exactly that fingerprint, once
    routes['POST /v1/obid/sphere/pins/dima'] = ok({ owner: 'dima', fingerprint: FP, verified: true });
    await click(button('Verify dima'));
    routes['GET /v1/obid/sphere/pins'] = ok([pin({ verified: true })]);
    routes['GET /v1/obid/sphere/peers/dima'] = threadOf(true);
    routes['GET /v1/spheres/deskree/room?after=0'] = ok({ room: 'available', posts: [roomPost(true)] });
    const verify = button('Verify', document.body.querySelector('[role="alertdialog"]')!)!;
    await click(verify);
    expect(posted()).toEqual([{ method: 'POST', path: '/v1/obid/sphere/pins/dima', body: { fingerprint: FP } }]);
    // that request is the owner's own, signed by the dialog's Verify label alone (WORDS-19, SPHERE-9)
    expect(owners('POST', '/v1/obid/sphere/pins/dima', { fingerprint: FP }, [verify.textContent!], [])).toEqual([0]);
    expect(document.body.querySelector('[role="alertdialog"]')).toBeNull();
    expect(rowOf('dima')!.textContent).toContain('Verified');
    expect(button('Verify dima')).toBeUndefined();
    // the thread and the room, read again whole, drop Not verified
    await act(async () => threadRoot.render(<PeerThreadView owner="dima" leaving={false} onClose={() => {}} onGone={() => {}} />));
    await settle();
    expect(threadHost.textContent).toContain('Fixing the login');
    expect(threadHost.textContent).not.toContain('Not verified');
    expect(roomHtml()).toContain('Billing retries goes out first.');
    expect(roomHtml()).not.toContain('Not verified');
    await act(async () => threadRoot.unmount());
  });

  it('sphere_11_a_changed_key_compares_the_new_one_and_a_refusal_says_why', async () => {
    const NEW = 'ed25519:Zz9y8X7w6V5u4T3s2R1q0P';
    routes['GET /v1/obid/sphere/pins'] = ok([pin({ verified: true, changedTo: NEW })]);
    routes['GET /v1/obid/sphere/members'] = ok([]);
    await show(<trust.MembersKeys />);
    expect(rowOf('dima')!.querySelector('.set-fingerprint')!.textContent).toBe(NEW);
    expect(rowOf('dima')!.textContent).toContain('Their key changed');
    routes['POST /v1/obid/sphere/pins/dima'] = ok({ error: 'That isn’t the key dima has now.' }, 422);
    await click(button('Verify dima'));
    await click(button('Verify', document.body.querySelector('[role="alertdialog"]')!));
    expect(posted()).toEqual([{ method: 'POST', path: '/v1/obid/sphere/pins/dima', body: { fingerprint: NEW } }]);
    expect(document.body.querySelector('[role="alertdialog"] [role="alert"]')!.textContent).toBe('That isn’t the key dima has now.');
  });

  it('sphere_12_a_lookalike_is_marked', async () => {
    routes['GET /v1/obid/sphere/pins'] = ok([pin(), pin({ owner: 'dirna' }), pin({ owner: 'ana' })]);
    routes['GET /v1/obid/sphere/members'] = ok([
      { obi: 'obi-ana', owner: 'ana' },
      { obi: 'obi-dima', owner: 'dima', lookalike: ['dirna', 'dіma'] },
      { obi: 'obi-dirna', owner: 'dirna', lookalike: ['dima'] },
    ]);
    await show(<trust.MembersKeys />);
    // the mark names the other id, a letter from another alphabet as its code
    expect(rowOf('dima')!.querySelector('.set-lookalike')!.textContent).toContain('Not dirna’s or d\\u{456}ma’s Oby');
    expect(rowOf('dirna')!.querySelector('.set-lookalike')!.textContent).toContain('Not dima’s Oby');
    expect(rowOf('ana')!.querySelector('.set-lookalike')).toBeNull();
    // and at the head of their thread
    routes['GET /v1/obid/sphere/peers/dima'] = threadOf(false, { lookalike: ['dirna'] });
    await show(<PeerThreadView owner="dima" leaving={false} onClose={() => {}} onGone={() => {}} />);
    expect(host.querySelector('.peer-lookalike')!.textContent).toBe('Not dirna’s Oby');
    routes['GET /v1/obid/sphere/peers/ana'] = ok({ owner: 'ana', name: 'ana', member: true, takesQuestions: true, posts: [] });
    await show(<PeerThreadView owner="ana" leaving={false} onClose={() => {}} onGone={() => {}} />);
    expect(host.querySelector('.peer-lookalike')).toBeNull();
  });
});

describe('what your Oby does without asking', () => {
  const kept = (id: string, more: Partial<RememberedDecision>): RememberedDecision => ({
    id, type: 'review_request', fromOwner: 'ernesto', repo: 'obi-ai-bot/tetrix', decision: 'accept', created: '2026-11-03T12:00:00Z', uses: 0, lastUsed: null, ...more,
  });
  it('sphere_13_forget_asks_again', async () => {
    const approval = kept('d-1a2b3c4d5e6f', { type: 'host_approval', uses: 2, lastUsed: '2026-11-04T12:00:00Z' });
    const reviews = kept('d-000000000001', {});
    const no = kept('d-000000000002', { type: 'question', decision: 'decline', fromOwner: 'dіma', repo: '*' });
    routes['GET /v1/obid/decisions'] = ok([approval, reviews, no]);
    await show(<trust.WithoutAsking />);
    // in words: who by exact owner id, what and where
    expect(rowOf('Approves ernesto’s pull requests on GitHub on obi-ai-bot/tetrix')!.textContent).toContain('When your Oby’s review passes and their code owners approved · Used 2 times');
    expect(rowOf('Reviews ernesto’s requests on obi-ai-bot/tetrix')!.textContent).toContain('Not used yet');
    expect(rowOf('Says no to d\\u{456}ma’s questions')).toBeTruthy();
    // Forget: obid forgets it, once, and the list is read again without it
    routes['DELETE /v1/obid/decisions/d-1a2b3c4d5e6f'] = ok(undefined, 204);
    routes['GET /v1/obid/decisions'] = ok([reviews, no]);
    await click(button('Forget Approves ernesto’s pull requests on GitHub on obi-ai-bot/tetrix'));
    expect(posted()).toEqual([{ method: 'DELETE', path: '/v1/obid/decisions/d-1a2b3c4d5e6f', body: undefined }]);
    expect(rowOf('Approves ernesto’s pull requests on GitHub on obi-ai-bot/tetrix')).toBeUndefined();
    expect(rowOf('Reviews ernesto’s requests on obi-ai-bot/tetrix')).toBeTruthy();
    // obid then asks the owner again for the next such request: no decision covers it
    // (core/crates/obid/tests/sphere_api.rs, a_request_waits_for_the_owner_and_the_remembered_decision_answers_the_next)
  });

  it('sphere_13_one_already_forgotten_says_why_and_the_list_is_read_again', async () => {
    routes['GET /v1/obid/decisions'] = ok([kept('d-1a2b3c4d5e6f', {})]);
    await show(<trust.WithoutAsking />);
    routes['DELETE /v1/obid/decisions/d-1a2b3c4d5e6f'] = ok({ error: 'No such decision.' }, 404);
    routes['GET /v1/obid/decisions'] = ok([]);
    await click(button('Forget Reviews ernesto’s requests on obi-ai-bot/tetrix'));
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('No such decision.');
    expect(rowOf('Nothing yet')!.textContent).toContain('Every request from another Oby waits for you.');
  });
});
