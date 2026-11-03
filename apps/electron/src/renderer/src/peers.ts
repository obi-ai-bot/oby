import { useSyncExternalStore } from 'react';
import { PEER_CONVOS, hasAttachments, type Attachments, type PeerConvo, type PeerMessage } from './mock';
import { ApiError, api, peerThreadOf, type PeerExchange, type PeerPost, type PeerThread } from './obid';
import { streamWords, wait } from './stream';

// What a member's row opens: in a live Obysphere their thread, which is obid's; the tour's sample and
// the mock's Obyspheres (?everything, a browser) stay Sangam's made-up conversations, below, and
// never a live one, which would say a message went when nothing did.
export const memberOpens = (s: { live?: true; sample?: true }) => (s.live ? 'thread' : s.sample ? 'tour' : 'mock');
// the mock's conversations with someone, for their row: a live member has none of them, whatever their name
export const mockConvosWith = (s: { live?: true; sample?: true }, all: Convo[], name: string) => (memberOpens(s) === 'mock' ? convosWith(all, name) : []);

// An owner id that can't pass for another's: only what obid's directory says, and anything but plain
// ASCII (a Cyrillic i in "dima", a reversing mark) shows as its code. A Sphere's operator names ids.
export const plainId = (id: string) => id.replace(/[^\x21-\x7e]/gu, (c) => `\\u{${c.codePointAt(0)!.toString(16)}}`);
// Who a thread is with, as obid's whose_oby (brain/ask.rs): a name is what a card says of itself, so
// its owner id goes beside any name that differs. An ask has no Confirm: this is where the owner sees who.
export const whoseOby = (name: string, owner: string) => (name === owner ? `${plainId(owner)}’s Oby` : `${name}’s Oby (${plainId(owner)})`);

// Members' threads (docs/contracts/peer-thread.md), as obid has them; `gone`: obid no longer has this
// one (it left the Sphere, or they aren't a member). One per owner read so far, by exact owner id.
// ponytail: each change reads the thread again rather than merging peerThread's posts: a read from
// loopback, nothing to disagree with obid. The newest 50 exchanges, obid's default; add paging when
// someone has more.
export type Thread = PeerThread | { gone: true };
let threads: ReadonlyMap<string, Thread> = new Map();
const threadListeners = new Set<() => void>();
export const threadNow = (owner: string) => threads.get(owner);
export const useThreads = () => useSyncExternalStore((l) => { threadListeners.add(l); return () => { threadListeners.delete(l); }; }, () => threads);
const setThread = (owner: string, thread: Thread) => { threads = new Map(threads).set(owner, thread); threadListeners.forEach((l) => l()); };
// the newest read asked for, per owner: an older answer doesn't take a thread back
const asked = new Map<string, number>();
export async function lookAtThread(owner: string) {
  const mine = (asked.get(owner) ?? 0) + 1;
  asked.set(owner, mine);
  try {
    const thread = peerThreadOf(await api.peerThread(owner));
    if (thread && asked.get(owner) === mine) setThread(owner, thread);
  } catch (e) {
    // obid down or a 5xx: as it was, until it's read again
    if (e instanceof ApiError && (e.status === 404 || e.status === 409) && asked.get(owner) === mine) setThread(owner, { gone: true });
  }
}
export const threadChanged = (owner: string) => { if (asked.has(owner)) void lookAtThread(owner); };
export const lookAgainAtThreads = () => { for (const owner of asked.keys()) void lookAtThread(owner); };
// out of the Sphere: rejoining starts every thread empty
export const forgetThreads = () => { if (asked.size) { asked.clear(); threads = new Map(); threadListeners.forEach((l) => l()); } };

// The owner's own question: null once obid took it, else why not (its own words)
export async function askMember(owner: string, text: string): Promise<string | null> {
  try {
    await api.peerAsk(owner, text);
    await lookAtThread(owner);
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}

// Why the box can't ask, if so. null is obid not knowing yet (it hasn't read the directory): the box
// stays, and nobody is called gone.
export function boxShut(t: PeerThread): string | null {
  if (t.member === false) return `${plainId(t.owner)} left this Obysphere.`;
  return t.takesQuestions === false ? `${whoseOby(t.name, t.owner)} doesn’t take questions.` : null;
}

// What a thread shows: posts with words. One without (a status, a bare decline) says it through its
// exchange's line, under the last post that has words.
export const withWords = (posts: PeerPost[]) => posts.filter((p): p is PeerPost & { text: string } => p.text !== null);
// Where an exchange stands, plainly. SPH-5 owns these words. The clock decides `quietAt`, so no event says it.
export function standing(x: PeerExchange, owner: string, now: number): string | null {
  if (x.waitingOnThem) return `Waiting on ${plainId(owner)}`;
  if (x.quietAt) return Date.parse(x.quietAt) <= now ? 'Hasn’t answered yet' : 'Asking…';
  return x.state === 'declined' ? 'Declined' : x.state === 'expired' ? 'No answer came' : x.state === 'cancelled' ? 'Called off' : null;
}
// under each exchange's last post with words, and no other
export const lineUnder = (posts: PeerPost[], i: number, owner: string, now: number) =>
  (posts.slice(i + 1).some((p) => p.exchange.id === posts[i].exchange.id) ? null : standing(posts[i].exchange, owner, now));
// when the next open question counts as unanswered, if one still will
export const nextQuiet = (posts: PeerPost[], now: number) => Math.min(Infinity, ...posts.flatMap((p) => {
  const at = p.exchange.quietAt ? Date.parse(p.exchange.quietAt) : NaN;
  return at > now ? [at] : [];
}));

// Your conversations with the other Obys in the mock's Obyspheres (mock.ts), as a
// small store: started from an Oby's page, added to in a conversation, answered by
// the Oby you're talking to. typing: the Oby writing its reply (its words then
// come in a word at a time, as your own Oby's do).
export type Convo = PeerConvo & { typing?: boolean };

let convos: Convo[] = PEER_CONVOS;
const listeners = new Set<() => void>();
const set = (next: Convo[]) => { convos = next; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useConvos = () => useSyncExternalStore(subscribe, () => convos);

// an Oby's conversations, the latest first
const last = (c: Convo) => c.messages.at(-1)?.ago ?? 0;
export const convosWith = (all: Convo[], name: string) =>
  all.filter((c) => c.with.includes(name)).sort((a, b) => last(a) - last(b));

const update = (id: string, fn: (c: Convo) => Partial<Convo>) => set(convos.map((c) => (c.id === id ? { ...c, ...fn(c) } : c)));

// opened: its blue dot goes (and its Obys', unless another of theirs is unread)
export function markRead(id: string) {
  if (convos.some((c) => c.id === id && c.unread)) update(id, () => ({ unread: false }));
}
// an Oby has something unread: any of its conversations does
export const hasUnread = (all: Convo[], name: string) => all.some((c) => c.unread && c.with.includes(name));

// Its Oby answers after a moment of writing, as it would while it reads the message
const REPLY_MS = 1500;
async function reply(id: string, who: string, text: string) {
  update(id, () => ({ typing: true }));
  await wait(REPLY_MS);
  let at = 0;
  update(id, (c) => { at = c.messages.length; return { typing: false, messages: [...c.messages, { who, text: '', ago: 0, streaming: true }] }; });
  const edit = (fn: (m: PeerMessage) => PeerMessage) => update(id, (c) => ({ messages: c.messages.map((m, i) => (i === at ? fn(m) : m)) }));
  await streamWords(text, edit as Parameters<typeof streamWords>[1]);
  edit((m) => ({ ...m, streaming: undefined }));
}

// Said in a conversation: it goes on the end, and the Oby it's with (the first,
// in a group) says it'll pass it on.
export function say(id: string, text: string, att?: Attachments) {
  const t = text.trim(), c = convos.find((x) => x.id === id);
  if ((!t && !hasAttachments(att)) || !c) return;
  update(id, (k) => ({ messages: [...k.messages, { who: 'you', text: t, ago: 0, ...att }] }));
  void reply(id, c.with[0], `Got it. I’ll pass that to ${c.with[0]}.`);
}

// A new conversation with an Oby, named after your first words; returns its id.
let started = 0;
export function start(name: string, text: string, att?: Attachments) {
  const t = text.trim() || att?.files?.[0]?.name || '';
  if (!t && !hasAttachments(att)) return;
  const id = `pn${++started}`;
  const named = t || 'New conversation';
  const title = named.length > 48 ? `${named.slice(0, 45).trimEnd()}…` : named;
  set([{ id, title: title[0].toUpperCase() + title.slice(1), with: [name], messages: [{ who: 'you', text: text.trim(), ago: 0, ...att }] }, ...convos]);
  void reply(id, name, `Got it. I’ll check with ${name} and get back to you.`);
  return id;
}
