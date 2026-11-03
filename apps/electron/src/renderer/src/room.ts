import { useSyncExternalStore } from 'react';
import { ApiError, api, roomPostOf, type RoomPost } from './obid';
import { onObid } from './lean';
import { MOCK_ROOM, mockRoomScene } from './mock';

// The Sphere's room (docs/contracts/room-app.md), as a small store: obid is in one Sphere at a time,
// and its `roomPost` event names none, so there's one room here: that Sphere's. Available or not is
// obid's own answer (404: an obid without it, today's); not available, nothing of it shows and
// everything goes on as it did. In a browser, the mock's.
export type Room = { sphere: string | null; status: 'unknown' | 'available' | 'unavailable'; posts: RoomPost[] };

const NOWHERE: Room = { sphere: null, status: 'unknown', posts: [] };
let room: Room = onObid ? NOWHERE : MOCK_ROOM;
// the highest seq obid's answers listed, never an event's: an event can pass posts missed while the
// stream was away, and asking from past them would miss them for good
let listed = 0;
const listeners = new Set<() => void>();
const set = (next: Room) => { room = next; listeners.forEach((l) => l()); };
export const roomNow = () => room;
export const useRoom = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, roomNow);

// Whether a Sphere's row opens its room: never the tour's sample
export const roomOpens = (r: Room, s: { slug: string; sample?: true }) => !s.sample && r.sphere === s.slug && r.status === 'available';

// As obid lists them: by seq, then those not sent yet, one copy of each by id, the newer one. A list
// read before an event landed doesn't take a post back to waiting. obid keeps its newest 1,000 too.
export function merged(posts: RoomPost[], more: RoomPost[]) {
  const all = new Map(posts.map((p) => [p.id, p]));
  for (const p of more) {
    const had = all.get(p.id);
    if (!had || had.state === 'waiting' || p.state !== 'waiting') all.set(p.id, p);
  }
  const place = (p: RoomPost) => p.seq ?? Infinity;
  const order = (a: RoomPost, b: RoomPost) => (place(a) === place(b) ? 0 : place(a) - place(b)) || a.createdAt.localeCompare(b.createdAt);
  return [...all.values()].sort(order).slice(-1000);
}

export async function lookAtRoom(sphere: string) {
  if (room.sphere !== sphere) { listed = 0; set({ ...NOWHERE, sphere }); }
  try {
    const answer = await api.room(sphere, listed);
    if (room.sphere !== sphere) return;
    if (answer?.room !== 'available') return set({ ...room, status: 'unavailable' });
    const posts = (Array.isArray(answer.posts) ? answer.posts : []).flatMap((p) => roomPostOf(p) ?? []);
    listed = Math.max(listed, ...posts.map((p) => p.seq ?? 0));
    set({ ...room, status: 'available', posts: merged(room.posts, posts) });
  } catch (e) {
    // an obid without the room, or a Sphere it's not in. Anything else (obid down, a 5xx): as it
    // was, until the next look
    if (e instanceof ApiError && e.status >= 400 && e.status < 500 && room.sphere === sphere) set({ ...room, status: 'unavailable' });
  }
}
export const lookAgain = () => { if (room.sphere) void lookAtRoom(room.sphere); };
export const forgetRoom = () => { if (room.sphere) { listed = 0; set(NOWHERE); } };

// obid's roomPost: only a Sphere with a room sends one. Before the Sphere is known, the first look has it.
export function takeRoomPost(post: RoomPost) {
  if (room.sphere) set({ ...room, status: 'available', posts: merged(room.posts, [post]) });
}

// ponytail: the mock's stand-in for obid's answer, its secret scan the word itself
async function mockSay(text: string): Promise<RoomPost> {
  if (/secret/i.test(text)) throw new ApiError(422, 'The secret scan found a possible secret in your words: nothing was posted.');
  const at = new Date().toISOString(), seq = Math.max(0, ...room.posts.map((p) => p.seq ?? 0)) + 1, waits = mockRoomScene === 'waiting';
  return { id: `mock-say-${Date.now()}`, seq: waits ? null : seq, recordedAt: waits ? null : at, from: 'obi-doti', owner: 'Doti', kind: 'say', by: 'owner', createdAt: at, replyTo: null, text, about: [], state: waits ? 'waiting' : 'sent', error: null };
}

// The owner's own words in the room: null once obid took them, else why not (its own words)
export async function sayInRoom(sphere: string, text: string): Promise<string | null> {
  try {
    const post = onObid ? roomPostOf(await api.roomSay(sphere, text)) : await mockSay(text);
    if (post) takeRoomPost(post);
    return null;
  } catch (e) {
    // the room went since the last look
    if (e instanceof ApiError && e.status === 409) void lookAtRoom(sphere);
    return e instanceof Error ? e.message : String(e);
  }
}

// What a kind means, in words: raw kinds are never shown. A kind this app doesn't know: no words
const DOING: Record<string, string> = { working_on: 'Working on', done: 'Finished', need: 'Asking', found: 'Answering', say: 'Said' };
// Who a post is from: its Oby's, or the person's own words
export const whose = (p: RoomPost, me: string) => {
  const mine = p.owner === me;
  return p.by === 'owner' ? (mine ? 'You' : p.owner) : mine ? 'Your Oby' : `${p.owner}’s Oby`;
};
// what it's doing, and whom it answers once that post is known here
export function doing(p: RoomPost, posts: RoomPost[], me: string) {
  const to = p.kind === 'found' && p.replyTo ? posts.find((x) => x.id === p.replyTo) : undefined;
  const name = to && whose(to, me);
  return (DOING[p.kind] ?? '') + (name ? ` ${name === 'You' ? 'you' : name === 'Your Oby' ? 'your Oby' : name}` : '');
}
// The repositories, pull requests and issues it's about, as plain words, and not what its words
// already say (a job's post names its own). A job's id means nothing to a person.
export function aboutWords(p: RoomPost) {
  const named = p.about.flatMap((r) => {
    if (typeof r.repo !== 'string') return [];
    return r.kind === 'repo' ? [r.repo] : r.kind === 'pr' ? [`${r.repo}#${r.pr}`] : r.kind === 'issue' ? [`${r.repo}#${r.issue}`] : [];
  });
  return named.filter((n) => !p.text.includes(n)).join(', ');
}
// whether the reader is at the end of the thread: then a new post follows them down; anywhere else
// they stay put
export const atEnd = (l: { scrollTop: number; clientHeight: number; scrollHeight: number }) => l.scrollHeight - l.scrollTop - l.clientHeight < 40;
