import { createHash, createPublicKey, randomUUID, sign, type KeyObject } from 'node:crypto';
import { starterRule, withStarterRule } from './policy';
import { INVITE_DAYS, INVITE_USES, openedInviteLink } from './invite';
import type { Invite } from './invite';

// The owner's side of joining an Obysphere (docs/contracts/obysphere.md; obi-sphere's spec
// identity.md): what the owner key signs (the Oby's card, its delegation, the owner's own short
// assertion) and the calls that join and leave through obid, which does the Oby's part
// (docs/runner-api.md, "obid only: Sphere membership"). Main holds the key (main/sphere.ts); this
// file holds no state, so its tests run it against a fake obid, and SPHERE-5 against two real ones.
// A port of the Swift Mac app's Sphere.swift (OwnerKey, InviteLink, join, leave).

export type Answer = { status: number; text: string };
export type Call = (method: string, path: string, body?: unknown) => Promise<Answer>;

// What another Oby may ask this one, and what the delegation lets it do (identity.md 4.2)
// handoff: another member's task, which the owner accepts on Home (docs/contracts/handoff.md, HANDOFF-6)
export const ACCEPTS = ['review_request', 'question', 'handoff'];
export const SCOPES = 'send:review_request send:question send:handoff reply receive directory:read audit:read';
const DELEGATION_LIFE = 2_592_000; // 30 days, the longest a delegation lives
const ASSERTION_LIFE = 300;

export const b64url = (b: Buffer) => b.toString('base64url');
// RFC 8785 for what the protocol uses (objects, arrays, strings, integers, booleans, null): an
// ES string is RFC 8785's string, and sort() orders keys by UTF-16 code units
export const canonical = (v: unknown): string =>
  Array.isArray(v) ? `[${v.map(canonical).join(',')}]`
    : v !== null && typeof v === 'object' ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canonical((v as Record<string, unknown>)[k])}`).join(',')}}`
      : JSON.stringify(v ?? null);

// The owner key's public half, raw (32 bytes) as base64url, and its id (identity.md section 1)
export const publicOf = (key: KeyObject) => createPublicKey(key).export({ format: 'jwk' }).x!;
export const keyIdOf = (key: KeyObject) => `ed25519:${b64url(createHash('sha256').update(Buffer.from(publicOf(key), 'base64url')).digest())}`;
const signed = (input: string, key: KeyObject) => b64url(sign(null, Buffer.from(input), key));

export function jwt(claims: Record<string, unknown>, key: KeyObject) {
  const input = `${b64url(Buffer.from(canonical({ alg: 'EdDSA', kid: keyIdOf(key), typ: 'JWT' })))}.${b64url(Buffer.from(canonical(claims)))}`;
  return `${input}.${signed(input, key)}`;
}

// The owner's own assertion for `sphere` (5 minutes), or with mayAct a delegation to that Oby (30 days)
export function assertion(key: KeyObject, owner: string, sphere: string, scope: string, mayAct?: string, now = Date.now()) {
  const iat = Math.floor(now / 1000);
  return jwt({
    iss: owner, sub: owner, aud: `obi-sphere:${sphere}`, iat, exp: iat + (mayAct ? DELEGATION_LIFE : ASSERTION_LIFE),
    jti: randomUUID(), scope, ...(mayAct ? { may_act: { sub: mayAct } } : {}),
  }, key);
}

// The card with the owner's one signature: a detached JWS over the card's canonical JSON (identity.md 2.1)
export function signCard(card: Record<string, unknown>, key: KeyObject) {
  const { signatures: _, ...rest } = card;
  const protectedHeader = b64url(Buffer.from(canonical({ alg: 'EdDSA', kid: keyIdOf(key), typ: 'JOSE' })));
  return { ...rest, signatures: [{ protected: protectedHeader, signature: signed(`${protectedHeader}.${b64url(Buffer.from(canonical(rest)))}`, key) }] };
}

// An invitation as a link: shared/invite.ts, which the page reads links with too
export { joinWarning, openedInviteLink, parseInvite, readInviteLink, type Invite } from './invite';

const error = (a: Answer) => { try { return (JSON.parse(a.text) as { error?: string }).error ?? a.text; } catch { return a.text; } };
const ok = (a: Answer) => a.status >= 200 && a.status < 300;
class Refused extends Error {}
async function must(call: Call, method: string, path: string, body?: unknown) {
  const a = await call(method, path, body);
  if (!ok(a)) throw new Refused(a.status ? error(a) : a.text);
  return JSON.parse(a.text) as Record<string, unknown>;
}

// The Sphere's refusal in plain words, with what to do (the Swift app's), or as it is
function plain(said: string, owner: string, sphere: string) {
  if (/has joined .* already/.test(said)) return ALREADY;
  if (said.includes('the owner already has another bound key')) return `${sphere} already has ${owner} with another Mac’s key (another Mac, or Oby before a reinstall). For now you’re in an Obysphere from one Mac: join from that one.`;
  if (said.includes('invitation_invalid')) return 'This link has expired, was used up or was cancelled. Ask whoever sent it for a new one.';
  return said;
}

const ALREADY = 'This Oby is already in an Obysphere. Leave it first (in its settings), then open this link again.';

// Joins with an invitation (SPHERE-1): obid's Oby key, its card signed by the owner, a delegation
// to it, then obid redeems the invitation and follows the Sphere. An open link's Oby is
// obi-<a new UUID> (an Oby is a UUID with a name, and a Sphere never takes an id twice); a named
// one's is its own. ownerKey is asked for only once the checks pass (SPHERE-2: a link that can't
// be used makes no key). Answers obid's membership.
export async function join(call: Call, ownerKey: () => KeyObject, invite: Invite, name?: string) {
  let owner = '';
  try {
    const now = await must(call, 'GET', '/v1/obid/sphere');
    owner = String(now.owner);
    if (invite.owner && invite.owner !== owner) {
      throw new Refused(`This invite is for ${invite.owner}, and this Mac’s Oby acts for ${owner}. Ask ${invite.by || 'the admin'} for one for ${owner}.`);
    }
    // ponytail: no Google sign-in in this app yet, so no link limited to a Workspace
    if (invite.domain && !invite.owner) throw new Refused(`This link is for people at ${invite.domain}, and Oby can’t sign you in with Google yet. Ask ${invite.by || 'the admin'} for a link made for you.`);
    if (now.state === 'joined') throw new Refused(ALREADY);
    const key = ownerKey();
    if (now.state === 'revoked') await must(call, 'DELETE', '/v1/obid/sphere');
    await must(call, 'POST', '/v1/obid/sphere/key');
    const obi = invite.obi ?? `obi-${randomUUID()}`;
    const made = await must(call, 'POST', '/v1/obid/sphere/card', {
      url: invite.url, sphere: invite.sphere, obiId: obi, name: name ?? `${owner}’s Oby`, ownerKey: publicOf(key), accepts: ACCEPTS,
    });
    return await must(call, 'POST', '/v1/obid/sphere/join', {
      url: invite.url, sphere: invite.sphere, invitation: { id: invite.id, secret: invite.secret },
      card: signCard(made.card as Record<string, unknown>, key),
      delegation: assertion(key, owner, invite.sphere, SCOPES, obi),
    });
  } catch (e) {
    throw new Error(e instanceof Refused ? plain(e.message, owner, invite.name) : e instanceof Error ? e.message : String(e));
  }
}

// What keeps a member's words sealed and its membership alive (SPHERE-6), signed with no click as
// the Swift app's were: a new card when obid says `card_needed` (its first card has no seal key;
// once the Obysphere and every teammate can read one, the next carries it), and a new delegation
// with the same scope once 5 days or fewer are left of the 30. Both also renew at once, once, when
// the membership lacks what this app's card takes or its delegation may send (SPHEREAPP-2). ownerKey: asked for only while
// joined (an owner who never joins gets none). Answers what it signed.
// A membership whose card another owner key signed (the Swift app's, or this app's before a
// Keychain reset) can't be kept: it throws NotThisKey, whose words say what to do.
const RENEW_WITHIN = 5 * 86_400_000;
export class NotThisKey extends Error {}
// The owner key a card names: its Obysphere extension's params (identity.md 2)
const cardOwnerKey = (card: unknown) =>
  ((card as { capabilities?: { extensions?: { params?: { owner_key?: string } }[] } } | null)?.capabilities?.extensions ?? [])
    .map((x) => x.params?.owner_key).find(Boolean);
export async function keepMembership(call: Call, ownerKey: () => KeyObject, now = Date.now()) {
  const m = await must(call, 'GET', '/v1/obid/sphere');
  if (m.state !== 'joined') return [];
  const [owner, sphere, obi] = [String(m.owner), String(m.sphere), String(m.obiId)];
  const signer = cardOwnerKey(m.card);
  if (signer && signer !== publicOf(ownerKey())) {
    throw new NotThisKey(`Your Oby joined ${sphere} with a key this app doesn’t have. If another Oby app on this Mac joined it, open that one: it keeps your membership going. If there’s no such app, leave ${sphere} in its settings, then open its link again.`);
  }
  const body: Record<string, unknown> = {};
  // what this app's card takes and its delegation may do, added to what the membership has (#2429,
  // SPHEREAPP-2): an Oby that joined before handoffs gains them, keeping anything else it holds
  const had = Array.isArray(m.accepts) ? (m.accepts as string[]) : [];
  const accepts = [...had, ...ACCEPTS.filter((a) => !had.includes(a))];
  const held = m.delegation as { scope?: string; expiresAt?: string } | null;
  const scopes = held?.scope ? held.scope.split(' ') : [];
  const scope = [...scopes, ...SCOPES.split(' ').filter((s) => !scopes.includes(s))].join(' ');
  if (m.sealing === 'card_needed' || accepts.length > had.length) {
    const made = await must(call, 'POST', '/v1/obid/sphere/card', {
      url: m.url, sphere, obiId: obi, name: (m.card as { name?: string } | null)?.name ?? `${owner}’s Oby`, ownerKey: publicOf(ownerKey()), accepts,
    });
    body.card = signCard(made.card as Record<string, unknown>, ownerKey());
  }
  const ending = !!held?.expiresAt && Date.parse(held.expiresAt) - now <= RENEW_WITHIN;
  if (held?.scope && (ending || scope !== held.scope)) body.delegation = assertion(ownerKey(), owner, sphere, scope, obi, now);
  if (Object.keys(body).length) await must(call, 'PUT', '/v1/obid/sphere/membership', body);
  return Object.keys(body);
}

// The owner's own token for `scope`, from a new 5-minute assertion (identity.md 4.3), with the
// scopes the Sphere granted; m is obid's membership. No redirect: one would carry the owner's
// assertion to another address.
async function ownerToken(http: typeof fetch, m: Record<string, unknown>, key: KeyObject, scope: string) {
  const t = await http(`${String(m.url)}/v0.1/oauth/token`, {
    method: 'POST', redirect: 'error', headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange', subject_token_type: 'urn:ietf:params:oauth:token-type:jwt',
      subject_token: assertion(key, String(m.owner), String(m.sphere), scope),
    }),
  });
  const got = t.ok ? ((await t.json()) as { access_token?: string; scope?: string }) : {};
  if (!got.access_token) throw new Error(`the Obysphere gave no token (${t.status})`);
  return { token: got.access_token, scopes: String(got.scope ?? '').split(' ') };
}
const sphereAt = (m: Record<string, unknown>) => `${String(m.url)}/v0.1/spheres/${encodeURIComponent(String(m.sphere))}`;

// Leaves (SPHERE-4): the Sphere takes this Oby out first, with the owner's own token, which obid
// never holds; then obid forgets the Sphere and its Oby key. When the Sphere can't be told, obid
// leaves all the same and `untold` says why: the Sphere lists this Oby until its admin removes it.
export async function leave(call: Call, ownerKey: () => KeyObject, http: typeof fetch = fetch) {
  const now = await must(call, 'GET', '/v1/obid/sphere');
  let untold: string | null = null;
  if (now.state === 'joined') {
    const obi = String(now.obiId);
    try {
      const { token } = await ownerToken(http, now, ownerKey(), 'directory:read');
      const d = await http(`${sphereAt(now)}/members/${encodeURIComponent(obi)}`, { method: 'DELETE', redirect: 'error', headers: { authorization: `Bearer ${token}` } });
      if (!d.ok) throw new Error(`the Obysphere answered ${d.status}`);
    } catch (e) {
      untold = e instanceof Error ? e.message : String(e);
    }
  }
  return { membership: await must(call, 'DELETE', '/v1/obid/sphere'), untold, sphere: String(now.sphere ?? 'the Obysphere') };
}

// The Obysphere's rules (docs/contracts/sphere-policy.md, POLICY-1 to 4): read with the owner's own
// directory:read token, written with an admin one. Each call makes a new assertion and exchange;
// a token is used on its own calls only and dropped, never kept, logged or answered. Nothing here
// throws: a failure is an answer, in words.
export type Rule = { id: string } & Record<string, unknown>;
export type Policy = { version: number; updated_at: string; rules: Rule[] };
// admins: who administers the Sphere now, sorted (its `admins`), on a read; me: this Oby's owner id
export type PolicyAnswer = {
  sphere: string | null; policy: Policy | null; admin: boolean;
  refused: null | 'not_joined' | 'not_admin' | 'changed' | 'failed'; why: string | null;
  admins?: string[]; me?: string;
};

// the members rule, with its id, is shared/policy.ts's
export { starterRule };

const NOT_JOINED = 'This Oby isn’t in an Obysphere.';
const ONLY_ADMINS = 'Only the Obysphere’s admins can change its rules.';
const CHANGED = 'The rules changed meanwhile: these are the rules as they are now.';
// Text from the Sphere as one line of plain text, at most 300 characters: no control or direction
// characters. Data, for the page to show as text, never markup or anything an agent reads.
const plainLine = (v: unknown) =>
  // eslint-disable-next-line no-control-regex
  (typeof v === 'string' ? [...v.replace(/[\u0000-\u001F\u007F\u202A-\u202E\u2066-\u2069]+/g, ' ').trim()].slice(0, 300).join('') : '');
const refusal = (sphere: string | null, refused: PolicyAnswer['refused'], why: string): PolicyAnswer => ({ sphere, policy: null, admin: false, refused, why });
const failed = (sphere: string | null, e: unknown) => refusal(sphere, 'failed', e instanceof Error ? e.message : String(e));

// The Sphere's policy (policy.schema.json), each rule as it was sent, or a throw
function asPolicy(v: unknown): Policy {
  const p = v as Partial<Policy> | null;
  if (!p || !Number.isSafeInteger(p.version) || typeof p.updated_at !== 'string' || !Array.isArray(p.rules) || !p.rules.every((r) => typeof r?.id === 'string')) {
    throw new Error('the Obysphere’s policy isn’t one Oby can read');
  }
  return { version: p.version!, updated_at: p.updated_at, rules: p.rules };
}
async function read(http: typeof fetch, url: string, token: string) {
  const r = await http(url, { redirect: 'error', headers: { authorization: `Bearer ${token}` } });
  if (!r.ok) throw new Error(`the Obysphere answered ${r.status}`);
  return (await r.json()) as unknown;
}

// The policy, and whether this owner is among the Sphere's admins (only `admins` says who may write)
export async function readPolicy(call: Call, ownerKey: () => KeyObject, http: typeof fetch = fetch): Promise<PolicyAnswer> {
  let sphere: string | null = null;
  try {
    const m = await must(call, 'GET', '/v1/obid/sphere');
    if (m.state !== 'joined') return refusal(null, 'not_joined', NOT_JOINED);
    sphere = String(m.sphere);
    const { token } = await ownerToken(http, m, ownerKey(), 'directory:read');
    const about = (await read(http, sphereAt(m), token)) as { admins?: unknown } | null;
    const policy = asPolicy(await read(http, `${sphereAt(m)}/policy`, token));
    const admins = Array.isArray(about?.admins) ? about.admins.filter((a): a is string => typeof a === 'string') : [];
    return { sphere, policy, admin: admins.includes(String(m.owner)), refused: null, why: null, admins, me: String(m.owner) };
  } catch (e) {
    return failed(sphere, e);
  }
}

// Replaces the policy with rules at expectedVersion (POLICY-2, 3): the PUT goes only with a token
// the Sphere granted `admin`. On a conflict the policy is read again with a new directory:read
// token; the Sphere's message is passed on as it came, never parsed.
export async function writePolicy(call: Call, ownerKey: () => KeyObject, rules: Rule[], expectedVersion: number, http: typeof fetch = fetch): Promise<PolicyAnswer> {
  let sphere: string | null = null;
  try {
    const m = await must(call, 'GET', '/v1/obid/sphere');
    if (m.state !== 'joined') return refusal(null, 'not_joined', NOT_JOINED);
    sphere = String(m.sphere);
    const { token, scopes } = await ownerToken(http, m, ownerKey(), 'admin directory:read');
    if (!scopes.includes('admin')) return refusal(sphere, 'not_admin', ONLY_ADMINS);
    const r = await http(`${sphereAt(m)}/policy`, {
      method: 'PUT', redirect: 'error', headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
      body: JSON.stringify({ expected_version: expectedVersion, rules }),
    });
    if (r.ok) return { sphere, policy: asPolicy(await r.json()), admin: true, refused: null, why: null };
    if (r.status === 403) return refusal(sphere, 'not_admin', ONLY_ADMINS);
    const said = r.status === 409 ? ((await r.json().catch(() => null)) as { error?: { code?: unknown; message?: unknown } } | null)?.error : undefined;
    if (said?.code !== 'conflict') throw new Error(`the Obysphere answered ${r.status}`);
    const now = await readPolicy(call, ownerKey, http);
    if (now.refused || !now.policy) return now;
    return now.policy.version !== expectedVersion
      ? { ...now, refused: 'changed', why: CHANGED }
      : { ...now, refused: 'failed', why: plainLine(said.message) || `the Obysphere answered ${r.status}` };
  } catch (e) {
    return failed(sphere, e);
  }
}

// The members rule added to the rules as they are at seenVersion, the version the page read
// (POLICY-1): every rule there kept, in order; a members rule already there gains only the actions
// it lacks (SPHEREAPP-1); nothing written when nothing is missing.
export async function addStarterRule(call: Call, ownerKey: () => KeyObject, seenVersion: number, http: typeof fetch = fetch): Promise<PolicyAnswer> {
  const now = await readPolicy(call, ownerKey, http);
  if (now.refused || !now.policy) return now;
  if (now.policy.version !== seenVersion) return { ...now, refused: 'changed', why: CHANGED };
  if (!now.admin) return { ...now, refused: 'not_admin', why: ONLY_ADMINS };
  const rules = withStarterRule(now.policy.rules);
  return rules ? writePolicy(call, ownerKey, rules, seenVersion, http) : now;
}

// Invite links, made by the Obysphere's admin from Oby (docs/contracts/sphere-app.md, SPHEREAPP-10):
// an open invitation (sphere-api.md section 4) for INVITE_USES people over INVITE_DAYS, its list and
// its withdrawal, each with a new owner token the Sphere grants `admin` only to an admin, used on
// that call and dropped. The secret comes once, in the answer that made it, and goes no further than
// the link handed back; the Sphere lists invitations without it. Nothing here throws.
export type Invitation = { id: string; uses: number; maxUses: number; expiresAt: string };
// handoffs: on a make or Invite member's reused link, whether the members rule now lets members hand off tasks (main adds it, SPHEREAPP-10, 25);
// unlisted: on a make, the link was made but the Sphere's list after it couldn't be read, so its
// invitations are none known, never none open (SPHEREAPP-21)
export type InvitesAnswer = { invitations: Invitation[]; link: string | null; refused: PolicyAnswer['refused']; why: string | null; handoffs?: boolean; unlisted?: true };
const invites = (invitations: Invitation[], link: string | null = null): InvitesAnswer => ({ invitations, link, refused: null, why: null });
const notInvited = (refused: PolicyAnswer['refused'], why: string): InvitesAnswer => ({ invitations: [], link: null, refused, why });

// The link a teammate's Oby opens: oby://join/<host>#sphere=…&by=…&id=…&secret=…, read back exactly
// as an oby:// link is (shared/invite.ts), or null when it wouldn't read
export function inviteLink(url: string, sphere: string, by: string, id: string, secret: string): string | null {
  const q = (v: string) => encodeURIComponent(v);
  const link = `oby://join/${new URL(url).host}#sphere=${q(sphere)}&by=${q(by)}&id=${q(id)}&secret=${q(secret)}`;
  return openedInviteLink(link) ? link : null;
}

const asInvitation = (v: unknown): Invitation | null => {
  const o = v as Record<string, unknown> | null;
  if (!o || typeof o.invitation_id !== 'string' || typeof o.expires_at !== 'string') return null;
  const n = (x: unknown) => (Number.isSafeInteger(x) ? (x as number) : 0);
  return { id: o.invitation_id, uses: n(o.uses), maxUses: n(o.max_uses), expiresAt: o.expires_at };
};

// m and an admin token, or the refusal in words
async function asAdmin(call: Call, ownerKey: () => KeyObject, http: typeof fetch) {
  const m = await must(call, 'GET', '/v1/obid/sphere');
  if (m.state !== 'joined') return { refused: notInvited('not_joined', NOT_JOINED) };
  const { token, scopes } = await ownerToken(http, m, ownerKey(), 'admin');
  if (!scopes.includes('admin')) return { refused: notInvited('not_admin', ONLY_ADMINS_INVITE) };
  return { m, token };
}
const ONLY_ADMINS_INVITE = 'Only the Obysphere’s admins can invite.';
const asked = (why: unknown) => notInvited('failed', why instanceof Error ? why.message : String(why));
async function listed(http: typeof fetch, m: Record<string, unknown>, token: string) {
  const r = await read(http, `${sphereAt(m)}/invitations`, token) as { invitations?: unknown } | null;
  return (Array.isArray(r?.invitations) ? r.invitations : []).flatMap((v) => asInvitation(v) ?? []);
}

export async function listInvitations(call: Call, ownerKey: () => KeyObject, http: typeof fetch = fetch): Promise<InvitesAnswer> {
  try {
    const a = await asAdmin(call, ownerKey, http);
    return 'refused' in a ? a.refused! : invites(await listed(http, a.m, a.token));
  } catch (e) { return asked(e); }
}

// A new open link, then the list as it is now
export async function makeInvitation(call: Call, ownerKey: () => KeyObject, http: typeof fetch = fetch): Promise<InvitesAnswer> {
  try {
    const a = await asAdmin(call, ownerKey, http);
    if ('refused' in a) return a.refused!;
    const r = await http(`${sphereAt(a.m)}/invitations`, {
      method: 'POST', redirect: 'error', headers: { authorization: `Bearer ${a.token}`, 'content-type': 'application/json' },
      body: JSON.stringify({ expires_in: INVITE_DAYS * 86_400, max_uses: INVITE_USES }),
    });
    if (r.status === 403) return notInvited('not_admin', ONLY_ADMINS_INVITE);
    if (!r.ok) throw new Error(`the Obysphere answered ${r.status}`);
    const made = (await r.json()) as { invitation_id?: unknown; secret?: unknown };
    const link = typeof made.invitation_id === 'string' && typeof made.secret === 'string'
      ? inviteLink(String(a.m.url), String(a.m.sphere), String(a.m.owner), made.invitation_id, made.secret) : null;
    if (!link) throw new Error('the Obysphere’s invitation isn’t one Oby can read');
    const now = await listed(http, a.m, a.token).catch(() => null);
    return now ? invites(now, link) : { ...invites([], link), unlisted: true };
  } catch (e) { return asked(e); }
}

// A link withdrawn (one the Sphere no longer has counts as withdrawn), then the list as it is now
export async function withdrawInvitation(call: Call, ownerKey: () => KeyObject, id: string, http: typeof fetch = fetch): Promise<InvitesAnswer> {
  try {
    const a = await asAdmin(call, ownerKey, http);
    if ('refused' in a) return a.refused!;
    const r = await http(`${sphereAt(a.m)}/invitations/${encodeURIComponent(id)}`, { method: 'DELETE', redirect: 'error', headers: { authorization: `Bearer ${a.token}` } });
    if (!r.ok && r.status !== 404) throw new Error(`the Obysphere answered ${r.status}`);
    return invites(await listed(http, a.m, a.token));
  } catch (e) { return asked(e); }
}

// Make admin and Remove admin (SPHEREAPP-11): PUT or DELETE …/admins/{owner} with the owner's own
// `admin` token, as the rules' write. The Sphere answers with itself, whose `admins` come back; it
// keeps at least one admin, and makes admin only an owner who has joined. Nothing here throws.
export type AdminsAnswer = { admins: string[] | null; refused: PolicyAnswer['refused']; why: string | null };
export async function setAdmin(call: Call, ownerKey: () => KeyObject, owner: string, on: boolean, http: typeof fetch = fetch): Promise<AdminsAnswer> {
  try {
    const a = await asAdmin(call, ownerKey, http);
    if ('refused' in a) return { admins: null, refused: a.refused!.refused, why: a.refused!.why === ONLY_ADMINS_INVITE ? ONLY_ADMINS_CHANGE : a.refused!.why };
    const r = await http(`${sphereAt(a.m)}/admins/${encodeURIComponent(owner)}`, { method: on ? 'PUT' : 'DELETE', redirect: 'error', headers: { authorization: `Bearer ${a.token}` } });
    if (r.status === 409) return { admins: null, refused: 'failed', why: on ? `${plainLine(owner)} hasn’t joined with their own key yet.` : 'An Obysphere keeps at least one admin.' };
    if (r.status === 403) return { admins: null, refused: 'not_admin', why: ONLY_ADMINS_CHANGE };
    if (!r.ok) throw new Error(`the Obysphere answered ${r.status}`);
    const about = (await r.json()) as { admins?: unknown } | null;
    return { admins: Array.isArray(about?.admins) ? about.admins.filter((x): x is string => typeof x === 'string') : null, refused: null, why: null };
  } catch (e) { return { admins: null, refused: 'failed', why: e instanceof Error ? e.message : String(e) }; }
}
const ONLY_ADMINS_CHANGE = 'Only the Obysphere’s admins can change who’s admin.';

// Getting back into an Obysphere with a sign-in (SPHEREAPP-12; obi-sphere's sphere-api.md section
// 3.2, `owner_rebind`): for an owner whose key is lost with their Mac. A new owner key, a sign-in for
// it at the Sphere's provider in the person's browser, back to main's own listener on this Mac; a new
// Oby card signed by the new key; then the Sphere binds the new key to the owner, revoking the old
// key's Obys, and this Oby joins in the same step. Only what the Sphere says comes back, in words.
export type SigninProvider = { id: string; name: string };
const sphereCode = async (r: Response) => ((await r.json().catch(() => null)) as { error?: { code?: unknown } } | null)?.error?.code;

// The providers to get back in with, when the Sphere offers it (`owner_rebind` in its features),
// else null. A Sphere from before the providers' list signs in with Google alone.
export async function rebindOffer(http: typeof fetch, url: string): Promise<SigninProvider[] | null> {
  try {
    const f = await http(`${url}/v0.1/features`, { redirect: 'error' });
    const features = f.ok ? ((await f.json()) as { features?: unknown }).features : null;
    if (!Array.isArray(features) || !features.includes('owner_rebind')) return null;
    const p = await http(`${url}/v0.1/signin`, { redirect: 'error' });
    if (p.status === 404) return [{ id: 'google', name: 'Google' }];
    const listed = p.ok ? ((await p.json()) as { providers?: unknown }).providers : null;
    const providers = (Array.isArray(listed) ? listed : []).flatMap((x) => {
      const o = x as { id?: unknown; name?: unknown };
      return typeof o.id === 'string' && /^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$/.test(o.id) ? [{ id: o.id, name: plainLine(o.name) || o.id }] : [];
    });
    return providers.length ? providers : null;
  } catch { return null; }
}

// What main lends getBackIn: the new key, a listener on this Mac for the browser's way back, the
// browser, and keeping the new key once the Sphere has bound it
export type BackInDeps = {
  newKey: KeyObject;
  listen: () => Promise<{ returnTo: string; back: Promise<{ state?: string; code?: string; error?: string }>; close: () => void }>;
  open: (url: string) => Promise<void>;
  keep: (key: KeyObject) => void;
};
export type BackIn = { kind: 'back'; owner: string; admin: boolean } | { kind: 'failed'; why: string };
const REBIND_WORDS: Record<number, string> = {
  404: 'This Obysphere has no owner by that id. Check its address and the owner id you’re getting back in as.',
  403: 'That sign-in can’t get you back in. Use the account this name added, or ask an admin to reset it.',
  409: 'That account is linked to someone else in this Obysphere, or this Mac already holds your key there.',
  429: 'Too many sign-ins at once right now. Try again in a minute.',
};
const isSecure = (u: string) => { try { const x = new URL(u); return x.protocol === 'https:' || (x.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(x.hostname)); } catch { return false; } };

// A sign-in at the Sphere's provider for ownerKey, in the person's browser, back to main's listener:
// its state and code, only for the state it started, or why not in words (sphere-api.md 3.1)
async function signedIn(http: typeof fetch, url: string, provider: string, ownerKey: string,
  listener: Awaited<ReturnType<BackInDeps['listen']>>, open: BackInDeps['open']): Promise<{ state: string; code: string } | { why: string }> {
  const started = await http(`${url}/v0.1/signin/${encodeURIComponent(provider)}`, {
    method: 'POST', redirect: 'error', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ owner_key: ownerKey, return_to: listener.returnTo }),
  });
  if (started.status === 429) return { why: REBIND_WORDS[429] };
  const signin = started.ok ? ((await started.json()) as { url?: unknown; state?: unknown }) : {};
  if (typeof signin.url !== 'string' || typeof signin.state !== 'string' || !isSecure(signin.url)) return { why: 'This Obysphere couldn’t start a sign-in. Try again later.' };
  await open(signin.url);
  const back = await listener.back;
  if (back.error === 'timeout') return { why: 'The sign-in took too long. Try again.' };
  if (back.state !== signin.state || back.error || !back.code) return { why: 'The sign-in didn’t finish. Try again.' };
  return { state: back.state, code: back.code };
}

export async function getBackIn(call: Call, http: typeof fetch, where: { url: string; sphere: string }, provider: string, d: BackInDeps): Promise<BackIn> {
  const failed = (why: string): BackIn => ({ kind: 'failed', why });
  let owner = '';
  let listener: Awaited<ReturnType<BackInDeps['listen']>> | null = null;
  try {
    const now = await must(call, 'GET', '/v1/obid/sphere');
    owner = String(now.owner ?? '');
    if (now.state === 'joined') return failed(ALREADY);
    const ownerKey = publicOf(d.newKey);
    listener = await d.listen();
    const back = await signedIn(http, where.url, provider, ownerKey, listener, d.open);
    if ('why' in back) return failed(back.why);
    // this Oby's own key and card, signed by the new owner key; obid's membership, if revoked, goes first
    if (now.state === 'revoked') await must(call, 'DELETE', '/v1/obid/sphere');
    await must(call, 'POST', '/v1/obid/sphere/key');
    const obi = `obi-${randomUUID()}`;
    const made = await must(call, 'POST', '/v1/obid/sphere/card', { url: where.url, sphere: where.sphere, obiId: obi, name: `${owner}’s Oby`, ownerKey, accepts: ACCEPTS });
    const card = signCard(made.card as Record<string, unknown>, d.newKey);
    const r = await http(`${where.url}/v0.1/spheres/${encodeURIComponent(where.sphere)}/owners/${encodeURIComponent(owner)}/rebind`, {
      method: 'POST', redirect: 'error', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ owner_key: ownerKey, signin: back, card }),
    });
    if (!r.ok) {
      const code = await sphereCode(r);
      if (code === NOT_LINKED_CODE) return failed(NOT_LINKED);
      if (code === 'card_invalid' || code === 'feature_not_taken') return failed('The Obysphere didn’t take this Oby’s card. Update Oby, then try again.');
      return failed(REBIND_WORDS[r.status] ?? `The Obysphere answered ${r.status}. Try again later.`);
    }
    const bound = (await r.json()) as { owner_key?: unknown; admin?: unknown };
    if (bound.owner_key !== ownerKey) return failed('The Obysphere answered with another key. Nothing was kept.');
    // bound: from here the new key is the owner's, so it's kept before anything else can fail
    d.keep(d.newKey);
    await must(call, 'POST', '/v1/obid/sphere/join', {
      url: where.url, sphere: where.sphere, rebound: true, card, delegation: assertion(d.newKey, owner, where.sphere, SCOPES, obi),
    }).catch((e) => { throw new Error(`You’re back in as ${owner}, but this Oby couldn’t take its place yet: ${e instanceof Error ? e.message : String(e)}`); });
    return { kind: 'back', owner, admin: bound.admin === true };
  } catch (e) {
    return failed(e instanceof Refused ? plain(e.message, owner, where.sphere) : e instanceof Error ? e.message : String(e));
  } finally {
    listener?.close();
  }
}

// Link first (the owner's decision): only an owner who added a sign-in, with that same account,
// gets back in by themselves. An owner who never did hears so (the Sphere's code for it; assumed
// until its route is fixed).
const NOT_LINKED_CODE = 'account_not_linked';
const NOT_LINKED = 'This name never added a sign-in. Ask an admin to reset it.';

// Add sign-in (SPHEREAPP-13), for an owner who still has their key: a sign-in at the Sphere's provider
// for the owner key this app holds, then the Sphere links that account to the owner, with the owner's
// own token: PUT …/owners/{owner}/account {signin} (assumed until the Sphere's shape is fixed). The
// account is the Sphere's to keep; the app never sees or shows an email.
export type SigninStatus = { providers: SigninProvider[] | null; linked: boolean | null };
export async function signinStatus(call: Call, ownerKey: () => KeyObject, http: typeof fetch = fetch): Promise<SigninStatus> {
  try {
    const m = await must(call, 'GET', '/v1/obid/sphere');
    if (m.state !== 'joined') return { providers: null, linked: null };
    const providers = await rebindOffer(http, String(m.url));
    if (!providers) return { providers: null, linked: null };
    const { token } = await ownerToken(http, m, ownerKey(), 'directory:read');
    const r = await http(`${sphereAt(m)}/owners/${encodeURIComponent(String(m.owner))}`, { redirect: 'error', headers: { authorization: `Bearer ${token}` } });
    const linked = r.ok ? ((await r.json()) as { account_linked?: unknown }).account_linked : undefined;
    return { providers, linked: typeof linked === 'boolean' ? linked : null };
  } catch { return { providers: null, linked: null }; }
}

export async function addSignin(call: Call, ownerKey: () => KeyObject, http: typeof fetch, provider: string,
  d: Pick<BackInDeps, 'listen' | 'open'>): Promise<{ linked: true } | { why: string }> {
  let listener: Awaited<ReturnType<BackInDeps['listen']>> | null = null;
  try {
    const m = await must(call, 'GET', '/v1/obid/sphere');
    if (m.state !== 'joined') return { why: NOT_JOINED };
    listener = await d.listen();
    const back = await signedIn(http, String(m.url), provider, publicOf(ownerKey()), listener, d.open);
    if ('why' in back) return back;
    const { token } = await ownerToken(http, m, ownerKey(), 'directory:read');
    const r = await http(`${sphereAt(m)}/owners/${encodeURIComponent(String(m.owner))}/account`, {
      method: 'PUT', redirect: 'error', headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' }, body: JSON.stringify({ signin: back }),
    });
    if (r.ok) return { linked: true };
    if (r.status === 409) return { why: 'That account already belongs to someone else in this Obysphere. Sign in with your own.' };
    if (r.status === 403) return { why: 'That sign-in didn’t take. Use the account you want to get back in with, then try again.' };
    if (r.status === 429) return { why: REBIND_WORDS[429] };
    return { why: `The Obysphere answered ${r.status}. Try again later.` };
  } catch (e) {
    return { why: e instanceof Error ? e.message : String(e) };
  } finally {
    listener?.close();
  }
}
