import { createPublicKey, generateKeyPairSync, verify, type KeyObject } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join as path } from 'node:path';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  ACCEPTS, NotThisKey, SCOPES, addStarterRule, canonical, join, keepMembership, keyIdOf, leave, openedInviteLink, parseInvite, publicOf, readInviteLink, readPolicy,
  starterRule, writePolicy, type Answer, type Call, type PolicyAnswer, type Rule,
} from '../../../shared/sphere';
import { canAddProgress, canChangeRepos, canOpenRoom, confirmIsOff, repoTicks, tickRepo, whatItTells, type SharingRule } from '../sharing';

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
      return { status: 200, text: JSON.stringify({ card: { name: 'x', capabilities: { extensions: [{ params: { obi_id: b.obiId, owner, owner_key: b.ownerKey } }] } } }) };
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
    expect(await join(obid.call, () => key, invite)).toEqual({ state: 'joined' });
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
    await join(named.call, () => key, parseInvite(NAMED)!);
    expect(named.sent('/v1/obid/sphere/card').obiId).toBe('obi-wellington');
    // left after a revoke: obid forgets the old Sphere first
    const revoked = fakeObid('wellington', 'revoked');
    await join(revoked.call, () => key, invite);
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
    // no owner key is made for a link that can't be used
    const never = () => { throw new Error('no key for a refused link'); };
    // a named link for another owner
    const other = fakeObid('ernesto');
    await expect(join(other.call, never, parseInvite(NAMED)!)).rejects.toThrow('This invite is for wellington, and this Mac’s Oby acts for ernesto.');
    expect(other.calls.map((c) => c.method)).toEqual(['GET']);
    // a link for one Google Workspace: no sign-in in this app yet
    const domain = fakeObid();
    await expect(join(domain.call, never, parseInvite(`${OPEN}&domain=deskree.com`)!)).rejects.toThrow('This link is for people at deskree.com');
    expect(domain.calls.map((c) => c.method)).toEqual(['GET']);
    // already in one: a plain sentence, before anything (obid's 409 says the same if it races)
    const inOne = fakeObid('wellington', 'joined');
    await expect(join(inOne.call, never, parseInvite(OPEN)!)).rejects.toThrow('This Oby is already in an Obysphere. Leave it first');
    expect(inOne.calls.map((c) => c.method)).toEqual(['GET']);
    const raced = fakeObid('wellington', 'none', { 'POST /v1/obid/sphere/join': { status: 409, text: JSON.stringify({ error: 'obid has joined demo already (joined).' }) } });
    await expect(join(raced.call, () => key, parseInvite(OPEN)!)).rejects.toThrow('This Oby is already in an Obysphere.');
    // the Sphere's refusal, in plain words
    const used = fakeObid('wellington', 'none', { 'POST /v1/obid/sphere/join': { status: 502, text: JSON.stringify({ error: 'The Sphere refused it: invitation_invalid: used up' }) } });
    await expect(join(used.call, () => key, parseInvite(OPEN)!)).rejects.toThrow('This link has expired, was used up or was cancelled.');
    // obid not answering
    const down: Call = async () => ({ status: 0, text: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' });
    await expect(join(down, () => key, parseInvite(OPEN)!)).rejects.toThrow('obid isn’t running');
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
    expect((await leave(obid.call, () => key, http as unknown as typeof fetch)).untold).toBeNull();
    expect(http.mock.calls.map((c) => c[0])).toEqual(['https://sphere.example.com/v0.1/oauth/token', 'https://sphere.example.com/v0.1/spheres/demo/members/obi-x']);
    expect(obid.calls.at(-1)).toMatchObject({ method: 'DELETE', path: '/v1/obid/sphere' });
    // nothing joined: no owner key is made, nothing reaches a Sphere
    const none = fakeObid('wellington', 'none');
    const nothing = vi.fn();
    expect((await leave(none.call, () => { throw new Error('no key'); }, nothing as unknown as typeof fetch)).untold).toBeNull();
    expect(nothing).not.toHaveBeenCalled();
    // the Obysphere can't be told: obid leaves all the same, and says so
    const away = fakeObid('wellington', 'joined');
    const r = await leave(away.call, () => key, (async () => { throw new Error('offline'); }) as unknown as typeof fetch);
    expect(r.untold).toBe('offline');
    expect(away.calls.at(-1)).toMatchObject({ method: 'DELETE', path: '/v1/obid/sphere' });
  });

  it('sphere_6_the_card_obid_asks_for_and_a_delegation_near_its_end_are_signed_again', async () => {
    const at = Date.parse('2026-10-30T12:00:00Z');
    const membership = (sealing: string, expiresAt: string) => ({
      state: 'joined', owner: 'wellington', url: 'https://sphere.example.com', sphere: 'demo', obiId: 'obi-x',
      sealing, accepts: ACCEPTS, card: { name: 'Wellington’s Oby', capabilities: { extensions: [{ params: { owner_key: publicOf(key) } }] } }, delegation: { scope: SCOPES, expiresAt },
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
    // nothing to sign: nothing sent
    const quiet = obid(membership('on', '2026-11-20T12:00:00Z'));
    expect(await keepMembership(quiet.call, owner, at)).toEqual([]);
    expect(quiet.calls).toHaveLength(1);
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
    // not joined: nothing, and no owner key made
    asked = 0;
    expect(await keepMembership(obid({ state: 'none' }).call, owner, at)).toEqual([]);
    expect(asked).toBe(0);
    // a card another owner key signed (the Swift app's): nothing signed, and words that say what to do
    const swift = { ...membership('card_needed', '2026-11-04T12:00:00Z'), card: { name: 'x', capabilities: { extensions: [{ params: { owner_key: publicOf(generateKeyPairSync('ed25519').privateKey) } }] } } };
    const other = obid(swift);
    await expect(keepMembership(other.call, owner, at)).rejects.toThrow(NotThisKey);
    await expect(keepMembership(other.call, owner, at)).rejects.toThrow('leave demo in its settings, then open its link again.');
    expect(other.calls.every((c) => c.method === 'GET')).toBe(true);
  });
});

// docs/contracts/sphere-policy.md: the Obysphere's rules as main reads and writes them. POLICY-1 and
// 3 also run against a real obi-sphere in scripts/test-sphere.mjs.
const BASE = 'https://sphere.example.com/v0.1';
// The Obysphere as main reaches it, stood in for: its admins and policy, and every request. An
// exchange grants `admin` only to an admin, as obi-sphere does (left out, never an error); onPut
// runs before a PUT is checked, as another admin's write would, or answers in its place.
function fakeSphere(o: { admins?: string[]; version?: number; rules?: Rule[]; onPut?: (p: { version: number; rules: Rule[] }) => Response | void } = {}) {
  const policy = { version: o.version ?? 0, rules: o.rules ?? [] };
  const admins = o.admins ?? ['wellington'];
  const granted = new Map<string, string>();
  const requests: { url: string; init: RequestInit }[] = [];
  const answer = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
  const now = () => ({ version: policy.version, updated_at: '2026-11-14T00:00:00Z', rules: policy.rules });
  const http = vi.fn(async (url: string, init: RequestInit = {}) => {
    requests.push({ url, init });
    if (url === `${BASE}/oauth/token`) {
      const claims = part(new URLSearchParams(String(init.body)).get('subject_token')!.split('.')[1]);
      const scope = (claims.scope as string).split(' ').filter((s) => s !== 'admin' || admins.includes(claims.sub)).join(' ');
      const token = `token-${granted.size + 1}-${claims.jti}`;
      granted.set(token, scope);
      return answer({ access_token: token, token_type: 'Bearer', scope, expires_in: 300 });
    }
    const scope = granted.get(String((init.headers as Record<string, string>).authorization).replace('Bearer ', '')) ?? '';
    if (url === `${BASE}/spheres/demo`) return answer({ sphere: 'demo', admin_owner: 'dmytro', admins });
    if (url !== `${BASE}/spheres/demo/policy`) return answer({}, 404);
    if (init.method !== 'PUT') return answer(now());
    if (!scope.split(' ').includes('admin')) return answer({ error: { code: 'insufficient_scope', message: 'this call needs the scope admin' } }, 403);
    const instead = o.onPut?.(policy);
    if (instead) return instead;
    const body = JSON.parse(String(init.body)) as { expected_version: number; rules: Rule[] };
    if (body.expected_version !== policy.version) return answer({ error: { code: 'conflict', message: `the policy is at version ${policy.version}` } }, 409);
    Object.assign(policy, { version: policy.version + 1, rules: body.rules });
    return answer(now());
  });
  const puts = () => requests.filter((r) => r.init.method === 'PUT');
  // the scope each exchange's assertion asked for, in order
  const asked = () => requests.filter((r) => r.url.endsWith('/oauth/token')).map((r) => part(new URLSearchParams(String(r.init.body)).get('subject_token')!.split('.')[1]));
  return { http: http as unknown as typeof fetch, calls: http, requests, policy, granted, puts, asked };
}
const OTHER: Rule = { id: 'dmytro-reviews', from_owner: 'dmytro', to_owner: '*', actions: ['review_request'], repos: ['acme/app'] };
const THIRD: Rule = { id: 'ernesto-asks', from_owner: 'ernesto', to_owner: 'dmytro', actions: ['question'], repos: ['*'], note: { kept: true } };

describe('the Obysphere\'s rules', () => {
  it('policy_1_the_starter_rule_lets_members_ask', async () => {
    expect(starterRule()).toEqual({ id: 'members-ask', from_owner: '*', to_owner: '*', actions: ['question', 'review_request'], repos: ['*'] });
    expect(starterRule()).not.toBe(starterRule());
    // a new Sphere's policy, [] at version 0: one PUT, and version 1
    const fresh = fakeSphere();
    const r = await addStarterRule(fakeObid('wellington', 'joined').call, () => key, 0, fresh.http);
    expect(r).toMatchObject({ sphere: 'demo', admin: true, refused: null, why: null, policy: { version: 1, rules: [starterRule()] } });
    expect(fresh.puts()).toHaveLength(1);
    expect(JSON.parse(String(fresh.puts()[0].init.body))).toEqual({ expected_version: 0, rules: [starterRule()] });
    // the rules already there are kept first, unchanged and in order
    const ruled = fakeSphere({ version: 3, rules: [structuredClone(OTHER), structuredClone(THIRD)] });
    const kept = await addStarterRule(fakeObid('wellington', 'joined').call, () => key, 3, ruled.http);
    expect(JSON.parse(String(ruled.puts()[0].init.body))).toEqual({ expected_version: 3, rules: [OTHER, THIRD, starterRule()] });
    expect(kept.policy).toMatchObject({ version: 4, rules: [OTHER, THIRD, starterRule()] });
    // already there, by its id: nothing written, the policy as read
    const there = fakeSphere({ version: 2, rules: [OTHER, { ...starterRule(), repos: ['acme/app'] }] });
    const same = await addStarterRule(fakeObid('wellington', 'joined').call, () => key, 2, there.http);
    expect(there.puts()).toEqual([]);
    expect(same).toMatchObject({ refused: null, admin: true, policy: { version: 2, rules: [OTHER, { ...starterRule(), repos: ['acme/app'] }] } });
    // and a member reads it
    expect(await readPolicy(fakeObid('wellington', 'joined').call, () => key, fakeSphere({ admins: ['dmytro'], version: 2, rules: [OTHER] }).http))
      .toMatchObject({ sphere: 'demo', admin: false, refused: null, policy: { version: 2, updated_at: '2026-11-14T00:00:00Z', rules: [OTHER] } });
  });

  it('policy_2_a_member_cannot_write', async () => {
    // the exchange grants only directory:read: no PUT, and words naming the admins
    const member = fakeSphere({ admins: ['dmytro'] });
    const w = await writePolicy(fakeObid('wellington', 'joined').call, () => key, [starterRule()], 0, member.http);
    expect(w).toMatchObject({ sphere: 'demo', policy: null, admin: false, refused: 'not_admin' });
    expect(w.why).toMatch(/admins/);
    expect(member.asked().map((c) => c.scope)).toEqual(['admin directory:read']);
    expect(member.puts()).toEqual([]);
    // the Sphere's own 403 answers the same
    const taken = fakeSphere({ onPut: () => new Response('{}', { status: 403 }) });
    expect(await writePolicy(fakeObid('wellington', 'joined').call, () => key, [], 0, taken.http)).toMatchObject({ refused: 'not_admin', admin: false });
    // the starter rule for an owner not in `admins`: never asked for admin
    const asked = fakeSphere({ admins: ['dmytro'] });
    const a = await addStarterRule(fakeObid('wellington', 'joined').call, () => key, 0, asked.http);
    expect(a).toMatchObject({ refused: 'not_admin', admin: false, policy: { version: 0, rules: [] } });
    expect(a.why).toMatch(/admins/);
    expect(asked.asked().map((c) => c.scope)).toEqual(['directory:read']);
    expect(asked.puts()).toEqual([]);
    // not joined: no owner key, nothing reaches a Sphere
    const never = () => { throw new Error('no key'); };
    const nothing = vi.fn();
    for (const f of [
      () => readPolicy(fakeObid('wellington', 'none').call, never, nothing as unknown as typeof fetch),
      () => writePolicy(fakeObid('wellington', 'none').call, never, [starterRule()], 0, nothing as unknown as typeof fetch),
      () => addStarterRule(fakeObid('wellington', 'none').call, never, 0, nothing as unknown as typeof fetch),
    ]) expect(await f()).toMatchObject({ sphere: null, policy: null, admin: false, refused: 'not_joined' });
    expect(nothing).not.toHaveBeenCalled();
  });

  it('policy_3_a_stale_version_replaces_nothing', async () => {
    // another admin wrote meanwhile: the PUT is 409 conflict, read again with a new directory:read token
    const raced = fakeSphere({ onPut: (p) => { if (p.version === 0) Object.assign(p, { version: 1, rules: [OTHER] }); } });
    const r = await writePolicy(fakeObid('wellington', 'joined').call, () => key, [starterRule()], 0, raced.http);
    expect(r).toMatchObject({ sphere: 'demo', admin: true, refused: 'changed', policy: { version: 1, rules: [OTHER] } });
    expect(r.why).toContain('changed meanwhile');
    expect(raced.policy).toEqual({ version: 1, rules: [OTHER] });
    expect(raced.puts()).toHaveLength(1);
    expect(raced.asked().map((c) => c.scope)).toEqual(['admin directory:read', 'directory:read']);
    const [adminToken, readToken] = [...raced.granted.keys()];
    const after = raced.requests.slice(raced.requests.indexOf(raced.puts()[0]) + 1);
    expect(after.map((q) => `${q.init.method ?? 'GET'} ${q.url}`)).toEqual([
      `POST ${BASE}/oauth/token`, `GET ${BASE}/spheres/demo`, `GET ${BASE}/spheres/demo/policy`,
    ]);
    for (const q of after.slice(1)) expect((q.init.headers as Record<string, string>).authorization).toBe(`Bearer ${readToken}`);
    expect(readToken).not.toBe(adminToken);
    // a conflict at the same version (a rule naming an owner who hasn't joined): the Sphere's words
    const said = 'the policy names nobody, who hasn’t joined this Sphere: it can name only owners who have, or *';
    const named = fakeSphere({ onPut: () => new Response(JSON.stringify({ error: { code: 'conflict', message: said } }), { status: 409 }) });
    const f = await writePolicy(fakeObid('wellington', 'joined').call, () => key, [{ ...starterRule(), from_owner: 'nobody' }], 0, named.http);
    expect(f).toMatchObject({ refused: 'failed', why: said, policy: { version: 0, rules: [] } });
    // the starter rule at a version the page read before a change: nothing asked for admin
    const moved = fakeSphere({ version: 5, rules: [OTHER] });
    const m = await addStarterRule(fakeObid('wellington', 'joined').call, () => key, 4, moved.http);
    expect(m).toMatchObject({ refused: 'changed', policy: { version: 5, rules: [OTHER] } });
    expect(m.why).toContain('changed meanwhile');
    expect(moved.asked().map((c) => c.scope)).toEqual(['directory:read']);
    expect(moved.puts()).toEqual([]);
  });

  it('policy_4_the_owner_key_stays_in_main', async () => {
    const sphere = fakeSphere();
    const answers: PolicyAnswer[] = [];
    let used = 0;
    const owner = () => { used++; return key; };
    answers.push(await writePolicy(fakeObid('wellington', 'joined').call, owner, [OTHER], 0, sphere.http));
    answers.push(await writePolicy(fakeObid('wellington', 'joined').call, owner, [OTHER, starterRule()], 1, sphere.http));
    answers.push(await readPolicy(fakeObid('wellington', 'joined').call, owner, sphere.http));
    expect(answers.map((a) => a.refused)).toEqual([null, null, null]);
    expect(sphere.policy.version).toBe(2);
    // every call: no redirect
    for (const q of sphere.requests) expect(q.init.redirect, q.url).toBe('error');
    // two writes, two exchanges with their own jti; each admin token on exactly one request, the PUT
    const writes = sphere.asked().filter((c) => c.scope === 'admin directory:read');
    expect(writes).toHaveLength(2);
    expect(writes[0].jti).not.toBe(writes[1].jti);
    const adminTokens = [...sphere.granted].filter(([, scope]) => scope.split(' ').includes('admin')).map(([t]) => t);
    expect(adminTokens).toHaveLength(2);
    for (const t of adminTokens) {
      const carried = sphere.requests.filter((q) => (q.init.headers as Record<string, string>)?.authorization === `Bearer ${t}`);
      expect(carried.map((q) => q.init.method)).toEqual(['PUT']);
    }
    expect(used).toBe(3);
    // no answer carries a token or an assertion, failures included
    const assertions = sphere.requests.filter((q) => q.url.endsWith('/oauth/token')).map((q) => new URLSearchParams(String(q.init.body)).get('subject_token')!);
    const offline = await writePolicy(fakeObid('wellington', 'joined').call, () => key, [], 0, (async () => { throw new Error('offline'); }) as unknown as typeof fetch);
    expect(offline).toMatchObject({ refused: 'failed', why: 'offline' });
    const noToken = await readPolicy(fakeObid('wellington', 'joined').call, () => key, (async () => new Response('{"error":"invalid_grant"}', { status: 400 })) as unknown as typeof fetch);
    expect(noToken).toMatchObject({ refused: 'failed', why: 'the Obysphere gave no token (400)' });
    const down = await readPolicy(async () => ({ status: 0, text: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' }), () => key, sphere.http);
    expect(down).toMatchObject({ refused: 'failed', why: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' });
    for (const a of [...answers, offline, noToken, down]) {
      const said = JSON.stringify(a);
      for (const secret of [...sphere.granted.keys(), ...assertions]) expect(said).not.toContain(secret);
      expect(said).not.toMatch(/token-|eyJ/);
    }
    // the page's side never names the key, an assertion or a token
    const preload = readFileSync(path(__dirname, '../../../preload/index.ts'), 'utf8');
    for (const word of ['ownerKey', 'assertion', 'access_token']) expect(preload).not.toContain(word);
  });
});

describe('obysphere in the side panel', () => {
  let spheres: typeof import('../spheres');
  let peers: typeof import('../peers');
  beforeAll(async () => {
    vi.stubGlobal('location', { search: '' });
    vi.stubGlobal('window', {});
    spheres = await import('../spheres');
    peers = await import('../peers');
  });

  it('sphere_3_the_joined_obysphere_shows_with_the_other_obys_owners', () => {
    const joined = { state: 'joined', url: 'https://sphere.example.com', sphere: 'demo' };
    const s = spheres.liveSphere({ ...joined, owner: 'wellington' }, [{ owner: 'dmytro' }, { owner: 'ernesto' }], { demo: 'Demo team' })!;
    expect(s).toMatchObject({ id: 'live:demo', name: 'Demo team', admin: false, members: ['dmytro', 'ernesto'], invited: [], live: true, me: 'wellington' });
    expect(spheres.urlOf(s)).toBe('sphere.example.com');
    expect(spheres.memberCount(s)).toBe(3);
    // its id when the link's name isn't kept; nothing when obid isn't in one
    expect(spheres.liveSphere(joined, [], {})!.name).toBe('demo');
    expect(spheres.liveSphere({ state: 'none', url: null, sphere: null }, [], {})).toBeNull();
    expect(spheres.liveSphere({ ...joined, state: 'revoked' }, [], {})).toBeNull();
  });

  it('sphere_7_a_live_obysphere_s_members_open_no_made_up_conversation', () => {
    const live = spheres.liveSphere({ state: 'joined', url: 'https://sphere.example.com', sphere: 'demo' }, [{ owner: 'ernesto' }], {})!;
    expect(peers.memberOpens(live)).toBe('thread');
    // the mock's (and the tour's sample) keep Sangam's conversations
    expect(peers.memberOpens(spheres.SAMPLE)).toBe('tour');
  });
});

describe('what your Oby tells (docs/contracts/peer-thread.md)', () => {
  const rule = { id: 's-1', for: '*', repos: '*', tells: ['jobStatus'], uses: 0, starter: true } as SharingRule;
  it('peer_17_no_silent_share: a rule says what it tells, to whom and where, in plain words', () => {
    expect(whatItTells(rule, ['dana'])).toEqual({
      label: 'How your tasks are going',
      about: 'Each task’s title, repository, stage, state and pull request. To everyone in this Obysphere who asks, in every repository.',
    });
    expect(whatItTells({ ...rule, for: ['dana'], repos: ['acme/app'], uses: 2 }, ['dana', 'ernesto']).about).toContain('To dana, in acme/app. Answered 2 times.');
  });
  it('peer_23: offers each task\'s progress only to an owner who shares status with everyone and not that yet', () => {
    expect(canAddProgress([])).toBe(false);
    expect(canAddProgress([{ ...rule, for: ['dana'] }])).toBe(false);
    expect(canAddProgress([rule])).toBe(true);
    const yes = { ...rule, tells: ['jobStatus', 'jobProgress'] };
    expect(canAddProgress([yes])).toBe(false);
    expect(whatItTells(yes, ['dana']).label).toBe('How your tasks are going, and What each task is doing');
    expect(whatItTells(yes, ['dana']).about).toContain('current step and latest report line');
  });
  it('rooms_17: offers the room once status is shared, and not once a rule opens it', () => {
    const own = { ...rule, starter: false };
    expect(canOpenRoom([])).toBe(false);
    expect(canOpenRoom([{ ...rule, for: ['dana'] }])).toBe(false);
    expect(canOpenRoom([rule])).toBe(true);
    expect(canOpenRoom([rule, own])).toBe(false);
  });
  it('peer_38: the card lists what obid offers, ticked only as obid says, and a tick keeps the list order', () => {
    const card = (payload: unknown) => ({ confirm: { action: 'share_status', payload } }) as Parameters<typeof repoTicks>[0];
    expect(repoTicks(card({ offered: ['acme/app', 'me/taxes'], repos: [] }))).toEqual({ offered: ['acme/app', 'me/taxes'], chosen: [] });
    expect(repoTicks(card(null)), 'the card that only adds progress has no ticks').toBeUndefined();
    expect(repoTicks({ confirm: { action: 'share_room', payload: { offered: ['a/b'] } } })).toBeUndefined();
    const offered = ['a/a', 'b/b', 'c/c'];
    expect(tickRepo(offered, [], 'c/c')).toEqual(['c/c']);
    expect(tickRepo(offered, ['c/c'], 'a/a')).toEqual(['a/a', 'c/c']);
    expect(tickRepo(offered, ['a/a', 'c/c'], 'a/a')).toEqual(['c/c']);
    // Confirm: off with nothing ticked as obid has it, a tick on its way, or one refused
    expect(confirmIsOff({ chosen: [] }, false)).toBe(true);
    expect(confirmIsOff({ chosen: ['a/a'] }, true)).toBe(true);
    expect(confirmIsOff({ chosen: ['a/a'] }, false)).toBe(false);
    expect(confirmIsOff(undefined, true), 'a card with no ticks is as it was').toBe(false);
  });
  it('peer_42: Settings lists the chosen repositories, with Change only on the rule the owner’s yes made', () => {
    const chosen = { ...rule, repos: ['acme/app', 'acme/web'], tells: ['jobStatus', 'jobProgress'] };
    expect(whatItTells(chosen, ['dana']).about).toContain('in acme/app, acme/web.');
    expect(canChangeRepos(chosen)).toBe(true);
    expect(canChangeRepos({ ...chosen, starter: false }), 'a rule they wrote stays as they wrote it').toBe(false);
    expect(canChangeRepos(rule), 'nothing to tick on one for every repository').toBe(false);
    // the card already listed progress, and the room's card builds on it
    expect(canAddProgress([chosen])).toBe(false);
    expect(canAddProgress([{ ...chosen, tells: ['jobStatus'] }]), 'a rule of theirs for some repositories asks nothing').toBe(false);
    expect(canOpenRoom([chosen])).toBe(true);
    expect(canOpenRoom([chosen, { ...chosen, starter: false }])).toBe(false);
  });
  it('says the room only for a rule the owner made that covers everyone', () => {
    const own = { ...rule, starter: false };
    expect(whatItTells(own, ['dana']).about).toContain('in the room');
    expect(whatItTells(own, ['dana']).label).toBe('How your tasks are going, in the room');
    expect(whatItTells(rule, ['dana']).label).toBe('How your tasks are going');
    expect(whatItTells(rule, ['dana']).about).not.toContain('room');
    expect(whatItTells({ ...own, for: ['dana'] }, ['dana', 'ernesto']).about).not.toContain('room');
    expect(whatItTells({ ...own, for: ['dana', 'ernesto'] }, ['dana', 'ernesto']).about).toContain('in the room');
  });
});
