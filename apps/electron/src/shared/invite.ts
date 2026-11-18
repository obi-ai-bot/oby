// An Obysphere's invite link, read with no Node in it, so main's engine (shared/sphere.ts) and the
// page (the join field, JoinLink.tsx) read a link the one way (SPHEREAPP-3).

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


// What joining from a link came to (main/sphere.ts): joined, the owner's Cancel in its dialog, a
// text that isn't a link, or why it failed in words for the page to show
export type Joined = { kind: 'joined'; name: string } | { kind: 'cancelled' } | { kind: 'not_a_link' } | { kind: 'failed'; name: string; why: string };
