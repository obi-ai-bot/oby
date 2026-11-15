import { useSyncExternalStore } from 'react';
import { manageUsage, MANAGE_USAGE } from './chatgpt';
import { ApiError, api, type Event, type InboxItem, type Job, type LimitDecision, type Question, type Repo, type Status } from './obid';
import { STEER, TELL, type Attachments } from './view';
import { replaceChats } from './chats';
import { replaceTasks } from './tasks';
import { keepFoldersOf, nameFoldersBy, saveFolders, sendFoldersBy, takeFolders } from './folders';
import { accountRow, moveKeptChoices, replaceAccounts } from './SettingsView';
import { ALLOW_ONCE, TRY_AGAIN } from '../../shared/words';
import { has } from '../../shared/capabilities';
import { answer, answerStatus, answerTurns, apply as applyToObid, type ObidState } from '../../shared/live-state';
import { unwired, type LiveMessage, type LiveSnapshot } from '../../shared/live-wire';
import { onObid } from './lean';
import { lookAgain, takeRoomPost } from './room';
import { lookAgainAtThreads, threadChanged } from './peers';
import { acting, actionOn, added, adopted, answered as planAnswered, awaitedIn, confirmed, conversationOf, failed as sendFailed, gone as goneWith, settled, unacted, type Action, type Actions, type Pendings } from './pending';
import { answers, askOf, asksOf, chatsOf, checkOf, isEscalation, itemOf, lastRepo, limitAsks, limitError, limitHolds, limitLabel, limitLater, takesNoAnswer, tasksOf, waitingOf, wordsOf, workOf } from './liveView';
import { forgetSession, heard } from './liveSession';

// What obid has (docs/runner-api.md), as main's one follower of its events keeps it (main/live.ts):
// main's snapshot when the page opens and after each sync, then each event main applied, applied
// here too. Sangam's screens don't change: what obid has goes into the same
// stores the mock filled (chats.ts, tasks.ts), as conversations and tasks. Without ?everything
// (lean.ts) the app runs on this; with it, on the mock. What obid has (its jobs, conversations,
// turns, inbox, status and GitHub's work) is ObidState's, kept by the shared rule (shared/live-state.ts)
export type Live = ObidState & {
  connection: 'connecting' | 'live' | 'down';
  problem: string | null;        // why obid isn't answering (its own reason, main/obid.ts), or what last went wrong
  why?: 'waiting' | 'approval';  // and what kind of why (main/obid.ts, reason): 'waiting', none yet
  repos: Repo[];                 // where a job may start (GET /v1/repos)
  firstJobs: Set<string>;        // the jobs there when the app opened: never new
  seen: Set<string>;             // jobs opened since
  limits: LimitDecision[];       // one decision for the jobs stopped by an account's limit
  loaded: Set<string>;           // the conversations whose turns were fetched
  working: Record<string, string>;            // a turn under way, by conversation: what it's on
  on: Record<string, string>;                 // and whose words it's for, by conversation: a supervisor's are nobody's
  // words this app sent, by their turn, until obid's state answers for them (pending.ts)
  pending: Pendings;
  // words whose next answer isn't theirs (a supervisor's came first): they keep what they started
  unanswered: Set<string>;
  words: Record<string, string>;              // its words so far (delta events), by conversation
  // Oby's words as shown, by conversation: a word at a time toward `to` (its words so far, then its
  // turn's once that's in; the turn waits, hidden, until they're all shown). History shows at once.
  pace: Record<string, { to: string; n: number; turn?: string }>;
  waiting: Record<string, Question & { said: string }>;  // Oby's question back, by conversation
  // obid's id for a conversation words of this app's started, by their turn or the plan's answer
  // only, onto the app's own id they're shown under ('new:1'), so its page stays open
  alias: Record<string, string>;
  // answers to cards and task actions on their way, until obid's state holds what they act on at a
  // newer revision (pending.ts): a card shows its pick meanwhile, and the stack lets it go
  actions: Actions;
  // the repository last picked for words sent here, by conversation (its app id), and when
  picked: Record<string, { repo: string; at: string }>;
  // the files your words took, by their turn: the name obid kept, the one picked, a photo's preview
  previews: Record<string, { name: string; shown: string; preview?: string }[]>;
};

let state: Live = {
  connection: 'connecting', problem: null, status: null, repos: [], github: null, jobs: {}, firstJobs: new Set(), seen: new Set(), inbox: [], limits: [],
  conversations: [], turns: {}, loaded: new Set(), working: {}, on: {}, pending: {}, unanswered: new Set(), words: {}, pace: {}, waiting: {}, alias: {}, actions: {}, picked: {}, previews: {}, gone: new Set(), goneConversations: new Set(), settledInbox: {},
};
const listeners = new Set<() => void>();
// what an entry acts on, as obid's state now holds it
const copyOf = (s: Live, a: Action) => (a.in === 'jobs' ? s.jobs[a.on] : a.in === 'inbox' ? s.inbox.find((i) => i.id === a.on) : s.limits.find((d) => `limit:${d.id}:${d.revision}` === a.on));
const set = (patch: Partial<Live>) => {
  const next = { ...state, ...patch };
  state = { ...next, actions: confirmed(next.actions, (a) => copyOf(next, a)) }; publish(); listeners.forEach((l) => l());
  if (!pacing && Object.keys(state.pace).length) { ticked = performance.now(); pacing = setInterval(tick, WORD_MS); }
};
// An event, or obid's answer to a call as one, over the state as it is when it's taken: never a
// state read before the call's await, which would undo what came meanwhile (a Confirm's answer
// putting back the job its own `deleted` event took away)
// (main brings an event's item's job itself; an answer's the page asks it for)
const takeEvent = (e: Event) => {
  if (e.kind === 'transcript' || e.kind === 'transcriptLine') return heard(e);
  if (e.kind === 'jobGone') forgetSession(e.id);
  set(apply(state, e));
};
const take = (e: Event) => {
  takeEvent(e);
  if (e.kind === 'inbox') lookUpJobOf(e.item);
};
// a job that waits on you, however old: past the latest 100, its ask would show nowhere (obid
// keeps every open item's job, and an item can come with no event for its job; a start item's isn't one until it starts)
// (read and kept by main, which sends it on as its event)
const lookUpJobOf = (i: Pick<InboxItem, 'job' | 'kind' | 'state'>) => {
  if (i.job && i.kind !== 'start' && i.state !== 'done' && !state.jobs[i.job] && !state.gone.has(i.job)) void window.app?.obid.load({ job: i.job });
};
export const useLive = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => state);
// the limit decisions for a conversation's or a job's work (ChatView), as Home's stack shows them
export const useLimitAsks = (conversation?: string, job?: string) => { useLive(); return limitAsks(state, conversation, job); };
export const useLimitCount = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => state.limits.length);
export const useWaitingCount = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => waitingOf(state).length);
export const liveState = () => state;

const without = <T,>(r: Record<string, T>, key: string) => { const { [key]: _, ...rest } = r; return rest; };
// words this app has on their way in a conversation
const inFlight = (s: Live, c: string) => awaitedIn(s.pending, s.alias[c] ?? c, c);
// the app's id for one of obid's conversations, and back
export const shownId = (c: string) => state.alias[c] ?? c;
export const realId = (shown: string) => Object.keys(state.alias).find((k) => state.alias[k] === shown) ?? shown;
// a new conversation's working folders, saved under obid's id once it has one (folders.ts)
nameFoldersBy(realId);
// once obid takes folders, a conversation's go to it (one not yet named keeps its own meanwhile)
sendFoldersBy((shown, folders) => {
  const c = realId(shown);
  if (!has(state.status, 'folders-v1') || c.startsWith('new:')) return undefined;
  return api.editConversation(c, { folders }).then((conversation) => take({ kind: 'conversation', conversation }));
}, say);
let aliasSeen: Live['alias'] | undefined;
// obid's state as it now is, under the words on their way: words starting a conversation are shown
// for the one their own turn is in, and those obid's state answers for leave (pending.ts)
const overlaid = (s: Live): Live => {
  const alias = Object.values(s.pending).reduce((a, e) => adopted(a, e, conversationOf(e, s)), s.alias);
  const pending = settled(s.pending, s, answers);
  return alias === s.alias && pending === s.pending ? s : { ...s, alias, pending };
};

// --- obid's state as the screens' conversations and tasks (liveView.ts)

export { approvingTools, askOf, chatOf, homeAsk, itemAsk, limitAsks, projectOf, questionOf, reviewOf, taskOf, waitingOf } from './liveView';
// Waiting for you's rows (waitingOf): Merge is the merge card's own, signed by main by its label (signed-words.md);
// Allow once and Deny answer the task's permission, as its card does; an item with nothing to open (a
// standalone Confirm, another Oby's question): its own answer, by label
export const mergeWaiting = (i: InboxItem) => answerJob(i.id, 'Merge');
export const allowWaiting = (i: InboxItem, option: 'Allow once' | 'Deny') => answerJob(i.job, option);
export const answerWaiting = (i: InboxItem, option: string) => answerJob(i.id, option);
// A card as obid just answered it (PEER-38: its title after a tick)
export const seeItem = (item: InboxItem) => take({ kind: 'inbox', item });
// A limit card's answer: the option shown, sent with the revision shown (signed by main, device.ts)
async function answerLimitAsk(id: string, option: string) {
  const [, decisionId, revision] = id.split(':');
  if (option === 'Later') { limitLater.set(`${decisionId}:${revision}`, Date.now() + 3600_000); set({}); return; }
  // (no answer: it opens OpenAI's settings, and the question stays)
  if (option === MANAGE_USAGE) { manageUsage(); return; }
  const decision = state.limits.find((d) => d.id === decisionId && String(d.revision) === revision);
  const picked = decision?.options.find((o) => limitLabel(o) === option);
  if (!decision || !picked) throw new Error('This choice changed. Look at it again before choosing.');
  const a = act('answer', 'limits', id, { option });
  try {
    await answerLimit(decision, picked);
    limitError.delete(decisionId);
  } catch (e) {
    limitError.set(decisionId, `Couldn’t save your choice: ${e instanceof Error ? e.message : String(e)}`);
    throw e;
  } finally { drop(a); }
}

// into the stores the screens read
let waking: ReturnType<typeof setTimeout> | undefined;
let accountsFrom: Status | null = null;
let sessionTurn: ReturnType<typeof setTimeout> | undefined;
function publish() {
  if (state.alias !== aliasSeen) { aliasSeen = state.alias; saveFolders(); }
  if (has(state.status, 'folders-v1')) takeFolders(Object.fromEntries(state.conversations.map((c) => [shownId(c.id), c.folders ?? []])));
  replaceChats(chatsOf(state));
  replaceTasks(tasksOf(state), { asks: asksOf(state), work: workOf(state.github), error: state.github?.error ?? undefined });
  // what's put off comes back when its time comes, which obid sends no event for: look again then
  // (the soonest, and at least hourly)
  clearTimeout(waking);
  const due = Math.min(...[...state.inbox.map((i) => (i.later ? Date.parse(i.later) : Infinity)), ...limitLater.values()].filter((t) => t > Date.now()));
  if (due < Infinity) waking = setTimeout(() => set({}), Math.min(due - Date.now() + 500, 3600_000));
  // the week's share, else the 5-hour window's; unknown until the tool says (never a made-up 0%).
  // Only when obid's status changed: publish runs at every change, each word of Oby's included,
  // and Settings › Accounts and the usage gauge draw again for each list.
  if (!state.status || state.status === accountsFrom) return;
  accountsFrom = state.status;
  // a 5-hour window past its reset reads as empty (docs/runner-api.md): drawn again at the soonest
  // reset still ahead, which obid sends no event for
  const now = Date.now();
  clearTimeout(sessionTurn);
  const ahead = state.status.accounts.map((a) => Date.parse(a.fiveHourResets ?? '')).filter((t) => t > now);
  if (ahead.length) sessionTurn = setTimeout(() => { accountsFrom = null; publish(); }, Math.min(...ahead) - now + 500);
  replaceAccounts(state.status.accounts.map((a) => accountRow(a, now)));
}

// what a Settings change obid answered changes in its status, shown at once
export function patchStatus(patch: Partial<Status>) {
  if (state.status) set({ status: { ...state.status, ...patch } });
}

// the usage panel opened: obid asks the tools for usage it hasn't read in 10 minutes, which comes
// in an `accounts` event a few seconds later
export function lookAtUsage() {
  if (state.connection === 'live') void api.status().then((status) => set(answerStatus(state, status)), () => {});
}

// --- events

export function apply(s: Live, e: Exclude<Event, { kind: 'transcript' | 'transcriptLine' }>): Live {
  switch (e.kind) {
    // A question's card on a turn changes only the card, and nothing about a turn under way
    case 'job': case 'jobGone': case 'inbox': case 'turnAsks': case 'accounts': case 'github':
      return { ...s, ...applyToObid(s, e) };
    // a conversation's isn't taken as the one this app's words start: another device may start one
    // meanwhile (the words' own turn, or the plan's answer, says which is this app's). Its listing
    // lets go of words kept only for it; its deletion, of the words to it
    case 'conversation': case 'conversationGone':
      return overlaid({ ...s, ...applyToObid(s, e), ...(e.kind === 'conversationGone' ? { pending: goneWith(s.pending, e.id) } : {}) });
    case 'turn': {
      const c = e.turn.conversation, reduced = { ...s, ...applyToObid(s, e) };
      // one the rule kept out (an older copy, a deleted conversation's, or one of none) changes nothing shown
      if (!c || reduced.turns === s.turns) return s;
      // the words this app sent to start one: the conversation their turn is in is the one shown for
      // them. An answer: the words it answers expect nothing more (the next, not here yet, still do)
      const next = overlaid(reduced);
      // one a turn for other words gave (a supervisor's) leaves the last words what they started
      const said = next.turns[c].filter((t) => t.who === 'user').at(-1);
      const answered = {
        unanswered: said && s.on[c] && s.on[c] !== said.id && c in s.working ? new Set([...s.unanswered, said.id]) : s.unanswered,
      };
      // one sent again (its jobs changed): only it changes, unless it's the answer still being shown.
      // (Fetched before its event, the last answer still ends the turn: the plan's answer left that to it)
      if (s.turns[c]?.some((t) => t.id === e.turn.id) && s.pace[c]?.turn !== e.turn.id) {
        return answers(e.turn) && next.turns[c].at(-1)?.id === e.turn.id ? { ...next, ...answered, working: without(s.working, c), words: without(s.words, c) } : next;
      }
      // a job's report or a routine's run ends no turn this app is waiting on, nor says an answer
      // came. It does end what only events said was under way (words stopped on another device:
      // obid sends nothing for that), and a turn really going on says so again at its next step
      if (!answers(e.turn) && e.turn.who === 'oby' && !s.pace[c]?.turn) {
        return inFlight(s, c) ? next : { ...next, working: without(s.working, c), words: without(s.words, c), pace: without(s.pace, c) };
      }
      // Oby's answer: its turn is over, its words final (paced on from those shown, if it was talking here)
      const live = c in s.working || c in s.words || c in s.pace;
      if (e.turn.who === 'oby') return { ...next, ...answered, working: without(s.working, c), words: without(s.words, c), pace: live ? paced(s, c, e.turn.text, e.turn.id) : s.pace };
      // your words after Oby's: the rest of its last answer shows whole
      return s.pace[c]?.turn ? { ...next, pace: without(s.pace, c) } : next;
    }
    case 'step': {
      const c = e.step.conversation ?? Object.keys(s.turns).find((k) => s.turns[k].some((t) => t.id === e.step.turn));
      if (!c || e.step.state !== 'running') return s;
      return { ...s, working: { ...s.working, [c]: e.step.text }, on: { ...s.on, [c]: e.step.turn } };
    }
    case 'delta': {
      const c = e.delta.conversation, words = (s.words[c] ?? '') + e.delta.text;
      return { ...s, working: { ...s.working, [c]: s.working[c] ?? 'Writing' }, on: { ...s.on, [c]: e.delta.turn }, words: { ...s.words, [c]: words }, pace: paced(s, c, words) };
    }
    // the room keeps its own (room.ts); taking one twice changes nothing
    case 'roomPost':
      takeRoomPost(e.post);
      return s;
    case 'peerThread':
      threadChanged(e.owner);
      return s;
  }
}

// --- Oby's words, a word at a time

// about a word each WORD_MS, faster when they pile up, so they never trail by more than MAX_LAG;
// by the time passed, so a throttled window keeps up, and all at once while it's hidden.
// With reduced motion they show as they come.
const WORD_MS = 35, MAX_LAG = 1200;
function paced(s: Live, c: string, to: string, turn?: string): Live['pace'] {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return without(s.pace, c);
  const p = s.pace[c];
  // another turn's words while one is still shown: that one shows whole, this one starts from its first word
  return { ...s.pace, [c]: { to, turn, n: p?.turn && p.turn !== turn ? 0 : p?.n ?? 0 } };
}
let pacing: ReturnType<typeof setInterval> | undefined, ticked = 0;
function tick() {
  const now = performance.now(), dt = now - ticked;
  ticked = now;
  const pace = { ...state.pace };
  let moved = false;
  for (const [c, p] of Object.entries(pace)) {
    // while they come in, only whole words (the last may be cut)
    const all = wordsOf(p.to).length, ready = p.turn || /\s$/.test(p.to) ? all : all - 1;
    const step = document.hidden ? ready : Math.max(Math.floor(dt / WORD_MS), Math.ceil((ready - p.n) * Math.min(1, dt / MAX_LAG)));
    if (p.n < ready) pace[c] = { ...p, n: Math.min(ready, p.n + Math.max(1, step)) };
    else if (p.turn) delete pace[c];  // all shown: its turn takes over
    else continue;
    moved = true;
  }
  // idle until more words come
  if (moved) set({ pace });
  else { clearInterval(pacing); pacing = undefined; }
}

// --- following obid, through main (main/live.ts)

// An answer to a limit's card: the decisions again, read by main, whatever the answer
export async function answerLimit(decision: LimitDecision, option: LimitDecision['options'][number]) {
  try {
    await api.answerLimit(decision, option);
  } finally {
    await window.app?.obid.load({ limits: true });
  }
}

// Main's snapshot, then each message it sends (main.tsx)
export function connect() {
  window.app!.obid.live(snapshot, message);
}

let usageLooked = false;
// What main holds replaces what the page held of obid's; what's the screen's own stays. Once it's
// live, what stays the page's own after obid's lists are read again
function snapshot(s: LiveSnapshot) {
  const obid = unwired(s.state);
  // the folders of conversations deleted since (folders.ts). A list cut short isn't all of them:
  // nothing's dropped then
  if (s.connection === 'live' && s.complete) keepFoldersOf(new Set(obid.conversations.map((c) => c.id)));
  set(overlaid({
    ...state, ...obid, limits: s.limits, connection: s.connection, problem: s.problem, why: s.why,
    firstJobs: state.status || !obid.status ? state.firstJobs : new Set(Object.keys(obid.jobs)),
  }));
  if (s.connection !== 'live') return;
  // what the room missed while the stream was away
  lookAgain();
  lookAgainAtThreads();
  // Settings › Agents' choices kept here before obid kept them: moved to it once (AGENTS-7)
  if (obid.status) void moveKeptChoices(obid.status).then((moved) => { if (moved) lookAtUsage(); });
  // where work can start: GitHub's list takes a moment, so the rest doesn't wait for it
  void api.repos().then(({ repos }) => set({ repos }), () => {});
  // the app opened: obid asks the tools for usage it hasn't read in 10 minutes, as a stream opening did
  if (!usageLooked) { usageLooked = true; lookAtUsage(); }
}

function message(m: LiveMessage) {
  if (m.kind === 'snapshot') return snapshot(m);
  if (m.kind === 'turns') return set(overlaid({ ...state, ...answerTurns(state, m.conversation, m.turns) }));
  if (m.kind === 'limits') return set({ limits: m.limits });
  if (m.kind === 'connection') {
    if (m.connection === 'down') return set({ connection: 'down', problem: m.problem, why: m.why, working: {}, words: {}, pace: {} });
    return set({ connection: m.connection, problem: m.problem, why: m.why });
  }
  const e = m.event;
  takeEvent(e);
  // what a first turn would use (TalkPill) changed once the one obid named can't run
  if (e.kind === 'accounts') {
    const next = e.accounts.find((a) => a.id === state.status?.talkNext);
    if (next && (!next.signedIn || (next.fullUntil && Date.parse(next.fullUntil) > Date.now()))) lookAtUsage();
  }
}

// A conversation's turns, read by main and sent here before it answers (main/live.ts): why not, when not
async function loadTurns(c: string) {
  const done = await window.app!.obid.load({ conversation: c });
  if (done !== true) throw new Error(done ?? 'Oby’s engine is reached from the Mac app only.');
}

// what went wrong with something the app did, for a moment
let clearing: ReturnType<typeof setTimeout> | undefined;
export function say(e: unknown) {
  set({ problem: e instanceof Error ? e.message : String(e) });
  clearTimeout(clearing);
  clearing = setTimeout(() => { if (state.connection === 'live') set({ problem: null }); }, 6000);
}

// --- what the app does

// The message box's repositories, and the one it starts on: the conversation's last, else the
// newest job's (lean.ts: only on obid)
export function useRepos(shown?: string) {
  const s = useLive();
  const last = lastRepo(s, shown);
  return { repos: s.repos.map((r) => r.repo), last: last.here ?? last.anywhere };
}
// The words as obid gets them: with the repository picked in front ("In owner/name: …") unless
// Oby already knows it (the conversation's last) or the words name it. obid's plans take no
// repository of their own.
const inRepo = (words: string, repo: string | undefined, shown?: string) =>
  !repo || repo === lastRepo(state, shown).here || words.toLowerCase().includes(repo.toLowerCase()) ? words : `In ${repo}: ${words}`;

// A conversation opened: its turns fetched once by main (then the events keep them), and the jobs
// they name that aren't among the latest
export async function openConversation(shown: string) {
  const c = realId(shown);
  if (state.loaded.has(c) || c.startsWith('new:')) return;
  set({ loaded: new Set([...state.loaded, c]) });
  try {
    await loadTurns(c);
  } catch (e) {
    set({ loaded: new Set([...state.loaded].filter((x) => x !== c)) });
    say(e);
  }
}

// Words to Oby (POST /v1/plans): in a conversation (its app id), or starting one (none). Your
// words, its answer and the jobs it starts come back as events. Oby's question back waits over
// the message box; your next words answer it. The files that go with them are uploaded first
// (POST /v1/attachments), and the words carry an id of their own: sent again while obid doesn't
// answer, obid takes them once, files and tasks included.
let made = 0;
export function ask(said: string, shown?: string, repo?: string, folders?: string[], att?: Pick<Attachments, 'files' | 'jobs'>): string | undefined {
  const files = att?.files?.filter((f) => f.path) ?? [], jobs = att?.jobs ?? [];
  if (!said.trim() && !files.length && !jobs.length) return;
  const id = shown ?? `new:${++made}`;
  const c = shown ? realId(shown) : null;
  const q = c ? state.waiting[c] : undefined;
  const turn = crypto.randomUUID();
  // an answer to Oby's question is only the answer
  const words = q ? said.trim() : said.trim() ? inRepo(said.trim(), repo, shown) : '';
  if (repo && !q) set({ picked: { ...state.picked, [id]: { repo, at: new Date().toISOString() } } });
  // on their way until obid's state answers for them (pending.ts); words starting a conversation
  // show at once under the app's own id
  set({ pending: added(state.pending, { id: turn, shown: id, conversation: c, said: words, files: files.map(({ name, preview }) => ({ name, preview })) }) });
  // (the rest of Oby's last answer, if it's still being shown, shows whole)
  if (c) set({ waiting: without(state.waiting, c), working: { ...state.working, [c]: 'Thinking' }, on: { ...state.on, [c]: turn }, ...(state.pace[c]?.turn ? { pace: without(state.pace, c) } : {}) });
  let failed = false, planned = false, answerComing = false;
  const paths = files.map((f) => f.path!);
  void (async () => {
    try {
      if (files.length) set({ previews: { ...state.previews, [turn]: files.map((f) => ({ name: f.path!.split('/').pop()!, shown: f.name, preview: f.preview })) } });
      // the working folders go with words that start a conversation, once obid takes them (an
      // older one refuses the key); a conversation's own are obid's already (sendFoldersBy)
      const a = await sent(() => api.plan(q ? q.said : words, q ? { question: q.text, option: words } : undefined, c ? { conversation: c } : { new: true },
        has(state.status, 'folders-v1') && !c ? folders : undefined, { id: turn, attachments: paths, jobs }));
      planned = true;
      const where = a.conversation ?? c;
      // Oby's turn is in a reply or a question (its event may land after this answer, and the card
      // would jump); a `none` has none, unless it's the same words again, answered in their turn
      answerComing = a.kind !== 'none' || a.repeated === true;
      // the conversation the plan's answer names is obid's own for these words: it's the one shown
      // for words starting one (their entry stays until obid lists it, so its page stays)
      set({
        alias: adopted(state.alias, state.pending[turn], a.conversation), pending: planAnswered(state.pending, turn, a, state),
        jobs: a.jobs.reduce(answer, state).jobs,
        ...(a.kind === 'question' && where ? { waiting: { ...state.waiting, [where]: { ...a.question, said: a.said } } } : {}),
      });
      // the turns of the one it started, from the start
      if (!c && where) {
        set({ loaded: new Set([...state.loaded, where]) });
        // (the turns as they are once these come: any that came meanwhile stay)
        await loadTurns(where);
      }
    } catch (e) {
      failed = true;
      // the send failed: its words aren't on their way any more
      if (!planned) set({ pending: sendFailed(state.pending, turn, state) });
      // obid refused the words: their files go too (one it didn't answer may have them)
      if (e instanceof ApiError && e.status) forgetUploads(paths);
      say(e);
    } finally {
      // (words still being shown carry on to their turn, unless it failed; with none they stop)
      const where = c ?? Object.keys(state.alias).find((k) => state.alias[k] === id);
      if (where && !answerComing && !awaitedIn(state.pending, id, where)) set({ working: without(state.working, where), words: without(state.words, where), ...(failed || !state.pace[where]?.turn ? { pace: without(state.pace, where) } : {}) });
    }
  })();
  return id;
}
// Files picked or pasted are uploaded before their words go (POST /v1/attachments), each then
// with the path obid kept. One refused (over 25 MB, say) takes those uploaded before it away
// again, says why, and answers null: the box keeps the words and the files. Off obid, as they are.
export async function uploadFiles<F extends { name: string; file?: File; path?: string }>(files: F[]): Promise<F[] | null> {
  if (!onObid) return files;
  const done: F[] = [];
  try {
    for (const f of files) done.push(f.file && !f.path ? { ...f, path: (await api.upload(f.file, f.name)).path } : f);
    return done;
  } catch (e) {
    forgetUploads(done.flatMap((f) => (f.file && f.path ? [f.path] : [])));
    say(e);
    return null;
  }
}
// uploads whose words never went
export const forgetUploads = (paths: string[]) => { for (const p of paths) void api.unload(p).catch(() => {}); };
// A send obid didn't answer (status 0: the connection went, or obid restarted) is sent again, the
// same request with the same id, a few times a second apart; any answer of obid's is final.
export async function sent<T>(send: () => Promise<T>, tries = 3, pause = 1000): Promise<T> {
  for (let n = 1; ; n++) {
    try { return await send(); } catch (e) {
      if (!(e instanceof ApiError && e.status === 0) || n >= tries) throw e;
      await new Promise((done) => setTimeout(done, pause));
    }
  }
}
// One click, one key (docs/contracts/receipts.md, RECEIPTS-16), only once obid says it takes keys
export const requestKey = () => (state.status?.receipts === true ? crypto.randomUUID() : undefined);
// One click's request: with its key, made before the first try, sent again while obid doesn't
// answer (sent), every try the same body and key; with none (an obid that takes no key), once
export function keyed<T>(send: (key?: string) => Promise<T>): Promise<T> {
  const key = requestKey();
  return key ? sent(() => send(key)) : send();
}

export async function renameConversation(shown: string, title: string) {
  const t = title.trim();
  if (!t) return;
  try { const c = await api.editConversation(realId(shown), { title: t }); take({ kind: 'conversation', conversation: c }); } catch (e) { say(e); }
}
// The account a conversation talks through from its next turn (TalkPill). One obid hasn't named yet
// keeps its choice until it has: TalkPill isn't shown for it.
export async function chooseTalk(shown: string, talkAccount: string) {
  const c = realId(shown);
  if (c.startsWith('new:')) return;
  take({ kind: 'conversation', conversation: await api.editConversation(c, { talkAccount }) });
}
export const talkReady = (shown: string) => !realId(shown).startsWith('new:');
export async function pinConversation(shown: string, pinned: boolean) {
  try { const c = await api.editConversation(realId(shown), { pinned }); take({ kind: 'conversation', conversation: c }); } catch (e) { say(e); }
}
// Stop (CHATFIRST-33): Oby's turn event ends the working line, as any answer's does
export async function stopConversation(shown: string) {
  try { await api.stopConversation(realId(shown)); } catch (e) { say(e); }
}
export async function deleteConversation(shown: string) {
  const c = realId(shown);
  // hidden at once, but not as deleted: only obid's own event says so, and one it refuses can come back
  set({ conversations: state.conversations.filter((x) => x.id !== c), turns: without(state.turns, c) });
  try { await api.deleteConversation(c); } catch (e) { if (!(e instanceof ApiError && e.status === 404)) say(e); }
}

// Told a job's agent something (POST /v1/jobs/{id}/messages). conversation: from its page, so the
// words go into the task's conversation, which obid makes now for a task that has none
// (TASKCONV-4); the job, which names it, comes back
export async function tellJob(id: string, text: string, files: { path?: string }[] = [], conversation = false): Promise<Job | undefined> {
  const paths = files.flatMap((f) => (f.path ? [f.path] : []));
  try {
    const j = await keyed((key) => api.message(id, text.trim(), paths, conversation, key));
    // the follow-up it started, and the task itself, now in the conversation
    const told = j.id === id ? j : await api.job(id).catch(() => undefined);
    take({ kind: 'job', job: j });
    if (told && told.id !== j.id) take({ kind: 'job', job: told });
    return j;
  } catch (e) {
    // obid refused (files go only with a follow-up of a finished task): its files go too
    if (e instanceof ApiError && e.status) forgetUploads(paths);
    say(e);
  }
}
// Told from the agent's own conversation, opened in place under its card (AgentThread.tsx): the follow-up it started, if it had finished
export const tellAgent = async (id: string, text: string, files?: { path?: string }[]) => {
  const j = await tellJob(id, text, files, true);
  return j && j.id !== id ? j.id : undefined;
};
// A task's Mode menu: how much it asks from now on (POST /v1/jobs/{id}/mode)
export async function changeMode(id: string, mode: string) {
  try { const j = await api.mode(id, mode); take({ kind: 'job', job: j }); } catch (e) { say(e); }
}
// A click on its way (pending.ts), on obid's copy as it is now; and its entry gone (its call failed)
function act(what: Action['what'], where: Action['in'], on: string, more: Pick<Action, 'option' | 'ask' | 'title'> = {}): Action {
  const a: Action = { what, in: where, on, of: copyOf(state, { what, in: where, on, of: {} }) ?? {}, ...more };
  set({ actions: acting(state.actions, a) });
  return a;
}
const drop = (a: Action) => set({ actions: unacted(state.actions, a) });
export async function stopJob(id: string) {
  const a = act('stop', 'jobs', id);
  try { const j = await keyed((key) => api.stop(id, key)); take({ kind: 'job', job: j }); } catch (e) { drop(a); say(e); }
}

// The task's menu: Rename (PATCH /v1/jobs/{id}), shown at once as the mock does it; Archive
// (POST /v1/jobs/{id}/archive), off Home into Archive; Delete (DELETE /v1/jobs/{id}), gone for good.
// obid's job comes back and the task follows it; a refusal (a 409: still running, its pull request
// still open) shows in obid's own words and leaves the task as it was.
export async function renameJob(id: string, title: string) {
  // the same name again (a blur with nothing changed): nothing to tell obid
  if ((actionOn(state.actions, 'rename', id)?.title ?? state.jobs[id]?.title) === title) return;
  const a = act('rename', 'jobs', id, { title });
  try { const j = await api.rename(id, title); take({ kind: 'job', job: j }); } catch (e) { drop(a); say(e); }
}
export async function archiveJob(id: string) {
  const a = act('archive', 'jobs', id);
  try { const j = await api.archive(id); take({ kind: 'job', job: j }); } catch (e) { drop(a); say(e); }
}
// true once it's gone (obid deleted it, or had no such job). With commits that aren't published,
// obid asks first: its Confirm comes onto the task's card (and Home's stack), naming the commits
// lost, and only the owner's click there deletes it (answerJob, signed by main).
// (obid asking first leaves the job as it was: its entry goes then too)
export async function deleteJob(id: string) {
  const a = act('delete', 'jobs', id);
  try {
    const { deleted, ask } = await api.deleteJob(id);
    if (ask) take({ kind: 'inbox', item: ask });
    if (!deleted) return false;
  } catch (e) {
    if (!(e instanceof ApiError && e.status === 404)) { say(e); return false; }
  } finally { drop(a); }
  take({ kind: 'jobGone', id });
  return true;
}

// A job's session (liveSession.ts)
export { entryDetail, loadSession, sessionNow, useFollowedSession, useSession } from './liveSession';

// Work on a GitHub item, on the owner's tap (POST /v1/github/work/{id}/start): its job comes back,
// and the feed's next event takes the item off To do. A review in a repository Oby doesn't work in
// starts nothing yet: its Confirm comes into Home's stack, and obid's words say so for a moment.
export async function startGitHubWork(id: string) {
  try { const j = await api.startGitHubWork(id); take({ kind: 'job', job: j }); } catch (e) { say(e); }
}

// A job's decision answered: its permission through obid's own route (POST /v1/jobs/{id}/permission),
// anything else in the inbox; Later puts it off an hour. A Confirm goes with this Mac's device
// signature, which main adds (#776). id: a job's, or an item's with no task (asksOf), answered from
// Home's stack.
// On Oby's ask for a check, Use it is the owner's Confirm, No check (or Keep it) their Cancel; Edit
// sends the command they typed (words) to POST /v1/repos/check, which asks for it in the ask's
// place, confirmed here at once with this Mac's signature over that ask's title.
const DECISIONS: Record<string, 'allow' | 'always' | 'deny'> = { Allow: 'allow', [ALLOW_ONCE]: 'allow', 'Always allow': 'always', Deny: 'deny' };
const settleQuestion = async (item: InboxItem) => {
  if (!takesNoAnswer(item)) take({ kind: 'inbox', item: await keyed((key) => api.answer(item.id, { answer: 'Done' }, key)) });
};
export async function answerJob(id: string, option: string, words?: string) {
  if (id.startsWith('limit:')) return answerLimitAsk(id, option).catch(say);
  const j = state.jobs[id], item = itemOf(state, id) ?? state.inbox.find((i) => i.id === id), ask = j && askOf(state, j);
  const shown = option === 'Edit' && words ? `Use ${words}` : (option === TELL || option === STEER) && words ? words : option;
  // on its way by what it answers (an item, else the job's own card), as the card asked it
  const a = act('answer', item ? 'inbox' : 'jobs', item?.id ?? id, { option: shown, ...(ask ? { ask } : {}) });
  try {
    if (checkOf(item) && option !== 'Later') {
      let asked = item!;
      if (option === 'Edit' && words) {
        // (the card stays answered: obid closes this ask, and the new one has no job)
        const changed = await api.changeCheck(asked.confirm!.target, words);
        if (!changed.ask) return;
        asked = changed.ask;
      }
      const next = option === 'No check' || option === 'Keep it' ? await keyed((key) => api.answer(asked.id, { answer: 'Cancel' }, key)) : await api.confirm(asked.id);
      take({ kind: 'inbox', item: next });
      void api.repos().then(({ repos }) => set({ repos }), () => {});
    } else if (item?.kind === 'confirm' && option === 'Confirm') {
      take({ kind: 'inbox', item: await api.confirm(item.id) });
      // on past a safety limit, what Oby's own question asked is answered
      const question = state.inbox.find((i) => i.job === id && i.kind === 'question');
      if (question && (item.confirm?.action === 'retry_job' || item.confirm?.action === 'message_job')) await settleQuestion(question);
    }
    else if (item?.kind === 'permission' && j?.permission && DECISIONS[option]) {
      const next = await api.permission(id, { id: j.permission.id, decision: DECISIONS[option] });
      take({ kind: 'job', job: next });
    } else if (item?.kind === 'question' && j && (option === TELL || option === STEER)) {
      // obid leaves the question open while the task runs on, so it's settled here; refused, it's
      // still asked. Tell it what to do (TASKS-17) is the task page's shape: conversation true, then Done
      if (await tellJob(id, words!, [], option === STEER)) await settleQuestion(item);
      drop(a);
    } else if (item) {
      // (an agent's question by its own item, never the job's permission, which can lag behind the
      // inbox: Tell it something sends the owner's words as its answer)
      const said = (option === TELL || option === STEER) && words ? words : option;
      const body = option === 'Later' ? { later: new Date(Date.now() + 3600_000).toISOString().replace(/\.\d+Z$/, 'Z') } : { answer: said };
      take({ kind: 'inbox', item: await keyed((key) => api.answer(item.id, body, key)) });
    }
  } catch (e) {
    // not taken: it's still yours to answer
    drop(a);
    say(e);
  }
}
// A failed or interrupted task's menu (TASKS-25). Try again tells its agent obid's Try-again words
// (docs/runner-api.md), signed for that click; one that never started is run again by obid as
// itself (TASKS-20). Not for a job an account's limit holds: only its limit card's signed choice
// carries it on (#1563). Taken, Oby's own question about it is answered Done; refused, obid's
// words show (tellJob). Stop here lets it go: obid settles that question itself (TASKS-13).
export async function tryAgainJob(id: string) {
  const j = state.jobs[id];
  if (j && limitHolds(state, j)) return;
  const question = state.inbox.find((i) => i.job === id && i.kind === 'question' && isEscalation(i));
  try {
    if ((await tellJob(id, TRY_AGAIN)) && question) await settleQuestion(question);
  } catch (e) { say(e); }
}
export async function letGoJob(id: string) {
  try { take({ kind: 'job', job: await api.dismiss(id) }); } catch (e) { say(e); }
}
// Change check, on a task's page (#1062): the owner's own command for its repository, or none to
// start over. obid asks for it (POST /v1/repos/check) and this Mac's signature over that ask's
// title, given at once, keeps it: the same Confirm as Use it. Nothing to keep (starting over with
// no check kept) forgets a No check at once.
export async function changeCheck(repo: string, check: string | null) {
  try {
    const changed = await api.changeCheck(repo, check);
    const repos = state.repos.map((r) => (r.repo === changed.repo.repo ? changed.repo : r));
    set({ repos });
    if (changed.ask) {
      take({ kind: 'inbox', item: changed.ask });
      take({ kind: 'inbox', item: await api.confirm(changed.ask.id) });
      set({ repos: (await api.repos()).repos });
    }
  } catch (e) {
    say(e);
  }
}

// The owner's answer to another Oby's question, or their Decline (or a review request's answer,
// AskStack.tsx): signed by main, or obid refuses it, and that refusal is the caller's to say, over
// the words it keeps (QuestionCard.tsx). On its way as a reply (pending.ts), its card shown meanwhile.
export async function answerQuestion(id: string, words: string) {
  const a = act('reply', 'inbox', id, { option: words });
  try { take({ kind: 'inbox', item: await keyed((key) => api.answer(id, { answer: words }, key)) }); } catch (e) { drop(a); throw e; }
}

// An offer answered (OfferCard.tsx), for each of its jobs still waiting: Dispatch is the owner's own
// Start, which only this tap gives (POST /v1/inbox/{id}/answer); Skip cancels it; Later puts it
// off an hour. What starts comes as its job's event, its card under the answer.
export async function answerOffer(offer: string, option: 'Dispatch' | 'Skip' | 'Later') {
  const items = state.inbox.filter((i) => i.offer?.id === offer);
  const acts = items.map((i) => act('answer', 'inbox', i.id, { option }));
  for (const [n, item] of items.entries()) {
    const body = option === 'Later' ? { later: new Date(Date.now() + 3600_000).toISOString().replace(/\.\d+Z$/, 'Z') } : { answer: option === 'Dispatch' ? 'Start' : 'Cancel' };
    try { take({ kind: 'inbox', item: await keyed((key) => api.answer(item.id, body, key)) }); } catch (e) { drop(acts[n]); say(e); }
  }
}

export function markSeen(id: string) {
  if (!state.seen.has(id)) set({ seen: new Set([...state.seen, id]) });
}
