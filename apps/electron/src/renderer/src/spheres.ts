import { useSyncExternalStore } from 'react';
import { BRANDS, type Brand } from './brands';
import { OBYS } from './mock';
import { everything, onObid } from './lean';

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
  // acts for there (obid's owner id)
  live?: true; url?: string; me?: string;
};
// you (until there's sign-in)
export const ME = { name: 'Doti', email: 'doti@deskree.com' };
export const SUFFIX = '.obysphere.com';
export const urlOf = (s: Sphere) => (s.url ? new URL(s.url).host : `${s.slug}${SUFFIX}`);

let spheres: Sphere[] = [
  { id: 'deskree', name: 'Deskree', slug: 'deskree', brand: BRANDS.deskree, admin: true, members: OBYS.map((o) => o.name), invited: [], domain: 'deskree.com' },
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
const NAMES = 'oby.sphereNames';
type Membership = { state: string; url: string | null; sphere: string | null; owner?: string };
export function liveSphere(m: Membership, members: { owner: string }[], names: Record<string, string>): Sphere | null {
  if (m.state !== 'joined' || !m.sphere || !m.url) return null;
  return { id: LIVE + m.sphere, name: names[m.sphere] ?? m.sphere, slug: m.sphere, admin: false, members: members.map((x) => x.owner), invited: [], live: true, url: m.url, me: m.owner };
}
// Whether a member's row opens a conversation with its Oby (SPHERE-7): not in a live Obysphere,
// whose conversations are the mock's (peers.ts), which would say a message went when nothing did.
// ponytail: a live member's row opens nothing until conversations go through obid's requests
export const canChat = (s: Sphere) => !s.live;
const names = (): Record<string, string> => { try { return JSON.parse(localStorage.getItem(NAMES) ?? '{}'); } catch { return {}; } };
async function loadLive() {
  const call = window.app!.obid.call;
  const m = await call('GET', '/v1/obid/sphere');
  if (m.status !== 200) return;
  const membership = JSON.parse(m.text) as Membership;
  const r = membership.state === 'joined' ? await call('GET', '/v1/obid/sphere/members') : null;
  const live = liveSphere(membership, r?.status === 200 ? JSON.parse(r.text) : [], names());
  set([...spheres.filter((s) => !s.live), ...(live ? [live] : [])]);
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
