import { useSyncExternalStore } from 'react';
import { BRANDS, type Brand } from './brands';
import { OBYS } from './mock';
import { everything, onObid } from './lean';
import { forgetRoom, lookAtRoom } from './room';
import { forgetThreads } from './peers';
import type { PictureAnswer, Rule } from '../../shared/sphere';
import type { SphereMember } from '../../shared/obid-wire';
import { askableOf, setAskable } from './askable';

// The Obyspheres you're in: each a place where your Oby works with other
// people's. Its picture in the icon strip is its logo (an image you gave it,
// or a brand's mark), else its name's first letter. admin: you made it or were
// made an admin of it (only people invited in can leave). members: the Obys in
// it besides yours (by their owner's name); invited: addresses asked in that
// haven't joined yet. domain: anyone with an address there can join by its link.
// sample: the one Oby shows while setting up (SAMPLE), never one you're in
export type Sphere = {
  id: string; name: string; slug: string; brand?: Brand; image?: string;
  admin: boolean; members: string[]; invited: string[]; domain?: string; sample?: true;
  // live: the one obid is in (docs/contracts/obysphere.md), at its own address; me: who your Oby
  // acts for there (obid's owner id); obi: your Oby's id there
  live?: true; url?: string; me?: string; obi?: string;
  // the live one took this Oby out (obid's membership `revoked`): it stays, saying so, until the owner
  // leaves it or joins again (SPHEREAPP-5); why: obid's code for it (`revokedReason`), when it gives one
  revoked?: true; why?: string;
};
// you (until there's sign-in)
export const ME = { name: 'Doti', email: 'doti@deskree.com' };
export const SUFFIX = '.obysphere.com';
export const urlOf = (s: Sphere) => (s.url ? new URL(s.url).host : `${s.slug}${SUFFIX}`);

let spheres: Sphere[] = [
  { id: 'deskree', name: 'Deskree', slug: 'deskree', brand: BRANDS.deskree, admin: true, members: OBYS.map((o) => o.name), invited: [], domain: 'deskree.com', obi: 'obi-doti' },
];
// A sample to look around in while setting up (onboarding.ts's Obysphere tour): shown, never
// kept. It's outside the store, so nothing that lists your Obyspheres (Settings, Knowledge, Home)
// sees it; only the side panel adds it, while the tour has it out
export const SAMPLE: Sphere = {
  id: 'sample', name: 'Deskree', slug: 'deskree', brand: BRANDS.deskree, admin: false,
  members: ['Wellington', 'Ernesto', 'Dima', 'Sangam'], invited: ['ana@deskree.com'], domain: 'deskree.com', sample: true,
};
// The + to make one: only with ?everything until making one reaches the Obysphere (it would add
// to this mock only, gone at the next launch). Showing it in the lean build is the plan's
// decision 1, the owner's to make; the tour's words follow this
export const CAN_CREATE = everything;
const listeners = new Set<() => void>();
const set = (next: Sphere[]) => { spheres = next; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useSpheres = () => useSyncExternalStore(subscribe, () => spheres);

// Its URL (deskree.obysphere.com): lowercase letters, digits and single hyphens, 3–30 long; made
// from the name as it's typed (Deskree × Avenga → deskree-avenga)
export const slugOf = (text: string) => text.toLowerCase().normalize('NFKD').replace(/\p{M}/gu, '').replace(/[^a-z0-9]+/g, '-').replace(/^-+/, '').slice(0, 30);
// a hyphen left at its end while typing doesn't count
export const trimSlug = (slug: string) => slug.replace(/-+$/, '');
// how many are in it, you included
export const memberCount = (s: Sphere) => s.members.length + 1;
// mine: the Obysphere whose URL it is now (changing it, it isn't taken by itself)
export const slugProblem = (slug: string, mine?: string) => {
  const s = trimSlug(slug);
  if (s.length < 3) return 'At least 3 characters';
  // taken: someone else's (the mock's), the app's own words, or one of yours
  if (TAKEN.includes(s) || spheres.some((x) => x.slug === s && x.id !== mine)) return 'Taken';
  return null;
};
const TAKEN = ['avenga', 'tetrix', 'oby', 'app', 'www', 'admin', 'api', 'help', 'acme'];

// made by you: you're its admin; the people you invited are waiting to join
export function addSphere(s: Omit<Sphere, 'id' | 'admin' | 'members'>) {
  const id = `s${Date.now().toString(36)}`;
  set([...spheres, { ...s, slug: trimSlug(s.slug), id, admin: true, members: [] }]);
  return id;
}
// changed in its Settings (an admin's); deleted (an admin), or left (anyone
// else): gone from yours either way
export const updateSphere = (id: string, patch: Partial<Pick<Sphere, 'name' | 'slug' | 'image'>>) =>
  set(spheres.map((s) => (s.id === id ? { ...s, ...patch } : s)));
// A picture main set or removed (SPHEREAPP-19): shown on the live Obysphere only when the answer is
// of that one, its address and id as it is now; another's (one left or joined meanwhile) shows on
// none. Whether it showed.
export function showPicture(a: PictureAnswer): boolean {
  const live = spheres.find((s) => s.live && a.at === `${s.url}/${s.slug}`);
  if (live) updateSphere(live.id, { image: a.image ?? undefined });
  return !!live;
}
export const deleteSphere = (id: string) => {
  // the live one: obid leaves it (main/sphere.ts), then what obid says shows
  if (spheres.find((s) => s.id === id)?.live) void window.app?.sphereLeave?.().finally(loadLive);
  const image = spheres.find((s) => s.id === id)?.image;
  if (image) URL.revokeObjectURL(image);
  set(spheres.filter((s) => s.id !== id));
};
// from its settings' Members (an admin's): someone taken out, or an invite taken back
export const removeMember = (id: string, name: string) => set(spheres.map((s) => (s.id === id ? { ...s, members: s.members.filter((m) => m !== name) } : s)));
export const cancelInvite = (id: string, email: string) => set(spheres.map((s) => (s.id === id ? { ...s, invited: s.invited.filter((e) => e !== email) } : s)));
export const invite = (id: string, emails: string[]) =>
  set(spheres.map((s) => (s.id === id ? { ...s, invited: [...s.invited, ...emails.filter((e) => !s.invited.some((x) => x.toLowerCase() === e.toLowerCase()))] } : s)));

// The Obysphere obid is in (SPHERE-3), from GET /v1/obid/sphere and its members: in the side panel
// as any other, by the name its invite link gave (kept here: obid knows only its id), its members
// the other Obys' owners. Not its admin: its admin made it on the Obysphere's own server.
const LIVE = 'live:';
// The room is hidden for launch (the launch plan's cut list; SPHEREAPP-8): no Obysphere anyone can
// reach has one yet. Never looked for, it never opens: its tile click, Settings' "Also post in the
// room" and its posts all wait on obid saying it's there (room.ts). Back once obi-sphere deploys it.
export const ROOM_SHOWS = false;
const NAMES = 'oby.sphereNames';
// revokedReason: obid's code for a revoke (add-only, an open set: `removed` today), null otherwise;
// absent from an obid before it
type Membership = { state: string; url: string | null; sphere: string | null; owner?: string; obiId?: string; revokedReason?: string | null };
export function liveSphere(m: Membership, members: { owner: string }[], names: Record<string, string>): Sphere | null {
  if ((m.state !== 'joined' && m.state !== 'revoked') || !m.sphere || !m.url) return null;
  const base = { id: LIVE + m.sphere, name: names[m.sphere] ?? m.sphere, slug: m.sphere, admin: false, invited: [], live: true as const, url: m.url, me: m.owner, obi: m.obiId };
  if (m.state === 'revoked') return { ...base, members: [], revoked: true, ...(typeof m.revokedReason === 'string' && m.revokedReason ? { why: m.revokedReason } : {}) };
  return { ...base, members: [...new Set(members.map((x) => x.owner))] };
}
// What a revoked Obysphere says, where its members were (SPHEREAPP-5), by obid's code, never the code
// itself: `removed` (and an obid that gives none) is the Sphere taking it out; a code this app doesn't
// know yet says only that it's out
export const revokedLine = (s: Sphere) =>
  (!s.why || s.why === 'removed' ? `${s.name}’s admin took your Oby out.` : `Your Oby isn’t in ${s.name} any more.`);
// a step that goes when its Obysphere does: a member's thread with the live one
export const outOfSphere = (l: { kind: string; id?: string }, all: Sphere[]) =>
  ((l.kind === 'sphere' || l.kind === 'knowledge' || l.kind === 'room') && !all.some((s) => s.id === l.id)) || (l.kind === 'member' && !all.some((s) => s.live));
const names = (): Record<string, string> => { try { return JSON.parse(localStorage.getItem(NAMES) ?? '{}'); } catch { return {}; } };
// Each read of the live Obysphere (loadLive: at launch, on a join, every minute), counted: what the
// side panel reads with it, such as whether this owner is its admin (SPHEREAPP-26), reads again
let reads = 0;
const readers = new Set<() => void>();
export const useLiveReads = () => useSyncExternalStore((l) => { readers.add(l); return () => { readers.delete(l); }; }, () => reads);
async function loadLive() {
  const call = window.app!.obid.call;
  const m = await call('GET', '/v1/obid/sphere');
  if (m.status !== 200) return;
  const membership = JSON.parse(m.text) as Membership;
  const r = membership.state === 'joined' ? await call('GET', '/v1/obid/sphere/members') : null;
  const members: SphereMember[] = r?.status === 200 ? JSON.parse(r.text) : [];
  const live = liveSphere(membership, members, names());
  // whom `@` in the message box asks (ASK-37): none out of a Sphere, or revoked
  setAskable(live && !live.revoked ? askableOf(members) : []);
  // the membership shows now, with the picture shown so far; main's read of the picture (SPHEREAPP-15)
  // follows and never holds it up. The one shown stays when main couldn't read it.
  // the same Obysphere is its address and its id: another server's with the same id keeps nothing
  const same = (s: Sphere) => !!live && !!s.live && s.url === live.url && s.slug === live.slug;
  const shown = spheres.find(same)?.image;
  set([...spheres.filter((s) => !s.live), ...(live ? [shown ? { ...live, image: shown } : live] : [])]);
  if (live && !live.revoked) void window.app!.spherePicture?.().then((read) => {
    if (read?.refused !== null || read.at !== `${live.url}/${live.slug}`) return;
    set(spheres.map((s) => (same(s) ? { ...s, image: read.image ?? undefined } : s)));
  }, () => {});
  // whether it has a room, and what came since: with this minute's look at the directory, below
  if (live && !live.revoked) { if (ROOM_SHOWS) void lookAtRoom(live.slug); } else { forgetRoom(); forgetThreads(); }
  reads++;
  readers.forEach((l) => l());
}
if (onObid && window.app?.onSphere) {
  window.app.onSphere(({ sphere, name }) => {
    try { localStorage.setItem(NAMES, JSON.stringify({ ...names(), [sphere]: name })); } catch { /* its id shows */ }
    void loadLive();
  });
  void loadLive();
  // ponytail: a teammate who just joined shows within a minute; obid has no event for the directory
  setInterval(() => void loadLive(), 60_000);
}

// A rule of the Obysphere's (docs/contracts/sphere-policy.md), in the words its settings list it
// with: who may ask whom, what and where. One with anything else in it (from_classes, an action
// this version doesn't know) shows by its id, since partial words would say more than it allows.
const ACTIONS = new Map([['question', 'questions'], ['review_request', 'reviews'], ['handoff', 'handoffs'], ['run_request', 'runs'], ['share_offer', 'share offers'], ['introduction', 'introductions']]);
const RULE_KEYS = ['id', 'from_owner', 'to_owner', 'actions', 'repos'];
const someStrings = (v: unknown): v is string[] => Array.isArray(v) && v.length > 0 && v.every((x) => typeof x === 'string');
export function ruleWords(rule: Rule): { label: string; about: string } {
  const { from_owner: from, to_owner: to, actions, repos } = rule;
  if (typeof from !== 'string' || typeof to !== 'string' || !someStrings(actions) || !actions.every((a) => ACTIONS.has(a))
    || !someStrings(repos) || Object.keys(rule).some((k) => !RULE_KEYS.includes(k))) {
    return { label: rule.id, about: 'Oby can’t put this rule in words yet.' };
  }
  const what = actions.map((a) => ACTIONS.get(a)!);
  const listed = what.length > 1 ? `${what.slice(0, -1).join(', ')} and ${what.at(-1)}` : what[0];
  const where = repos.length === 1 && repos[0] === '*' ? 'every repository' : repos.join(', ');
  return {
    label: `${from === '*' ? 'Every member' : from} may ask ${to === '*' ? 'every other member’s Oby' : `${to}’s Oby`}`,
    about: `${listed[0].toUpperCase()}${listed.slice(1)}, in ${where}.`,
  };
}
