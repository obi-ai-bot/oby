import { createHash, createPublicKey, randomUUID, sign, type KeyObject } from 'node:crypto';

// The owner's side of joining an Obysphere (docs/contracts/obysphere.md; obi-sphere's spec
// identity.md): what the owner key signs (the Oby's card, its delegation, the owner's own short
// assertion) and the calls that join and leave through obid, which does the Oby's part
// (docs/runner-api.md, "obid only: Sphere membership"). Main holds the key (main/sphere.ts); this
// file holds no state, so its tests run it against a fake obid, and SPHERE-5 against two real ones.
// A port of the Swift Mac app's Sphere.swift (OwnerKey, InviteLink, join, leave).

export type Answer = { status: number; text: string };
export type Call = (method: string, path: string, body?: unknown) => Promise<Answer>;

// What another Oby may ask this one, and what the delegation lets it do (identity.md 4.2)
export const ACCEPTS = ['review_request', 'question'];
export const SCOPES = 'send:review_request send:question reply receive directory:read audit:read';
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

// An invitation as a link, read exactly as obi-sphere's spec sphere-api.md section 2.1 says
// (https://<host>/join#sphere=…&id=…&secret=…, a named one with for=<owner>&obi=<id>, one for a
// Google Workspace with domain=…), a port of its reference reader (spec/conformance/invite-links.mjs,
// Apache-2.0) checked against its known answers (test/fixtures/invite-link-vectors.json). The secret
// is after #, so it never reaches a server in a request. Oby's own scheme, oby://join/<host>#…, is
// the join page's Open in Oby, and always stands for https.
export type Invite = { url: string; host: string; sphere: string; name: string; by: string; owner?: string; obi?: string; domain?: string; id: string; secret: string };

const SLUG = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SECRET = /^[A-Za-z0-9_-]{43}$/;
const DOMAIN = /^(?=.{4,253}$)[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$/;
// eslint-disable-next-line no-control-regex
const LINE = /^[^\u0000-\u001F\u007F\u202A-\u202E\u2066-\u2069]*$/;
const LOOPBACK = new Set(['127.0.0.1', 'localhost', '[::1]']);
const KEYS = ['sphere', 'name', 'by', 'for', 'obi', 'domain', 'id', 'secret'];
// percent-decoded as UTF-8 (+ stays a plus), or null for a bad escape or bytes that aren't UTF-8
const decode = (t: string) => { try { return decodeURIComponent(t); } catch { return null; } };

// The link's fields as the spec names them (base, sphere, id, secret, and what else it says), or null
export function readInviteLink(text: string): Record<string, string> | null {
  // as written, not as a URL parser would tidy it: an ASCII host, the path exactly /join, no query
  const m = /^([hH][tT][tT][pP][sS]?):\/\/([A-Za-z0-9.-]+|\[[0-9A-Fa-f:.]+\])(:[0-9]{1,5})?\/join#(.*)$/s.exec(text.trim());
  if (!m) return null;
  let u: URL;
  try { u = new URL(`${m[1]}://${m[2]}${m[3] ?? ''}`); } catch { return null; }
  if (u.protocol !== 'https:' && !(u.protocol === 'http:' && LOOPBACK.has(u.hostname))) return null;
  const written = `${m[2]}${m[3] ?? ''}`.toLowerCase().replace(/:(80|443)$/, (p) => (p === `:${u.protocol === 'https:' ? 443 : 80}` ? '' : p));
  if (u.username || u.password || u.pathname !== '/' || u.host !== written || !m[4]) return null;
  const said: Record<string, string> = {};
  for (const part of m[4].split('&')) {
    if (!part) continue;
    const at = part.indexOf('=');
    const key = decode(at < 0 ? part : part.slice(0, at)), value = decode(at < 0 ? '' : part.slice(at + 1));
    if (key === null || value === null) return null;
    if (KEYS.includes(key) && !(key in said)) said[key] = value;
  }
  // an empty value is as if left out; so are a name or a by that aren't one line of 1 to 100 characters
  const f: Record<string, string> = { base: u.origin };
  for (const [k, v] of Object.entries(said)) {
    if (v === '' || ((k === 'name' || k === 'by') && (!LINE.test(v) || [...v].length > 100))) continue;
    f[k] = v;
  }
  if (!SLUG.test(f.sphere ?? '') || !UUID.test(f.id ?? '') || !SECRET.test(f.secret ?? '')) return null;
  if (('for' in f) !== ('obi' in f) || ('for' in f && (!SLUG.test(f.for) || !SLUG.test(f.obi)))) return null;
  if ('domain' in f && !DOMAIN.test(f.domain)) return null;
  return f;
}

// <scheme>://join/<host>[:<port>]#<fragment> as the https link it stands for, or null
export function openedInviteLink(text: string, scheme = 'oby'): string | null {
  const m = /^([A-Za-z][A-Za-z0-9+.-]*):\/\/join\/([A-Za-z0-9.-]+(?::[0-9]{1,5})?)#(.+)$/.exec(text.trim());
  if (!m || m[1].toLowerCase() !== scheme.toLowerCase()) return null;
  const link = `https://${m[2]}/join#${m[3]}`;
  return readInviteLink(link) ? link : null;
}

// An https link or an oby:// one, as what joining needs
export function parseInvite(text: string): Invite | null {
  const f = readInviteLink(openedInviteLink(text) ?? text);
  if (!f) return null;
  return { url: f.base, host: new URL(f.base).host, sphere: f.sphere, name: f.name ?? f.sphere, by: f.by ?? '', owner: f.for, obi: f.obi, domain: f.domain, id: f.id, secret: f.secret };
}

// What the confirm before joining warns of: the server is the link's, and it acts for the Oby for 30 days
export const joinWarning = (i: Invite) =>
  `Joining lets ${i.host} act for your Oby for 30 days. Join only if you trust ${i.host}.`;

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
// with the same scope once 5 days or fewer are left of the 30. ownerKey: asked for only while
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
    throw new NotThisKey(`Your Oby joined ${sphere} with another Oby app on this Mac, whose key this one doesn’t have. Nothing is broken while that app keeps running. Only if it’s gone for good: leave ${sphere} in its settings, then open its link again.`);
  }
  const body: Record<string, unknown> = {};
  if (m.sealing === 'card_needed') {
    const made = await must(call, 'POST', '/v1/obid/sphere/card', {
      url: m.url, sphere, obiId: obi, name: (m.card as { name?: string } | null)?.name ?? `${owner}’s Oby`, ownerKey: publicOf(ownerKey()), accepts: m.accepts,
    });
    body.card = signCard(made.card as Record<string, unknown>, ownerKey());
  }
  const held = m.delegation as { scope?: string; expiresAt?: string } | null;
  if (held?.scope && held.expiresAt && Date.parse(held.expiresAt) - now <= RENEW_WITHIN) body.delegation = assertion(ownerKey(), owner, sphere, held.scope, obi, now);
  if (Object.keys(body).length) await must(call, 'PUT', '/v1/obid/sphere/membership', body);
  return Object.keys(body);
}

// Leaves (SPHERE-4): the Sphere takes this Oby out first, with the owner's own token, which obid
// never holds; then obid forgets the Sphere and its Oby key. When the Sphere can't be told, obid
// leaves all the same and `untold` says why: the Sphere lists this Oby until its admin removes it.
export async function leave(call: Call, ownerKey: () => KeyObject, http: typeof fetch = fetch) {
  const now = await must(call, 'GET', '/v1/obid/sphere');
  let untold: string | null = null;
  if (now.state === 'joined') {
    const base = String(now.url), sphere = String(now.sphere), obi = String(now.obiId);
    try {
      // no redirect: one would carry the owner's assertion to another address
      const t = await http(`${base}/v0.1/oauth/token`, {
        method: 'POST', redirect: 'error', headers: { 'content-type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({
          grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange', subject_token_type: 'urn:ietf:params:oauth:token-type:jwt',
          subject_token: assertion(ownerKey(), String(now.owner), sphere, 'directory:read'),
        }),
      });
      const token = t.ok ? ((await t.json()) as { access_token?: string }).access_token : undefined;
      if (!token) throw new Error(`the Obysphere gave no token (${t.status})`);
      const d = await http(`${base}/v0.1/spheres/${encodeURIComponent(sphere)}/members/${encodeURIComponent(obi)}`, { method: 'DELETE', redirect: 'error', headers: { authorization: `Bearer ${token}` } });
      if (!d.ok) throw new Error(`the Obysphere answered ${d.status}`);
    } catch (e) {
      untold = e instanceof Error ? e.message : String(e);
    }
  }
  return { membership: await must(call, 'DELETE', '/v1/obid/sphere'), untold, sphere: String(now.sphere ?? 'the Obysphere') };
}
