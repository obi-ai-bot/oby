// What obid has, as the app keeps it, and the one rule its events and answers follow over it
// (docs/contracts/live-state.md). Pure: no I/O, no module state and nothing of the screens, so
// main can run it too.
import type { Conversation, Event, GitHubWork, InboxItem, Job, Status, Turn } from './obid-wire';

export type ObidState = {
  jobs: Record<string, Job>;
  gone: ReadonlySet<string>;  // the jobs obid said it deleted: never back from an answer that crossed it
  status: Status | null;
  github: GitHubWork | null;     // what GitHub wants from the owner (GET /v1/github/work), none from an older obid
  inbox: InboxItem[];            // open and put off, by when they came
  conversations: Conversation[];
  turns: Record<string, Turn[]>; // by conversation: each opened one's, and any that came since, by when they came
  goneConversations: ReadonlySet<string>;          // the conversations obid said it deleted, as `gone`
  settledInbox: Readonly<Record<string, number>>;  // the revision each settled item was settled at: an older open copy can't reopen it
};

// The kinds the rule takes
export type ObidEvent = Extract<Event, { kind: 'job' | 'jobGone' | 'conversation' | 'conversationGone' | 'turn' | 'turnAsks' | 'turnDelivery' | 'inbox' | 'accounts' | 'github' }>;

// Of two copies of one object, from an event or an answer, whether the incoming one is taken: the
// newer by obid's revision, as an answer can come after an event that's newer than it (#1709), the
// incoming one on a tie. One with none (an older obid, after a downgrade) is taken as it comes,
// never held off by a revision from before
const takes = (had: { revision?: number } | undefined, incoming: { revision?: number }) =>
  !had || incoming.revision === undefined || (had.revision ?? 0) <= incoming.revision;

// a job: a deleted one stays out
const taken = (s: ObidState, j: Job): ObidState =>
  s.gone.has(j.id) || !takes(s.jobs[j.id], j) ? s : { ...s, jobs: { ...s.jobs, [j.id]: j } };

const conversationTaken = (s: ObidState, c: Conversation): ObidState =>
  s.goneConversations.has(c.id) || !takes(s.conversations.find((x) => x.id === c.id), c) ? s
    : { ...s, conversations: [...s.conversations.filter((x) => x.id !== c.id), c] };

const byAt = (a: Turn, b: Turn) => a.at.localeCompare(b.at);
// a turn, under its conversation: none of a deleted one
const turnTaken = (s: ObidState, c: string, t: Turn): ObidState => {
  const had = s.turns[c] ?? [];
  if (s.goneConversations.has(c) || !takes(had.find((x) => x.id === t.id), t)) return s;
  return { ...s, turns: { ...s.turns, [c]: [...had.filter((x) => x.id !== t.id), t].sort(byAt) } };
};

// an inbox item: one settled leaves, and a copy older than the one that settled it doesn't bring it back
const itemTaken = (s: ObidState, i: InboxItem): ObidState => {
  const mark = s.settledInbox[i.id];
  if ((mark !== undefined && !takes({ revision: mark }, i)) || !takes(s.inbox.find((x) => x.id === i.id), i)) return s;
  const rest = s.inbox.filter((x) => x.id !== i.id);
  if (i.state === 'done') return { ...s, inbox: rest, settledInbox: i.revision === undefined ? s.settledInbox : { ...s.settledInbox, [i.id]: i.revision } };
  const { [i.id]: _, ...settledInbox } = s.settledInbox;
  return { ...s, inbox: [...rest, i].sort((a, b) => a.created.localeCompare(b.created)), settledInbox: mark === undefined ? s.settledInbox : settledInbox };
};

export function apply(s: ObidState, e: ObidEvent): ObidState {
  switch (e.kind) {
    case 'job': return taken(s, e.job);
    case 'jobGone': {
      const { [e.id]: _, ...jobs } = s.jobs;
      return { ...s, jobs, gone: new Set([...s.gone, e.id]) };
    }
    case 'conversation': return conversationTaken(s, e.conversation);
    case 'conversationGone': {
      const { [e.id]: _, ...turns } = s.turns;
      return { ...s, conversations: s.conversations.filter((c) => c.id !== e.id), turns, goneConversations: new Set([...s.goneConversations, e.id]) };
    }
    // a question's card on a turn, and its words' delivery (docs/api/events.md), are the turn again,
    // as far as what obid has goes
    case 'turn': case 'turnAsks': case 'turnDelivery':
      return e.turn.conversation ? turnTaken(s, e.turn.conversation, e.turn) : s;
    case 'inbox': return itemTaken(s, e.item);
    case 'accounts': return s.status ? { ...s, status: { ...s.status, accounts: e.accounts } } : s;
    case 'github': return { ...s, github: e.work };
  }
}

// a job a call answered with: the same rule as its event
export const answer = (s: ObidState, j: Job): ObidState => taken(s, j);
// the turns GET /v1/conversation answered with, under the conversation asked for: the same rule as their events
export const answerTurns = (s: ObidState, c: string, turns: Turn[]): ObidState => turns.reduce((s, t) => turnTaken(s, c, t), s);
// obid's status, which has no revision: taken as it comes
export const answerStatus = (s: ObidState, status: Status): ObidState => ({ ...s, status });

// how many of its latest jobs the reconnect reads
export const LATEST_JOBS = 100;

// After a reconnect: what obid listed (`read`) over what's held, `before` being what was held when
// the read started. obid's lists are the truth, since a revision can go back (a restart after a
// save that didn't land, a downgrade and upgrade again), but over a copy that changed while they
// were read (an answer came meanwhile) the rule decides. The events that came meanwhile are applied
// over this afterwards.
export type Read = { jobs: Job[]; conversations: Conversation[]; complete: boolean; inbox: InboxItem[]; status: Status | null; github: GitHubWork | null };
export function synced(held: ObidState, before: ObidState, read: Read): ObidState {
  const changed = <T,>(now: T | undefined, then: T | undefined) => now !== undefined && now !== then;
  const newest = <T extends { revision?: number }>(had: T | undefined, then: T | undefined, listed: T) =>
    had && changed(had, then) && !takes(had, listed) ? had : listed;

  // obid's latest jobs, newest first, and the older ones a conversation fetched, which aren't among
  // them: any other held was deleted while the stream was away (on the iPhone, say), and goes
  const oldest = read.jobs.length < LATEST_JOBS ? undefined : read.jobs.at(-1)!.created;
  const older = Object.values(held.jobs).filter((j) => oldest && j.created <= oldest); // <=: created is whole seconds, so jobs past the 100th can share its second
  const listed = read.jobs.filter((j) => !held.gone.has(j.id)).map((j) => newest(held.jobs[j.id], before.jobs[j.id], j));

  // every conversation, unless the list was cut short: then none held is dropped
  const find = (list: Conversation[], id: string) => list.find((c) => c.id === id);
  const conversations = read.conversations.filter((c) => !held.goneConversations.has(c.id))
    .map((c) => newest(find(held.conversations, c.id), find(before.conversations, c.id), c));
  const ids = new Set(read.conversations.map((c) => c.id));
  const kept = read.complete ? [] : held.conversations.filter((c) => !ids.has(c.id));

  // the inbox as listed, by the item rule over only the copies and settled marks that came while it was read
  const items = new Set(read.inbox.map((i) => i.id));
  const fresh: ObidState = {
    ...held,
    inbox: held.inbox.filter((i) => items.has(i.id) && changed(i, before.inbox.find((b) => b.id === i.id))),
    settledInbox: Object.fromEntries(Object.entries(held.settledInbox).filter(([id, r]) => before.settledInbox[id] !== r)),
  };
  const { inbox, settledInbox } = read.inbox.reduce(itemTaken, fresh);

  return {
    ...held, jobs: Object.fromEntries([...older, ...listed].map((j) => [j.id, j])), conversations: [...conversations, ...kept],
    inbox, settledInbox, status: read.status, github: read.github,
  };
}
