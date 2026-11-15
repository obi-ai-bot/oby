import type { Conversation, Turn } from '../../shared/obid-wire';
import type { Ask } from './view';

// Words to Oby on their way (docs/contracts/live-state.md, LIVE-15 to 18): one entry per send, by
// the turn id POST /v1/plans takes, until obid's own state answers for it. What the screen shows is
// obid's reduced state plus these. Nothing here sends or reads anything: each function answers new
// values and leaves its input as it was.

export type Pending = {
  id: string;                  // the words' turn id, as sent
  shown: string;               // the conversation as shown: obid's id, or the app's own ('new:1') for words starting one
  conversation: string | null; // obid's, once known
  said: string;                // the words as sent
  files: { name: string; preview?: string }[];  // their files as the screen shows them
  coming: boolean;             // the plan's answer said Oby's answer is coming
  // left on its own terms, but kept until obid lists the conversation it started, so its page stays
  over: boolean;
};
export type Pendings = Readonly<Record<string, Pending>>;

type Obid = { turns: Readonly<Record<string, Turn[]>>; conversations: readonly Conversation[] };

// words that start a conversation: shown under the app's own id until obid's is known
export const starts = (e: Pending) => e.shown.startsWith('new:');
const without = <T,>(p: Readonly<Record<string, T>>, id: string) => { const { [id]: _, ...rest } = p; return rest; };
// words that started a conversation stay until obid lists it (one it never named has nothing to wait for)
const held = (e: Pending, s: Pick<Obid, 'conversations'>) => starts(e) && !!e.conversation && !s.conversations.some((c) => c.id === e.conversation);
const leaving = (p: Pendings, e: Pending, s: Pick<Obid, 'conversations'>) => (held(e, s) ? { ...p, [e.id]: { ...e, over: true } } : without(p, e.id));

// words sent: on their way
export const added = (p: Pendings, e: Omit<Pending, 'coming' | 'over'>): Pendings => ({ ...p, [e.id]: { ...e, coming: false, over: false } });

// the plan's answer: Oby's answer is coming (a reply, a question, or the same words again), or none
// is and the words are done with. The conversation it names is obid's for them.
export function answered(p: Pendings, id: string, a: { kind: string; repeated?: boolean; conversation?: string | null }, s: Pick<Obid, 'conversations'>): Pendings {
  const e = p[id];
  if (!e) return p;
  const known = { ...e, conversation: e.conversation ?? a.conversation ?? null };
  const coming = a.kind !== 'none' || a.repeated === true;
  return coming ? { ...p, [id]: { ...known, coming } } : leaving({ ...p, [id]: known }, known, s);
}

// the send failed: obid refused it, or never answered any try
export const failed = (p: Pendings, id: string, s: Pick<Obid, 'conversations'>): Pendings => (p[id] ? leaving(p, p[id], s) : p);

// obid deleted a conversation: the words to it go with it
export function gone(p: Pendings, conversation: string): Pendings {
  const left = Object.values(p).filter((e) => e.conversation === conversation || e.shown === conversation);
  return left.length ? left.reduce((rest, e) => without(rest, e.id), p) : p;
}

// The conversation the words' own user turn is in, wherever it came from
const turnOf = (e: Pending, s: Obid) => {
  const where = e.conversation ? [e.conversation] : Object.keys(s.turns);
  for (const c of where) {
    const at = s.turns[c]?.findIndex((t) => t.id === e.id && t.who === 'user') ?? -1;
    if (at >= 0) return { c, at };
  }
  return undefined;
};

// obid's conversation for the words: the plan's answer's, or their own turn's
export const conversationOf = (e: Pending, s: Obid) => e.conversation ?? turnOf(e, s)?.c ?? null;

// Against obid's turns and conversations: each entry learns its conversation from its own turn,
// leaves once Oby's answer follows that turn (`answers` says which turns answer words), and one
// kept only for its conversation leaves once obid lists it. Unchanged, the same object.
export function settled(p: Pendings, s: Obid, answers: (t: Turn) => boolean): Pendings {
  let next = p;
  for (const e of Object.values(p)) {
    const mine = turnOf(e, s);
    const known = mine && e.conversation !== mine.c ? { ...e, conversation: mine.c } : e;
    if (known !== e) next = { ...next, [e.id]: known };
    const done = known.over || (!!mine && s.turns[mine.c].slice(mine.at + 1).some(answers));
    if (done && !(known.over && held(known, s))) next = leaving(next, known, s);
  }
  return next;
}

// obid's conversation the words starting a conversation are shown for: the app's id maps onto it,
// and onto no other. Anything but the words' own entry leaves the map as it is.
export function adopted(alias: Readonly<Record<string, string>>, e: Pending | undefined, conversation: string | null | undefined): Readonly<Record<string, string>> {
  if (!e || !conversation || !starts(e) || alias[conversation] === e.shown) return alias;
  return { ...Object.fromEntries(Object.entries(alias).filter(([, v]) => v !== e.shown)), [conversation]: e.shown };
}

// The owner's answers to cards and task actions on their way (LIVE-19 to 21): one entry per click,
// by what it acts on, until obid's state holds that object at a newer revision, or the call fails.
// What a card shows is obid's reduced state plus these.
export type Action = {
  what: 'answer' | 'stop' | 'archive' | 'delete' | 'rename';
  in: 'jobs' | 'inbox' | 'limits';  // where obid's state keeps what it acts on
  on: string;                       // its id there: a job's, an inbox item's, or a limit card's
  of: { revision?: number };        // obid's copy it acts on, and so its revision (none from an older obid)
  option?: string;                  // the answer, as the card shows it
  ask?: Ask;                        // the card it answered, as it was
  title?: string;                   // a rename's new name
};
export type Actions = Readonly<Record<string, Action>>;
const keyOf = (a: Pick<Action, 'what' | 'on'>) => `${a.what}:${a.on}`;
export const acting = (p: Actions, a: Action): Actions => ({ ...p, [keyOf(a)]: a });
// the call failed, or its click is over: its own entry leaves, never a later click's
export const unacted = (p: Actions, a: Action): Actions => (p[keyOf(a)] === a ? without(p, keyOf(a)) : p);
export const actionOn = (p: Actions, what: Action['what'], on: string): Action | undefined => p[keyOf({ what, on })];
// obid's copy is newer than the one acted on: a higher revision, another copy where either has none, or none any more
const newer = (now: { revision?: number } | undefined, a: Action) =>
  !now || (now.revision !== undefined && a.of.revision !== undefined ? now.revision > a.of.revision : now !== a.of);
// Against obid's state (`copyOf` finds what an entry acts on): those it holds newer leave. Unchanged, the same object.
export function confirmed(p: Actions, copyOf: (a: Action) => { revision?: number } | undefined): Actions {
  const left = Object.entries(p).filter(([, a]) => !newer(copyOf(a), a));
  return left.length === Object.keys(p).length ? p : Object.fromEntries(left);
}

// an answer to these words is still coming
export const awaited = (p: Pendings, id: string) => !!p[id] && !p[id].over;
// words on their way in a conversation (its shown id, or obid's)
export const awaitedIn = (p: Pendings, shown: string, conversation: string) =>
  Object.values(p).some((e) => !e.over && (e.shown === shown || e.conversation === conversation));
