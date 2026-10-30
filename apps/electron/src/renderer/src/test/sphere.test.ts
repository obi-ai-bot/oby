import { createPublicKey, generateKeyPairSync, verify, type KeyObject } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join as path } from 'node:path';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  ACCEPTS, SCOPES, canonical, join, keepMembership, keyIdOf, leave, openedInviteLink, parseInvite, publicOf, readInviteLink, type Answer, type Call,
} from '../../../shared/sphere';

// docs/contracts/obysphere.md: each test is named after its scenario. SPHERE-5 (two real obids on
// one local Obysphere) is scripts/test-sphere.mjs.

// obi-sphere's known answers for invite links (spec/conformance/invite-link-vectors.json at
// bf51687c, obi-sphere's dev), copied as they are
const VECTORS = JSON.parse(readFileSync(path(__dirname, 'fixtures/invite-link-vectors.json'), 'utf8')) as {
  scheme: string; valid: { name: string; link: string; fields: Record<string, string> }[];
  invalid: { name: string; link: string }[]; opened: { name: string; opened: string; link: string | null }[];
};
const SECRET = 'A'.repeat(43), ID = '9187a87a-3126-424c-8861-8811bba1a5ec';
const OPEN = `https://sphere.example.com/join#sphere=demo&name=Demo%20team&by=dmytro&id=${ID}&secret=${SECRET}`;
const NAMED = `https://sphere.example.com/join#sphere=demo&for=wellington&obi=obi-wellington&id=${ID}&secret=${SECRET}`;

// A JWS part, decoded
const part = (s: string) => JSON.parse(Buffer.from(s, 'base64url').toString());
const verifies = (input: string, signature: string, key: KeyObject) => verify(null, Buffer.from(input), createPublicKey(key), Buffer.from(signature, 'base64url'));

// obid as main reaches it, stood in for: what it was asked, and its answers by path
function fakeObid(owner = 'wellington', state = 'none', refuse?: Record<string, Answer>) {
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const call: Call = async (method, p, body) => {
    calls.push({ method, path: p, body });
    if (refuse?.[`${method} ${p}`]) return refuse[`${method} ${p}`];
    if (method === 'GET' && p === '/v1/obid/sphere') return { status: 200, text: JSON.stringify({ state, owner, url: 'https://sphere.example.com', sphere: 'demo', obiId: 'obi-x' }) };
    if (p === '/v1/obid/sphere/card') {
      const b = body as { obiId: string; ownerKey: string };
      return { status: 200, text: JSON.stringify({ card: { name: 'x', params: { obi_id: b.obiId, owner, owner_key: b.ownerKey } } }) };
    }
    if (p === '/v1/obid/sphere/join') return { status: 200, text: JSON.stringify({ state: 'joined' }) };
    return { status: 200, text: JSON.stringify({ state: 'none' }) };
  };
  return { call, calls, sent: (p: string) => calls.find((c) => c.path === p)?.body as Record<string, unknown> };
}

let key: KeyObject;
beforeAll(() => { key = generateKeyPairSync('ed25519').privateKey; });

describe('obysphere', () => {
  it('sphere_1_a_private_link_joins_with_a_card_and_delegation_the_owner_signed', async () => {
    const obid = fakeObid();
    const invite = parseInvite(OPEN)!;
    expect(await join(obid.call, key, invite)).toEqual({ state: 'joined' });
    expect(obid.calls.map((c) => `${c.method} ${c.path}`)).toEqual([
      'GET /v1/obid/sphere', 'POST /v1/obid/sphere/key', 'POST /v1/obid/sphere/card', 'POST /v1/obid/sphere/join',
    ]);
    // an open link's Oby is obi-<a new UUID>, a name its owner's
    const asked = obid.sent('/v1/obid/sphere/card');
    expect(asked.obiId).toMatch(/^obi-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(asked).toMatchObject({ url: 'https://sphere.example.com', sphere: 'demo', ownerKey: publicOf(key), accepts: ACCEPTS, name: 'wellington’s Oby' });
    const sent = obid.sent('/v1/obid/sphere/join') as { invitation: unknown; card: Record<string, unknown> & { signatures: { protected: string; signature: string }[] }; delegation: string };
    expect(sent.invitation).toEqual({ id: ID, secret: SECRET });
    // the card: one detached JWS over its canonical JSON, by the owner key
    const { signatures, ...card } = sent.card;
    expect(signatures).toHaveLength(1);
    expect(part(signatures[0].protected)).toEqual({ alg: 'EdDSA', kid: keyIdOf(key), typ: 'JOSE' });
    expect(verifies(`${signatures[0].protected}.${Buffer.from(canonical(card)).toString('base64url')}`, signatures[0].signature, key)).toBe(true);
    // the delegation: to that Oby, for this Obysphere, 30 days, receive and reply among its scopes
    const [h, c, s] = sent.delegation.split('.');
    expect(verifies(`${h}.${c}`, s, key)).toBe(true);
    const claims = part(c);
    expect(claims).toMatchObject({ iss: 'wellington', sub: 'wellington', aud: 'obi-sphere:demo', scope: SCOPES, may_act: { sub: asked.obiId } });
    expect(claims.exp - claims.iat).toBe(2_592_000);
    expect(SCOPES.split(' ')).toEqual(expect.arrayContaining(['receive', 'reply']));
    // a named link's Oby is the one it names
    const named = fakeObid();
    await join(named.call, key, parseInvite(NAMED)!);
    expect(named.sent('/v1/obid/sphere/card').obiId).toBe('obi-wellington');
    // left after a revoke: obid forgets the old Sphere first
    const revoked = fakeObid('wellington', 'revoked');
    await join(revoked.call, key, invite);
    expect(revoked.calls[1]).toMatchObject({ method: 'DELETE', path: '/v1/obid/sphere' });
  });

  it('sphere_1_the_canonical_json_orders_keys_and_escapes_as_rfc_8785', () => {
    expect(canonical({ b: 1, a: [true, null, 'é\n"'], A: { z: 0 } })).toBe('{"A":{"z":0},"a":[true,null,"é\\n\\""],"b":1}');
  });

  it('sphere_2_invite_links_read_as_the_spec_known_answers_say', () => {
    for (const v of VECTORS.valid) expect(readInviteLink(v.link), v.name).toEqual(v.fields);
    for (const v of VECTORS.invalid) expect(readInviteLink(v.link), v.name).toBeNull();
    for (const v of VECTORS.opened) expect(openedInviteLink(v.opened, VECTORS.scheme), v.name).toBe(v.link);
    // Oby's own scheme stands for https, never http
    expect(parseInvite(`oby://join/sphere.example.com#sphere=demo&id=${ID}&secret=${SECRET}`)?.url).toBe('https://sphere.example.com');
    expect(parseInvite(`OBY://join/127.0.0.1:8787#sphere=demo&id=${ID}&secret=${SECRET}`)?.url).toBe('https://127.0.0.1:8787');
    expect(parseInvite('oby://claim/sphere.example.com#code')).toBeNull();
  });

  it('sphere_2_a_link_this_oby_cant_use_joins_nothing_and_says_why', async () => {
    // a named link for another owner
    const other = fakeObid('ernesto');
    await expect(join(other.call, key, parseInvite(NAMED)!)).rejects.toThrow('This invite is for wellington, and this Mac’s Oby acts for ernesto.');
    expect(other.calls.map((c) => c.method)).toEqual(['GET']);
    // a link for one Google Workspace: no sign-in in this app yet
    const domain = fakeObid();
    await expect(join(domain.call, key, parseInvite(`${OPEN}&domain=deskree.com`)!)).rejects.toThrow('This link is for people at deskree.com');
    expect(domain.calls.map((c) => c.method)).toEqual(['GET']);
    // the Sphere's refusal, in plain words
    const used = fakeObid('wellington', 'none', { 'POST /v1/obid/sphere/join': { status: 502, text: JSON.stringify({ error: 'The Sphere refused it: invitation_invalid: used up' }) } });
    await expect(join(used.call, key, parseInvite(OPEN)!)).rejects.toThrow('This link has expired, was used up or was cancelled.');
    // obid not answering
    const down: Call = async () => ({ status: 0, text: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' });
    await expect(join(down, key, parseInvite(OPEN)!)).rejects.toThrow('obid isn’t running');
  });

  it('sphere_4_leaving_takes_this_oby_out_of_the_obysphere_then_obid_forgets_it', async () => {
    const obid = fakeObid('wellington', 'joined');
    const http = vi.fn(async (url: string, init: RequestInit) => {
      if (url.endsWith('/v0.1/oauth/token')) {
        const form = new URLSearchParams(String(init.body));
        expect(init.redirect).toBe('error');
        expect(part(form.get('subject_token')!.split('.')[1])).toMatchObject({ sub: 'wellington', aud: 'obi-sphere:demo', scope: 'directory:read' });
        return new Response(JSON.stringify({ access_token: 'owner-token', token_type: 'Bearer' }));
      }
      expect(init).toMatchObject({ method: 'DELETE', headers: { authorization: 'Bearer owner-token' } });
      return new Response('{}');
    });
    expect((await leave(obid.call, key, http as unknown as typeof fetch)).untold).toBeNull();
    expect(http.mock.calls.map((c) => c[0])).toEqual(['https://sphere.example.com/v0.1/oauth/token', 'https://sphere.example.com/v0.1/spheres/demo/members/obi-x']);
    expect(obid.calls.at(-1)).toMatchObject({ method: 'DELETE', path: '/v1/obid/sphere' });
    // the Obysphere can't be told: obid leaves all the same, and says so
    const away = fakeObid('wellington', 'joined');
    const r = await leave(away.call, key, (async () => { throw new Error('offline'); }) as unknown as typeof fetch);
    expect(r.untold).toBe('offline');
    expect(away.calls.at(-1)).toMatchObject({ method: 'DELETE', path: '/v1/obid/sphere' });
  });

  it('sphere_6_the_card_obid_asks_for_and_a_delegation_near_its_end_are_signed_again', async () => {
    const at = Date.parse('2026-10-30T12:00:00Z');
    const membership = (sealing: string, expiresAt: string) => ({
      state: 'joined', owner: 'wellington', url: 'https://sphere.example.com', sphere: 'demo', obiId: 'obi-x',
      sealing, accepts: ACCEPTS, card: { name: 'Wellington’s Oby' }, delegation: { scope: SCOPES, expiresAt },
    });
    const obid = (m: unknown) => {
      const calls: { method: string; path: string; body?: Record<string, unknown> }[] = [];
      const call: Call = async (method, p, body) => {
        calls.push({ method, path: p, body: body as Record<string, unknown> });
        if (p === '/v1/obid/sphere/card') return { status: 200, text: JSON.stringify({ card: { name: (body as { name: string }).name, seal_public_key: 'k' } }) };
        return { status: 200, text: JSON.stringify(method === 'GET' ? m : {}) };
      };
      return { call, calls };
    };
    let asked = 0;
    const owner = () => { asked++; return key; };
    // nothing to sign: nothing sent, and no owner key asked for
    const quiet = obid(membership('on', '2026-11-20T12:00:00Z'));
    expect(await keepMembership(quiet.call, owner, at)).toEqual([]);
    expect(quiet.calls).toHaveLength(1);
    expect(asked).toBe(0);
    // card_needed: the same name and accepts, signed, through PUT /v1/obid/sphere/membership
    const sealing = obid(membership('card_needed', '2026-11-20T12:00:00Z'));
    expect(await keepMembership(sealing.call, owner, at)).toEqual(['card']);
    expect(sealing.calls[1].body).toMatchObject({ obiId: 'obi-x', name: 'Wellington’s Oby', accepts: ACCEPTS, ownerKey: publicOf(key) });
    const put = sealing.calls[2];
    expect(put).toMatchObject({ method: 'PUT', path: '/v1/obid/sphere/membership' });
    const { signatures, ...card } = put.body!.card as Record<string, unknown> & { signatures: { protected: string; signature: string }[] };
    expect(verifies(`${signatures[0].protected}.${Buffer.from(canonical(card)).toString('base64url')}`, signatures[0].signature, key)).toBe(true);
    // 5 days or fewer left: a new delegation with the same scope, 30 days from now
    const ending = obid(membership('on', '2026-11-04T12:00:00Z'));
    expect(await keepMembership(ending.call, owner, at)).toEqual(['delegation']);
    const claims = part((ending.calls[1].body!.delegation as string).split('.')[1]);
    expect(claims).toMatchObject({ sub: 'wellington', aud: 'obi-sphere:demo', scope: SCOPES, may_act: { sub: 'obi-x' }, iat: at / 1000 });
    // not joined: nothing
    expect(await keepMembership(obid({ state: 'none' }).call, owner, at)).toEqual([]);
  });
});

describe('obysphere in the side panel', () => {
  let spheres: typeof import('../spheres');
  beforeAll(async () => {
    vi.stubGlobal('location', { search: '' });
    vi.stubGlobal('window', {});
    spheres = await import('../spheres');
  });

  it('sphere_3_the_joined_obysphere_shows_with_the_other_obys_owners', () => {
    const joined = { state: 'joined', url: 'https://sphere.example.com', sphere: 'demo' };
    const s = spheres.liveSphere(joined, [{ owner: 'dmytro' }, { owner: 'ernesto' }], { demo: 'Demo team' })!;
    expect(s).toMatchObject({ id: 'live:demo', name: 'Demo team', admin: false, members: ['dmytro', 'ernesto'], invited: [], live: true });
    expect(spheres.urlOf(s)).toBe('sphere.example.com');
    expect(spheres.memberCount(s)).toBe(3);
    // its id when the link's name isn't kept; nothing when obid isn't in one
    expect(spheres.liveSphere(joined, [], {})!.name).toBe('demo');
    expect(spheres.liveSphere({ state: 'none', url: null, sphere: null }, [], {})).toBeNull();
    expect(spheres.liveSphere({ ...joined, state: 'revoked' }, [], {})).toBeNull();
  });
});
