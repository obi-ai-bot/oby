import { useSyncExternalStore } from 'react';
import { PEER_CONVOS, hasAttachments, type Attachments, type PeerConvo, type PeerMessage } from './mock';
import { ApiError, api, peerThreadOf, type AskMark, type PeerExchange, type PeerPost, type PeerThread } from './obid';
import { draft, keep, leftInBox } from './drafts';
import { streamWords, wait } from './stream';

// What a member's row opens: in a live Obysphere their thread, which is obid's; the tour's sample and
// the mock's Obyspheres (?everything, a browser) stay Sangam's made-up conversations, below, and
// never a live one, which would say a message went when nothing did.
export const memberOpens = (s: { live?: true; sample?: true }) => (s.live ? 'thread' : s.sample ? 'tour' : 'mock');
// the mock's conversations with someone, for their row: a live member has none of them, whatever their name
export const mockConvosWith = (s: { live?: true; sample?: true }, all: Convo[], name: string) => (memberOpens(s) === 'mock' ? convosWith(all, name) : []);

// An owner id with anything but plain ASCII shows those characters as codes, so a Cyrillic i in "dima"
// or a reversing mark shows for what it is. A Sphere's operator names ids. ASCII look-alikes (`dirna`) still pass.
export const plainId = (id: string) => id.replace(/[^\x21-\x7e]/gu, (c) => `\\u{${c.codePointAt(0)!.toString(16)}}`);
// Who a thread is with, as obid's whose_oby (brain/ask.rs): a name is what a card says of itself, so
// its owner id goes beside any name that differs. An ask has no Confirm: this is where the owner sees who.
export const whoseOby = (name: string, owner: string) => (name === owner ? `${plainId(owner)}’s Oby` : `${name}’s Oby (${plainId(owner)})`);

// Members' threads (docs/contracts/peer-thread.md), per exact owner id, outliving the view: thread, as
// obid has it; gone, obid's words for why it has none; sending, words on their way; problem, why the
// last words didn't go.
// ponytail: each change reads the thread again rather than merging peerThread's posts: a read from
// loopback, nothing to disagree with obid. The newest 50 exchanges, obid's default; add paging when
// someone has more.
export type Held = { thread?: PeerThread; gone?: string; sending?: string; problem?: string };
let threads: ReadonlyMap<string, Held> = new Map();
const threadListeners = new Set<() => void>();
export const threadNow = (owner: string): Held => threads.get(owner) ?? {};
export const useThreads = () => useSyncExternalStore((l) => { threadListeners.add(l); return () => { threadListeners.delete(l); }; }, () => threads);
const hold = (owner: string, change: Held) => { threads = new Map(threads).set(owner, { ...threads.get(owner), ...change }); threadListeners.forEach((l) => l()); };
// the newest read asked for, per owner: an older answer doesn't take a thread back
const asked = new Map<string, number>();
export async function lookAtThread(owner: string) {
  const mine = (asked.get(owner) ?? 0) + 1;
  asked.set(owner, mine);
  try {
    const thread = peerThreadOf(await api.peerThread(owner));
    if (thread && asked.get(owner) === mine) hold(owner, { thread, gone: undefined });
  } catch (e) {
    // obid down or a 5xx: as it was, until it's read again
    if (e instanceof ApiError && (e.status === 404 || e.status === 409) && asked.get(owner) === mine) hold(owner, { gone: e.message });
  }
}
// The view opening: what obid said last time isn't what it says now
export function openThread(owner: string) {
  if (threadNow(owner).gone !== undefined) hold(owner, { gone: undefined });
  void lookAtThread(owner);
}
export const threadChanged = (owner: string) => { if (asked.has(owner)) void lookAtThread(owner); };
export const lookAgainAtThreads = () => { for (const owner of asked.keys()) void lookAtThread(owner); };
// out of the Sphere: rejoining starts every thread empty
export const forgetThreads = () => { if (asked.size || threads.size) { asked.clear(); threads = new Map(); threadListeners.forEach((l) => l()); } };

// The box's send, which outlives its view (the owner can click someone else and back): whether the
// words went. One at a time per owner, or a box put back from its draft would ask twice. A refusal is
// kept to show whenever the thread is open. Of the draft it lets go of only what went.
export async function askFromBox(owner: string, text: string, box: string): Promise<boolean> {
  if (threadNow(owner).sending !== undefined) return false;
  hold(owner, { sending: text, problem: undefined });
  try {
    await api.peerAsk(owner, text.trim());
  } catch (e) {
    hold(owner, { sending: undefined, problem: e instanceof Error ? e.message : String(e) });
    return false;
  }
  keep(box, leftInBox(text, draft(box), true));
  hold(owner, { sending: undefined });
  await lookAtThread(owner);
  return true;
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
// Who a state line names: the owner id beside any other name, as whoseOby does, but not for a name
// that's only the id's capitals (`Dana`, `dana`), or every line would say it twice.
export function named(t: Pick<PeerThread, 'name' | 'owner'>) {
  const same = t.name.toLowerCase() === t.owner.toLowerCase();
  const id = same ? '' : ` (${plainId(t.owner)})`, shown = same ? plainId(t.name) : t.name;
  return { person: `${shown}${id}`, oby: `${shown}’s Oby${id}` };
}
// Where a question you asked stands, in ask-an-oby.md's words: a thread's line and a conversation's
// card say the same (AskCard.tsx). `by` and `reason` are what the sender says of itself: they pick
// these words and never show.
export function askedLine(state: string, t: Pick<PeerThread, 'name' | 'owner'>, by?: string | null, reason?: string | null): string | null {
  const { person, oby } = named(t);
  switch (state) {
    case 'asking': return 'Asking…';
    case 'waiting': return `It’s waiting on ${person}.`;
    case 'quiet': return `${oby} hasn’t answered yet.`;
    case 'answered': return by === 'owner' ? `${person} answered this one, not its Oby.` : `${person} wasn’t bothered.`;
    case 'expired': return `${oby} never answered: the question expired.`;
    case 'cancelled': return 'Called off';
    case 'declined':
      switch (reason) {
        case 'owner_declined': return `${person} said no to this one.`;
        case 'busy': return `${oby} is busy with other questions and didn’t take this one. Ask again later.`;
        case 'out_of_scope': return `${oby} doesn’t take questions like this one.`;
        case 'policy': return `This Obysphere’s rules stopped ${oby} answering. An admin can change them.`;
        // their obid saying so, once it's back: nobody said no
        case 'expired': return `${oby} never answered: the question expired.`;
        default: return `${oby} declined without saying why.`;
      }
    default: return null;
  }
}
// Where a question's card stands (AskCard.tsx). Past its quiet by the clock: no word may have reached
// obid, so "asking" would never end
export const askCardState = (c: AskMark, now: number) => (c.state === 'asking' && c.quietAt && Date.parse(c.quietAt) <= now ? 'quiet' : c.state);
export const askCardLine = (c: AskMark, now: number) => (c.state === 'refused' ? c.why ?? null : askedLine(askCardState(c, now), c, c.by, c.reason));
// Where an exchange stands. The clock decides `quietAt`, so no event says it. Past waiting, a question
// of theirs or a review has only its plain state.
export function standing(x: PeerExchange, t: Pick<PeerThread, 'name' | 'owner' | 'posts'>, now: number): string | null {
  if (x.waitingOnThem) return askedLine('waiting', t);
  if (x.quietAt) return askedLine(Date.parse(x.quietAt) <= now ? 'quiet' : 'asking', t);
  const said = t.posts.filter((p) => p.exchange.id === x.id);
  if (x.direction === 'outgoing' && x.type === 'question') {
    const answer = said.find((p) => p.kind === 'answer');
    if (x.state === 'done' && answer) return askedLine('answered', t, answer.by);
    if (x.state === 'expired') return askedLine('expired', t);
    if (x.state === 'declined') return askedLine('declined', t, null, said.find((p) => p.kind === 'decline')?.reason);
  }
  return x.state === 'declined' ? 'Declined' : x.state === 'expired' ? 'No answer came' : x.state === 'cancelled' ? 'Called off' : null;
}
// under each exchange's last post with words, and no other, so it's told once. shown: the thread's posts with words
export const lineUnder = (shown: PeerPost[], i: number, t: Pick<PeerThread, 'name' | 'owner' | 'posts'>, now: number) =>
  (shown.slice(i + 1).some((p) => p.exchange.id === shown[i].exchange.id) ? null : standing(shown[i].exchange, t, now));
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
