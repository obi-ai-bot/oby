import { useSyncExternalStore } from 'react';
import { ApiError, api, parseEvent, sseReader, type Conversation, type Event, type InboxItem, type Job, type LimitDecision, type Question, type Repo, type Status, type TranscriptEntry, type Turn } from './obid';
import { pageOf, stagesOf } from './jobPage';
import { AGENTS, TELL, type Ask, type AskKind, type Job as JobView, type Message, type Offer } from './mock';
import { replaceChats, type Chat } from './chats';
import { replaceTasks, type Asking, type Task } from './tasks';
import { keepFoldersOf, nameFoldersBy, saveFolders, sendFoldersBy, takeFolders } from './folders';
import { moveKeptChoices, replaceAccounts } from './SettingsView';
import { BRANDS } from './brands';
import type { GitHubItem } from './mock';
import type { GitHubWork } from './obid';

// What obid has (docs/runner-api.md), kept live by its event stream, as apps/web/src/live.ts does
// it: the stream first, then the status, jobs, inbox and conversations, with the events that came
// meanwhile applied over them. Sangam's screens don't change: what obid has goes into the same
// stores the mock filled (chats.ts, tasks.ts), as conversations and tasks. Without ?everything
// (lean.ts) the app runs on this; with it, on the mock.
type Live = {
  connection: 'connecting' | 'live' | 'down';
  problem: string | null;        // why obid isn't answering (its own reason, main/obid.ts), or what last went wrong
  why?: 'waiting' | 'approval';  // and what kind of why (main/obid.ts, reason): 'waiting', none yet
  status: Status | null;
  repos: Repo[];                 // where a job may start (GET /v1/repos)
  github: GitHubWork | null;     // what GitHub wants from the owner (GET /v1/github/work), none from an older obid
  jobs: Record<string, Job>;
  firstJobs: Set<string>;        // the jobs there when the app opened: never new
  seen: Set<string>;             // jobs opened since
  inbox: InboxItem[];            // open and put off
  limits: LimitDecision[];       // one decision for the jobs stopped by an account's limit
  conversations: Conversation[];
  turns: Record<string, Turn[]>; // by conversation: each opened one's, and any that came since
  loaded: Set<string>;           // the conversations whose turns were fetched
  working: Record<string, string>;            // a turn under way, by conversation: what it's on
  words: Record<string, string>;              // its words so far (delta events), by conversation
  // Oby's words as shown, by conversation: a word at a time toward `to` (its words so far, then its
  // turn's once that's in; the turn waits, hidden, until they're all shown). History shows at once.
  pace: Record<string, { to: string; n: number; turn?: string }>;
  waiting: Record<string, Question & { said: string }>;  // Oby's question back, by conversation
  // words to Oby starting a new conversation, shown at once under an id of the app's own
  // ('new:1'); obid's id for it, once known, maps onto that one (alias), so its page stays open
  starting: { id: string; said: string } | null;
  alias: Record<string, string>;
  // decisions answered here, by job: what was asked, the pick, and the inbox item it settles.
  // The cards keep showing it answered (as the mock's do), and the stack lets it go.
  answered: Record<string, { ask: Ask; option: string; item?: string }>;
  // items with no task answered from Home's stack, their answer on its way: off the stack meanwhile
  answering: Set<string>;
  // the repository last picked for words sent here, by conversation (its app id), and when
  picked: Record<string, { repo: string; at: string }>;
  sessions: Record<string, TranscriptEntry[]>;  // each job's session opened (Full session), by job
};

let state: Live = {
  connection: 'connecting', problem: null, status: null, repos: [], github: null, jobs: {}, firstJobs: new Set(), seen: new Set(), inbox: [], limits: [],
  conversations: [], turns: {}, loaded: new Set(), working: {}, words: {}, pace: {}, waiting: {}, starting: null, alias: {}, answered: {}, answering: new Set(), picked: {}, sessions: {},
};
const listeners = new Set<() => void>();
const set = (patch: Partial<Live>) => {
  state = { ...state, ...patch }; publish(); listeners.forEach((l) => l());
  if (!pacing && Object.keys(state.pace).length) { ticked = performance.now(); pacing = setInterval(tick, WORD_MS); }
};
export const useLive = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => state);
// the limit decisions for a conversation's or a job's work (ChatView), as Home's stack shows them
export const useLimitAsks = (conversation?: string, job?: string) => { useLive(); return limitAsks(state, conversation, job); };
export const useLimitCount = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => state.limits.length);
export const liveState = () => state;

const minutesAgo = (iso: string | null | undefined) => (iso ? Math.max(0, (Date.now() - Date.parse(iso)) / 60_000) : 0);
const byAt = (a: Turn, b: Turn) => a.at.localeCompare(b.at);
const merged = (had: Turn[], more: Turn[]) => [...had.filter((t) => !more.some((m) => m.id === t.id)), ...more].sort(byAt);
const without = <T,>(r: Record<string, T>, key: string) => { const { [key]: _, ...rest } = r; return rest; };
// the app's id for one of obid's conversations, and back
export const shownId = (c: string) => state.alias[c] ?? c;
export const realId = (shown: string) => Object.keys(state.alias).find((k) => state.alias[k] === shown) ?? shown;
// a new conversation's working folders, saved under obid's id once it has one (folders.ts)
nameFoldersBy(realId);
// once obid takes folders, a conversation's go to it (one not yet named keeps its own meanwhile)
sendFoldersBy((shown, folders) => {
  const c = realId(shown);
  if (!state.status?.folders || c.startsWith('new:')) return undefined;
  return api.editConversation(c, { folders }).then((conversation) => set(apply(state, { kind: 'conversation', conversation })));
}, say);
let aliasSeen: Live['alias'] | undefined;

// --- obid's state as the screens' conversations and tasks

// What Oby offers to start in a conversation (obid's oby_offer), or in any (c: none, Home's
// stack): its start items still waiting, one card per offer (a turn's), in the order they came.
// Put off with Later, a card is gone until its time.
function offersOf(s: Live, c?: string, items = s.inbox): Offer[] {
  const cards = new Map<string, InboxItem[]>();
  for (const i of items) if (i.kind === 'start' && i.offer && (c === undefined || i.conversation === c)) cards.set(i.offer.id, [...(cards.get(i.offer.id) ?? []), i]);
  return [...cards].filter(([, items]) => items.some((i) => !i.later || Date.parse(i.later) <= Date.now())).map(([id, items]) => ({
    id, title: items.length === 1 ? items[0].title : `${items.length} jobs`,
    jobs: items.map((i) => ({ what: i.offer!.what, task: i.offer!.task })), untrusted: items.some((i) => i.offer!.untrusted),
  }));
}

function chatOf(s: Live, c: Conversation): Chat {
  // a task's card under the answer that started it, not again under each that mentions it. A
  // job your words started or were offered (its turn), and what Oby offered for them (the offer's
  // id), go under Oby's answer to them: its first turn after them that isn't a job's report or a
  // routine's run
  const carded = new Set<string>();
  const p = s.pace[c.id];
  const byTurn = Object.values(s.jobs).filter((j) => j.conversation === c.id && j.turn).sort((a, b) => a.created.localeCompare(b.created));
  const offers = offersOf(s, c.id);
  const said: Message[] = [];
  let asked: string | undefined;
  for (const t of (s.turns[c.id] ?? []).filter((t) => t.id !== p?.turn)) {
    const answer = t.who === 'oby' && !t.ended && !t.routine ? asked : undefined;
    const mine = answer ? byTurn.filter((j) => j.turn === answer).map((j) => j.id) : [];
    const jobs = [...t.jobs, ...mine].filter((j) => !carded.has(j) && carded.add(j));
    said.push({ who: t.who === 'user' ? 'you' : 'oby', text: t.text, ago: minutesAgo(t.at), ...(jobs.length ? { jobs } : {}) });
    for (const offer of offers.filter((o) => o.id === answer)) said.push({ who: 'oby', text: '', ago: minutesAgo(t.at), offer });
    if (t.who === 'user') asked = t.id;
    else if (answer) asked = undefined;
  }
  const words = p ? wordsOf(p.to).slice(0, p.n).join('') : s.words[c.id], q = s.waiting[c.id];
  if (p || words) said.push({ who: 'oby', text: words ?? '', ago: 0, streaming: true });
  // its answer still coming (under the words so far), or an event turn's offer: last
  const placed = new Set(said.flatMap((m) => (m.offer ? [m.offer.id] : [])));
  for (const offer of offers.filter((o) => !placed.has(o.id))) said.push({ who: 'oby', text: '', ago: 0, offer });
  return {
    id: s.alias[c.id] ?? c.id, title: c.title, pinned: c.pinned, at: minutesAgo(c.lastActivity), said,
    about: c.tasks?.length ? `Started ${c.tasks.length} task${c.tasks.length === 1 ? '' : 's'}` : '',
    typing: c.id in s.working, step: p || words ? undefined : s.working[c.id],
    waiting: q ? { question: q.text, options: q.options } : undefined,
  };
}
function chatsOf(s: Live): Chat[] {
  const chats = [...s.conversations].sort((a, b) => b.lastActivity.localeCompare(a.lastActivity)).map((c) => chatOf(s, c));
  // words starting a conversation obid hasn't listed yet: shown under the app's id meanwhile
  const st = s.starting;
  if (!st || chats.some((c) => c.id === st.id)) return chats;
  const real = Object.keys(s.alias).find((k) => s.alias[k] === st.id);
  const now = new Date().toISOString();
  const shown = chatOf(s, { id: real ?? st.id, title: st.said.split('\n')[0].slice(0, 48), pinned: false, created: now, lastActivity: now, cursor: '' });
  return [{ ...shown, id: st.id, said: shown.said.length ? shown.said : [{ who: 'you', text: st.said, ago: 0 }], typing: true, step: shown.step ?? 'Thinking' }, ...chats];
}

// The decision a job's card shows, of its open inbox items: what its agent waits on first (a
// permission or a question, its agent's own among them), else the oldest (an offer like a check to
// keep). The card holds one.
const WAITS = new Set(['permission', 'question', 'agentQuestion']);
const itemOf = (s: Live, job: string) => {
  const items = s.inbox.filter((i) => i.job === job);
  return items.find((i) => WAITS.has(i.kind)) ?? items[0];
};

// What a job needs from you: that decision (a permission's command as its detail); or, failed or
// interrupted and not let go, letting it go
function askOf(s: Live, j: Job): Ask | undefined {
  const item = itemOf(s, j.id);
  // The grouped limit appears once, but unrelated permissions or questions still need attention.
  if ((j.accountLimit || s.limits.some((d) => d.jobs.some((job) => job.id === j.id))) && (!item || item.kind === 'limit')) return undefined;
  if (item) return itemAsk(item, item.kind === 'permission' ? j.permission?.detail : undefined);
  if ((j.status === 'failed' || j.status === 'interrupted') && !j.dismissed) {
    return { kind: 'failed', headline: j.status === 'failed' ? 'It failed' : 'It was interrupted', text: j.summary.split('\n')[0] || j.step, options: ['Dismiss'] };
  }
}

// An item's decision as a card shows it; detail: a permission's command, else the review a ready
// review's Approve posts (as text). A GitHub offer is Sangam's "Start it?", as on To do's cards,
// an Oby failure his "Failed", and the agent's own question (#858) his Question, the question its
// headline as on his cards, quoted as the agent wrote it, its options and his Tell it something
// for the owner's own words (obid takes any words as the answer), so one with no options is
// answered too.
const LABELLED: Record<string, AskKind> = { github: 'start', failure: 'failed', agentQuestion: 'question' };
export const itemAsk = (item: InboxItem, detail?: string): Ask => {
  // Oby's ask for a repository's check (set_check): the command under the question, and the
  // answers in plain words (answerJob turns them into obid's Confirm and Cancel)
  // (Cancel keeps the check a repository has; with none it's No check)
  const check = checkOf(item);
  if (check) {
    const options = ['Use it', 'Edit', item.confirm!.payload?.was ? 'Keep it' : 'No check'];
    return { kind: 'check', headline: `Use this as ${item.confirm!.target}’s check before publishing?`, text: item.text, options, detail: check };
  }
  if (item.kind === 'agentQuestion') return { kind: 'question', headline: item.text, text: '', options: [...item.options, TELL] };
  return {
    kind: LABELLED[item.kind] ?? item.kind as AskKind, headline: item.headline ?? item.title, text: item.headline ? item.text : '',
    options: item.options, recommended: item.recommended ?? undefined, detail: detail ?? item.watch?.review ?? undefined,
  };
};
// a check to keep: one taking a check out shows as any confirm item, its Confirm as such
const checkOf = (item?: InboxItem) =>
  (item?.kind === 'confirm' && item.confirm?.action === 'set_check' ? item.confirm.payload?.check ?? undefined : undefined);

// What obid asks the owner with no task to show it on (a job's item shows on its task's card), for
// Home's stack, of the kinds it was made for: GitHub's offers, Confirms, failures, and work Oby
// offers to start (its job isn't one until it starts), as OfferCard shows it in its conversation.
// Any other kind stays where it's shown in full (an Obi-to-Obi request must show all it quotes
// before it's answered). Only what needs the owner (as obid sorts it: needs_you, pending with it),
// not put off, and not while its answer is on its way.
const ON_HOME = new Set(['github', 'confirm', 'start', 'failed', 'failure']);
function asksOf(s: Live): Asking[] {
  const now = Date.now();
  const open = s.inbox.filter((i) => ON_HOME.has(i.kind) && !s.jobs[i.job] && (!i.triage || i.triage === 'needs_you' || i.triage === 'pending')
    && !(i.later && Date.parse(i.later) > now) && !s.answering.has(i.id));
  return [
    ...open.filter((i) => !i.offer).map((i) => {
      // over it a GitHub item's own title, as on To do; obid's words under Oby's headline, or under
      // the title till one comes (clamped: they can be long)
      const g = i.github;
      return {
        id: i.id, ask: { ...itemAsk(i), text: i.text }, title: g?.quoted?.title ? `#${g.number} ${g.quoted.title}` : i.title,
        ...(g ? { who: g.list === 'reviews' ? g.author ?? undefined : undefined, brand: BRANDS.github, url: g.url } : {}),
      };
    }),
    // an offer under the conversation it was made in, the way to it one click away
    ...offersOf(s, undefined, open).map((offer) => {
      const c = open.find((i) => i.offer?.id === offer.id)?.conversation;
      return { id: offer.id, title: s.conversations.find((x) => x.id === c)?.title ?? offer.title, offer, talk: c ? shownId(c) : undefined };
    }),
    ...limitAsks(s),
  ];
}

// An account's limit (#1563): one card for all the jobs it stopped, its answers obid's options. The
// card's id carries the revision it shows, so a choice covers exactly the jobs and destination on
// it: once obid's decision changes, the old card's answer is refused (answerJob) and a new card
// takes its place. conversation/job: only the decisions for that conversation's or job's work.
const limitLabel = (o: LimitDecision['options'][number]) =>
  o.kind === 'move' ? `Move all to ${o.name} · ${o.agent}` : o.kind === 'resume' ? 'Resume when available' : 'Keep stopped';
const limitLater = new Map<string, number>();  // Later: put off an hour, in this window only
const limitError = new Map<string, string>();  // why the last answer to it didn't go
export function limitAsks(s: Live, conversation?: string, job?: string): Asking[] {
  const now = Date.now();
  return s.limits.filter((d) => (limitLater.get(`${d.id}:${d.revision}`) ?? 0) < now && !s.answering.has(`limit:${d.id}:${d.revision}`)
    && (!conversation && !job || d.jobs.some((j) => job === j.id || (!!conversation && s.jobs[j.id]?.conversation === realId(conversation)))))
    .map((d) => {
      const source = s.status?.accounts.find((a) => a.id === d.account);
      const reset = d.resetAt != null && Number.isFinite(d.resetAt)
        ? new Date(d.resetAt * 1000).toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' }) : null;
      const n = d.jobs.length;
      const text = [
        `${n} ${n === 1 ? 'task is' : 'tasks are'} waiting: ${d.jobs.map((j) => j.title).join(', ')}.`,
        reset ? `It resets ${reset}.` : 'Its reset time is unknown.',
        source?.fiveHour != null ? `Five-hour usage: ${Math.round(source.fiveHour * 100)}%.` : '',
        limitError.get(d.id) ?? d.problem ?? '',
      ].filter(Boolean).join(' ');
      return {
        id: `limit:${d.id}:${d.revision}`, title: source?.name ?? d.agent,
        brand: AGENTS[d.agent as keyof typeof AGENTS]?.brand,
        ask: { kind: 'limit', headline: `${source?.name ?? d.agent} reached its limit`, text, options: d.options.map(limitLabel) },
      };
    });
}
// its answer: the option shown, sent with the revision shown (signed by main, device.ts)
async function answerLimitAsk(id: string, option: string) {
  const [, decisionId, revision] = id.split(':');
  if (option === 'Later') { limitLater.set(`${decisionId}:${revision}`, Date.now() + 3600_000); set({}); return; }
  const decision = state.limits.find((d) => d.id === decisionId && String(d.revision) === revision);
  const picked = decision?.options.find((o) => limitLabel(o) === option);
  if (!decision || !picked) throw new Error('This choice changed. Look at it again before choosing.');
  answering(id, true);
  try {
    await answerLimit(decision, picked);
    limitError.delete(decisionId);
  } catch (e) {
    limitError.set(decisionId, `Couldn’t save your choice: ${e instanceof Error ? e.message : String(e)}`);
    throw e;
  } finally { answering(id, false); }
}

// GitHub's work no job is on yet (once one is, the job is the task), as To do's cards: reviews and
// your own pull requests under Review PR (as the Swift app had them: Sangam's design has no group
// for your own), assigned issues under Issues. A tap starts Oby's job where it can: not on a draft,
// an issue in a repository Oby doesn't work in, or your own pull request unless its checks failed
// on a branch of the repository itself; a review in a repository Oby doesn't work in asks your
// Confirm first, which adds it.
const REVIEWED: Record<string, string> = { APPROVED: 'Approved', CHANGES_REQUESTED: 'Changes requested', REVIEW_REQUIRED: 'Waiting for review' };
const CHECKED: Record<string, string> = { SUCCESS: 'checks passed', FAILURE: 'checks failed', ERROR: 'checks failed', PENDING: 'checks running', EXPECTED: 'checks running' };
function workOf(w: GitHubWork | null): GitHubItem[] {
  return (w ? [...w.reviews, ...w.mine, ...w.assigned] : []).filter((g) => !g.job).map((g) => {
    const name = g.repo.split('/')[1] ?? g.repo, at = `${name} #${g.number}`;
    const red = g.checks === 'FAILURE' || g.checks === 'ERROR';
    const action = g.draft ? undefined : g.list === 'reviews' ? 'Review it' as const
      : g.known && (g.list === 'assigned' || (red && !g.fork)) ? 'Fix it' as const : undefined;
    // where your own stands: "Changes requested, checks failed."
    const stands = [REVIEWED[g.reviewDecision ?? ''], CHECKED[g.checks ?? '']].filter(Boolean).join(', ');
    return {
      id: g.id, type: g.list === 'assigned' ? 'issues' : 'review', who: g.list === 'reviews' ? g.author ?? undefined : undefined,
      title: `#${g.number} ${g.title}`, repo: name, url: g.url, board: g.board ?? undefined, updated: minutesAgo(g.updated), action,
      request: g.list === 'reviews' ? `${g.author ?? 'Someone'} asked you to review ${at}.` : g.list === 'assigned' ? `Assigned to you in ${at}.`
        : stands ? `${stands[0].toUpperCase()}${stands.slice(1)}.` : `Open in ${at}.`,
      label: action ? undefined : g.list === 'mine' ? 'Your pull request' : 'On GitHub',
      note: g.list === 'mine' ? undefined : g.draft ? 'It’s a draft: Oby can review it once it’s ready for review.' : g.known ? undefined
        : g.list === 'reviews' ? `Oby doesn’t work in ${g.repo} yet: Review it asks you to confirm adding it.` : `Oby doesn’t work in ${g.repo} yet: add it to repos in obid’s configuration.`,
    };
  });
}

function taskOf(s: Live, j: Job): Task {
  // answered here: answered until obid asks something else
  const now = askOf(s, j), was = s.answered[j.id];
  const answer = was && (!now || itemOf(s, j.id)?.id === was.item) ? was.option : undefined;
  const ask = answer ? was!.ask : now;
  const job: JobView = {
    id: j.id, type: j.kind === 'review' ? 'review' : j.issue != null ? 'issues' : 'tasks', title: j.title, request: j.request,
    agent: j.agent, account: s.status?.accounts.find((a) => a.id === j.account)?.name ?? j.account,
    model: j.model ?? s.status?.models[j.agent]?.model ?? 'default', repo: j.repo, branch: j.branch || '—', base: j.base ?? '—',
    check: j.check ?? undefined, repoCheck: s.repos.find((r) => r.repo === j.repo)?.check, status: j.status, step: j.step, waiting: j.waiting ?? undefined,
    round: j.rounds > 1 ? [j.round, j.rounds] : undefined,
    stages: stagesOf(j),
    ask, summary: j.summary || undefined, commits: j.commits ?? undefined, published: j.published ?? undefined,
    created: minutesAgo(j.created), finished: j.finished ? minutesAgo(j.finished) : undefined, conversation: j.conversation ? shownId(j.conversation) : undefined,
    archived: !!j.archived,
  };
  const needs = (!!now && !answer) || answer === 'Later';
  job.page = pageOf(j, s.status?.accounts.find((a) => a.id === j.account), needs);
  return {
    id: j.id, type: job.type, title: j.title, brand: AGENTS[j.agent]?.brand ?? AGENTS.claude.brand, job,
    status: needs ? 'needs' : j.status === 'queued' || j.status === 'running' ? 'running' : 'done',
    fresh: !s.firstJobs.has(j.id) && !s.seen.has(j.id), answer,
  };
}
const tasksOf = (s: Live) => Object.values(s.jobs).sort((a, b) => b.created.localeCompare(a.created)).map((j) => taskOf(s, j));

// into the stores the screens read
let waking: ReturnType<typeof setTimeout> | undefined;
let accountsFrom: Status | null = null;
let sessionTurn: ReturnType<typeof setTimeout> | undefined;
function publish() {
  if (state.alias !== aliasSeen) { aliasSeen = state.alias; saveFolders(); }
  if (state.status?.folders) takeFolders(Object.fromEntries(state.conversations.map((c) => [shownId(c.id), c.folders ?? []])));
  replaceChats(chatsOf(state));
  replaceTasks(tasksOf(state), { asks: asksOf(state), work: workOf(state.github), error: state.github?.error ?? undefined });
  // what's put off comes back when its time comes, which obid sends no event for: look again then
  // (the soonest, and at least hourly)
  clearTimeout(waking);
  const due = Math.min(...state.inbox.map((i) => (i.later ? Date.parse(i.later) : Infinity)).filter((t) => t > Date.now()));
  if (due < Infinity) waking = setTimeout(() => set({}), Math.min(due - Date.now() + 500, 3600_000));
  // the week's share, else the 5-hour window's; unknown until the tool says (never a made-up 0%).
  // Cursor's CLI reports no usage at all. Only when obid's status changed: publish runs at every
  // change, each word of Oby's included, and Settings › Accounts and the usage gauge draw again
  // for each list.
  if (!state.status || state.status === accountsFrom) return;
  accountsFrom = state.status;
  // a 5-hour window past its reset reads as empty (docs/runner-api.md): drawn again at the soonest
  // reset still ahead, which obid sends no event for
  const now = Date.now();
  clearTimeout(sessionTurn);
  const ahead = state.status.accounts.map((a) => Date.parse(a.fiveHourResets ?? '')).filter((t) => t > now);
  if (ahead.length) sessionTurn = setTimeout(() => { accountsFrom = null; publish(); }, Math.min(...ahead) - now + 500);
  replaceAccounts(state.status.accounts.map((a) => {
    const used = a.agent === 'cursor' ? null : a.sevenDay ?? a.fiveHour;
    const over = !!a.fiveHourResets && Date.parse(a.fiveHourResets) <= now;
    const session = a.agent === 'cursor' || !a.signedIn || a.fiveHour == null ? null : over ? 0 : Math.round(a.fiveHour * 100);
    return {
      id: a.id, agent: a.agent, name: a.name, payer: a.payer ?? 'You', signedIn: a.signedIn, label: a.label ?? null, used: used == null ? null : Math.round(used * 100),
      resets: !a.signedIn ? a.label ?? 'Not signed in' : a.agent === 'cursor' ? 'Cursor doesn’t report usage' : used == null ? 'No use reported yet'
        : a.sevenDayResets ? `Resets ${new Date(a.sevenDayResets).toLocaleDateString([], { weekday: 'long' })}` : 'No reset reported',
      // the 5-hour session (signed in only), and when it starts over (Settings › Agents' bar, the
      // usage panel's); weekly: whether used is the week's
      session,
      sessionResets: session != null && !over && a.fiveHourResets ? new Date(a.fiveHourResets).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : null,
      weekly: a.sevenDay != null,
    };
  }));
}

// what a Settings change obid answered changes in its status, shown at once
export function patchStatus(patch: Partial<Status>) {
  if (state.status) set({ status: { ...state.status, ...patch } });
}

// the usage panel opened: obid asks the tools for usage it hasn't read in 10 minutes, which comes
// in an `accounts` event a few seconds later
export function lookAtUsage() {
  if (state.connection === 'live') void api.status().then((status) => set({ status }), () => {});
}

// --- events

function apply(s: Live, e: Event): Live {
  switch (e.kind) {
    case 'job':
      return { ...s, jobs: { ...s.jobs, [e.job.id]: e.job } };
    case 'jobGone':
      return { ...s, jobs: without(s.jobs, e.id) };
    case 'inbox': {
      const rest = s.inbox.filter((i) => i.id !== e.item.id);
      return { ...s, inbox: e.item.state === 'done' ? rest : [...rest, e.item].sort((a, b) => a.created.localeCompare(b.created)) };
    }
    case 'turn': {
      const c = e.turn.conversation;
      if (!c) return s;
      let alias = s.alias;
      // the words this app sent to start one: that conversation is the one shown for them
      if (s.starting && e.turn.who === 'user' && e.turn.text.trim() === s.starting.said && !(c in s.alias) && !s.turns[c]?.length) {
        alias = { ...alias, [c]: s.starting.id };
      }
      const next = { ...s, alias, turns: { ...s.turns, [c]: merged(s.turns[c] ?? [], [e.turn]) } };
      // one sent again (its jobs changed): only it changes, unless it's the answer still being shown
      if (s.turns[c]?.some((t) => t.id === e.turn.id) && s.pace[c]?.turn !== e.turn.id) return next;
      // Oby's answer: its turn is over, its words final (paced on from those shown, if it was talking here)
      const live = c in s.working || c in s.words || c in s.pace;
      if (e.turn.who === 'oby') return { ...next, working: without(s.working, c), words: without(s.words, c), pace: live ? paced(s, c, e.turn.text, e.turn.id) : s.pace };
      // your words after Oby's: the rest of its last answer shows whole
      return s.pace[c]?.turn ? { ...next, pace: without(s.pace, c) } : next;
    }
    case 'step': {
      const c = e.step.conversation ?? Object.keys(s.turns).find((k) => s.turns[k].some((t) => t.id === e.step.turn));
      if (!c || e.step.state !== 'running') return s;
      return { ...s, working: { ...s.working, [c]: e.step.text } };
    }
    case 'delta': {
      const c = e.delta.conversation, words = (s.words[c] ?? '') + e.delta.text;
      return { ...s, working: { ...s.working, [c]: s.working[c] ?? 'Writing' }, words: { ...s.words, [c]: words }, pace: paced(s, c, words) };
    }
    case 'accounts':
      return s.status ? { ...s, status: { ...s.status, accounts: e.accounts } } : s;
    case 'conversation': {
      // not taken as the one this app's words start: another device may start one meanwhile (the
      // words' own turn, or the plan's answer, says which is this app's)
      return { ...s, conversations: [...s.conversations.filter((c) => c.id !== e.conversation.id), e.conversation] };
    }
    case 'conversationGone':
      return { ...s, conversations: s.conversations.filter((c) => c.id !== e.id), turns: without(s.turns, e.id) };
    case 'github':
      return { ...s, github: e.work };
  }
}

// --- Oby's words, a word at a time

// about a word each WORD_MS, faster when they pile up, so they never trail by more than MAX_LAG;
// by the time passed, so a throttled window keeps up, and all at once while it's hidden.
// With reduced motion they show as they come.
const WORD_MS = 35, MAX_LAG = 1200;
const wordsOf = (text: string) => text.match(/\S+\s*/g) ?? [];
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

// --- following obid

let generation = 0;
let retry = 0;
let reconnecting: ReturnType<typeof setTimeout> | undefined;
let unfollow: (() => void) | undefined;
let limitTimer: ReturnType<typeof setTimeout> | undefined;
let limitRequest = 0;

// Options can change as accounts become available. Refresh after job/account events and
// periodically; discard an older response if a newer refresh has already started.
export async function refreshLimits() {
  const request = ++limitRequest;
  try {
    const limits = await api.limitDecisions();
    if (request === limitRequest) set({ limits });
  } catch { /* older obid, or a temporary disconnect: keep the last known decisions */ }
}
function queueLimits() {
  clearTimeout(limitTimer);
  limitTimer = setTimeout(() => void refreshLimits(), 300);
}
setInterval(() => { if (state.connection === 'live') void refreshLimits(); }, 15_000);

export async function answerLimit(decision: LimitDecision, option: LimitDecision['options'][number]) {
  try {
    await api.answerLimit(decision, option);
    await refreshLimits();
  } catch (e) {
    await refreshLimits();
    throw e;
  }
}

export function connect() {
  const mine = ++generation;
  clearTimeout(reconnecting);
  set({ connection: state.connection === 'down' ? 'down' : 'connecting' });
  const pending: Event[] = [];
  let syncing = true;
  const read = sseReader((name, data) => {
    if (['limitDecision', 'job', 'accounts'].includes(name)) queueLimits();
    const e = parseEvent(name, data);
    if (!e || mine !== generation) return;
    if (syncing) pending.push(e);
    else set(apply(state, e));
  });
  let first = true;
  unfollow?.();
  unfollow = window.app!.obid.follow(
    (chunk) => {
      read(chunk);
      // obid follows the stream before it answers anything else: fetch once its first text came
      if (first) { first = false; void sync(mine, pending, () => { syncing = false; }); }
    },
    ({ status, text, why }) => {
      if (status === 401) return down(mine, 'obid refused this Mac’s token (api-token).');
      if (!status) return down(mine, text, why);
      // obid's own words ("Oby’s engine didn’t start.", a failed start's answer to the held
      // stream). `starting` is only GET /v1/status's 503 while obid opens (core's Door): taken as
      // no problem yet, should the stream ever get it
      let said: { error?: unknown; starting?: unknown } | null = null;
      try { said = JSON.parse(text); } catch { /* not obid's JSON */ }
      down(mine, typeof said?.error === 'string' ? said.error : `obid answered ${status}.`, said?.starting ? 'waiting' : undefined);
    },
  );
}

function down(mine: number, problem: string, why?: Live['why']) {
  if (mine !== generation) return;
  generation++;
  unfollow?.();
  set({ connection: 'down', problem, why, working: {}, words: {}, pace: {} });
  // every second while main starts obid before the first load (the splash, main.tsx), so the chat
  // comes soon after it; otherwise backing off, as a failure won't clear that fast
  reconnecting = setTimeout(connect, state.status || why !== 'waiting' ? Math.min(10_000, 1000 * 2 ** retry++) : 1000);
}

async function sync(mine: number, pending: Event[], onSynced: () => void) {
  try {
    const limitVersion = ++limitRequest;
    const [status, jobs, inbox, { list: conversations, complete }, github, limits] = await Promise.all([
      api.status(), api.jobs(100), api.inbox().catch(() => []), allConversations(),
      // GitHub's work: not read, the last read stands, with why (as obid keeps its last lists)
      api.githubWork().catch((e) => (state.github ? { ...state.github, error: e instanceof Error ? e.message : String(e) } : null)),
      api.limitDecisions().catch(() => state.limits),
    ]);
    if (mine !== generation) return;
    retry = 0;
    let next: Live = {
      ...state, connection: 'live', problem: null, why: undefined, status, inbox: inbox.filter((i) => i.state !== 'done'), conversations, github,
      limits: limitVersion === limitRequest ? limits : state.limits,
      jobs: { ...state.jobs, ...Object.fromEntries(jobs.map((j) => [j.id, j])) },
      firstJobs: state.status ? state.firstJobs : new Set(jobs.map((j) => j.id)),
    };
    for (const e of pending) next = apply(next, e);
    pending.length = 0;
    // the folders of conversations deleted since (folders.ts); with the ones that moved up a page
    // while it was read (their events, just applied). A list cut short at allConversations' last
    // page isn't all of them: nothing's dropped then
    if (complete) keepFoldersOf(new Set(next.conversations.map((c) => c.id)));
    set(next);
    onSynced();
    // Settings › Agents' choices kept here before obid kept them: moved to it once (AGENTS-7)
    void moveKeptChoices(status).then((moved) => { if (moved && mine === generation) lookAtUsage(); });
    // where work can start: GitHub's list takes a moment, so the rest doesn't wait for it
    void api.repos().then(({ repos }) => { if (mine === generation) set({ repos }); }, () => {});
    // the conversations opened before: what they missed while obid was away; and the latest
    // words of the newest ones, for their lines in Conversations
    const latest = [...conversations].sort((a, b) => b.lastActivity.localeCompare(a.lastActivity)).slice(0, 20).map((c) => c.id);
    for (const c of new Set([...state.loaded, ...latest])) {
      void api.conversation({ id: c, limit: state.loaded.has(c) ? 100 : 2 }).then((turns) => set({ turns: { ...state.turns, [c]: merged(state.turns[c] ?? [], turns) } }), () => {});
    }
  } catch (e) {
    down(mine, e instanceof Error ? e.message : String(e));
  }
}

// every conversation, a page at a time (obid keeps 200, more when pinned)
async function allConversations() {
  const all = new Map<string, Conversation>();
  let complete = false;
  for (let after: string | undefined, page = 0; page < 10; page++) {
    const list = await api.conversations({ after, limit: 200 });
    for (const c of list) if (!all.has(c.id)) all.set(c.id, c);
    if (list.length < 200) { complete = true; break; }
    after = list.at(-1)!.cursor;
  }
  return { list: [...all.values()], complete };
}

// what went wrong with something the app did, for a moment
let clearing: ReturnType<typeof setTimeout> | undefined;
export function say(e: unknown) {
  set({ problem: e instanceof Error ? e.message : String(e) });
  clearTimeout(clearing);
  clearing = setTimeout(() => { if (state.connection === 'live') set({ problem: null }); }, 6000);
}

// --- what the app does

// The repository a conversation's work was last in, by its id in the app: the one picked for
// words sent in it, or its newest job's, whichever came later; with none, or on Home, the newest
// job's anywhere
const newest = (jobs: Job[]) => jobs.reduce<Job | undefined>((a, j) => (!a || j.created > a.created ? j : a), undefined);
function lastRepo(s: Live, shown?: string) {
  const c = shown && (Object.keys(s.alias).find((k) => s.alias[k] === shown) ?? shown);
  const all = Object.values(s.jobs);
  const job = c ? newest(all.filter((j) => j.conversation === c)) : undefined, picked = shown ? s.picked[shown] : undefined;
  return { here: picked && (!job || picked.at > job.created) ? picked.repo : job?.repo, anywhere: newest(all)?.repo };
}
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

// A conversation opened: its turns fetched once (then the events keep them), and the jobs they
// name that aren't among the latest
export async function openConversation(shown: string) {
  const c = realId(shown);
  if (state.loaded.has(c) || c.startsWith('new:')) return;
  set({ loaded: new Set([...state.loaded, c]) });
  try {
    const turns = await api.conversation({ id: c, limit: 100 });
    set({ turns: { ...state.turns, [c]: merged(state.turns[c] ?? [], turns) } });
    for (const j of new Set(turns.flatMap((t) => t.jobs))) {
      if (!state.jobs[j]) await api.job(j).then((job) => set({ jobs: { ...state.jobs, [job.id]: job } }), () => {});
    }
  } catch (e) {
    set({ loaded: new Set([...state.loaded].filter((x) => x !== c)) });
    say(e);
  }
}

// Words to Oby (POST /v1/plans): in a conversation (its app id), or starting one (none). Your
// words, its answer and the jobs it starts come back as events. Oby's question back waits over
// the message box; your next words answer it.
let made = 0;
export function ask(said: string, shown?: string, repo?: string, folders?: string[]): string | undefined {
  if (!said.trim()) return;
  const id = shown ?? `new:${++made}`;
  const c = shown ? realId(shown) : null;
  const q = c ? state.waiting[c] : undefined;
  // an answer to Oby's question is only the answer
  const words = q ? said.trim() : inRepo(said.trim(), repo, shown);
  if (repo && !q) set({ picked: { ...state.picked, [id]: { repo, at: new Date().toISOString() } } });
  // (the rest of Oby's last answer, if it's still being shown, shows whole)
  if (c) set({ waiting: without(state.waiting, c), working: { ...state.working, [c]: 'Thinking' }, ...(state.pace[c]?.turn ? { pace: without(state.pace, c) } : {}) });
  else set({ starting: { id, said: words } });
  let failed = false;
  void (async () => {
    try {
      // the working folders go with words that start a conversation, once obid takes them (an
      // older one refuses the key); a conversation's own are obid's already (sendFoldersBy)
      const a = await api.plan(q ? q.said : words, q ? { question: q.text, option: words } : undefined, c ? { conversation: c } : { new: true }, state.status?.folders && !c ? folders : undefined);
      const where = a.conversation ?? c;
      // the plan's answer is the one: it replaces a guess from the words' turn
      const alias = !c && a.conversation ? { ...Object.fromEntries(Object.entries(state.alias).filter(([, v]) => v !== id)), [a.conversation]: id } : state.alias;
      // the one it started, listed at once (its own event may come later), so its page stays
      const now = new Date().toISOString();
      const listed = !where || state.conversations.some((x) => x.id === where) ? state.conversations
        : [...state.conversations, { id: where, title: words.split('\n')[0].slice(0, 48), pinned: false, created: now, lastActivity: now, cursor: '' }];
      set({
        alias, conversations: listed, jobs: { ...Object.fromEntries(a.jobs.map((j) => [j.id, j])), ...state.jobs },
        ...(a.kind === 'question' && where ? { waiting: { ...state.waiting, [where]: { ...a.question, said: a.said } } } : {}),
      });
      // the turns of the one it started, from the start
      if (!c && where) { set({ loaded: new Set([...state.loaded, where]) }); set({ turns: { ...state.turns, [where]: merged(state.turns[where] ?? [], await api.conversation({ id: where, limit: 100 })) } }); }
    } catch (e) {
      failed = true;
      say(e);
    } finally {
      // (words still being shown carry on to the turn, unless it failed)
      const where = c ?? Object.keys(state.alias).find((k) => state.alias[k] === id);
      set({ starting: state.starting?.id === id ? null : state.starting, ...(where ? { working: without(state.working, where), words: without(state.words, where), ...(failed ? { pace: without(state.pace, where) } : {}) } : {}) });
    }
  })();
  return id;
}

export async function renameConversation(shown: string, title: string) {
  const t = title.trim();
  if (!t) return;
  try { const c = await api.editConversation(realId(shown), { title: t }); set(apply(state, { kind: 'conversation', conversation: c })); } catch (e) { say(e); }
}
export async function pinConversation(shown: string, pinned: boolean) {
  try { const c = await api.editConversation(realId(shown), { pinned }); set(apply(state, { kind: 'conversation', conversation: c })); } catch (e) { say(e); }
}
export async function deleteConversation(shown: string) {
  const c = realId(shown);
  set(apply(state, { kind: 'conversationGone', id: c }));
  try { await api.deleteConversation(c); } catch (e) { if (!(e instanceof ApiError && e.status === 404)) say(e); }
}

// Told a job's agent something (POST /v1/jobs/{id}/messages)
export async function tellJob(id: string, text: string) {
  try { const j = await api.message(id, text.trim()); set({ jobs: { ...state.jobs, [j.id]: j } }); } catch (e) { say(e); }
}
// A task's Mode menu: how much it asks from now on (POST /v1/jobs/{id}/mode)
export async function changeMode(id: string, mode: string) {
  try { const j = await api.mode(id, mode); set({ jobs: { ...state.jobs, [j.id]: j } }); } catch (e) { say(e); }
}
export async function stopJob(id: string) {
  try { const j = await api.stop(id); set({ jobs: { ...state.jobs, [j.id]: j } }); } catch (e) { say(e); }
}

// The task's menu: Rename (PATCH /v1/jobs/{id}), shown at once as the mock does it; Archive
// (POST /v1/jobs/{id}/archive), off Home into Archive; Delete (DELETE /v1/jobs/{id}), gone for good.
// obid's job comes back and the task follows it; a refusal (a 409: still running, its pull request
// still open) shows in obid's own words and leaves the task as it was.
export async function renameJob(id: string, title: string) {
  const had = state.jobs[id];
  // the same name again (a blur with nothing changed): nothing to tell obid
  if (had?.title === title) return;
  if (had) set({ jobs: { ...state.jobs, [id]: { ...had, title } } });
  try { const j = await api.rename(id, title); set({ jobs: { ...state.jobs, [j.id]: j } }); } catch (e) {
    const now = state.jobs[id];
    if (had && now?.title === title) set({ jobs: { ...state.jobs, [id]: { ...now, title: had.title } } });
    say(e);
  }
}
export async function archiveJob(id: string) {
  try { const j = await api.archive(id); set({ jobs: { ...state.jobs, [j.id]: j } }); } catch (e) { say(e); }
}
// true once it's gone (obid deleted it, or had no such job). With commits that aren't published,
// obid asks first: its Confirm comes onto the task's card (and Home's stack), naming the commits
// lost, and only the owner's click there deletes it (answerJob, signed by main).
export async function deleteJob(id: string) {
  try {
    const { deleted, ask } = await api.deleteJob(id);
    if (ask) set(apply(state, { kind: 'inbox', item: ask }));
    if (!deleted) return false;
  } catch (e) {
    if (!(e instanceof ApiError && e.status === 404)) { say(e); return false; }
  }
  set(apply(state, { kind: 'jobGone', id }));
  return true;
}

// A job's session, for Full session: fetched when it opens and again at each of the job's events
// (at most once a second while its agent works), only what came or changed since (?after=), by n,
// page after page (obid sends at most 1,000 at a time) until obid's `next` is null
const loading = new Map<string, boolean>();  // in flight; true: asked again meanwhile
export async function loadSession(id: string): Promise<void> {
  if (loading.has(id)) { loading.set(id, true); return; }
  loading.set(id, false);
  try {
    const had = state.sessions[id] ?? [];
    let after: number | null | undefined = had.length ? Math.max(...had.map((e) => e.n)) : undefined;
    do {
      const page = await api.transcript(id, after ?? undefined);
      const now = state.sessions[id] ?? [];
      set({ sessions: { ...state.sessions, [id]: [...now.filter((e) => !page.entries.some((m) => m.n === e.n)), ...page.entries] } });
      after = page.next;
    } while (after != null);
  } catch (e) { say(e); }
  const again = loading.get(id);
  loading.delete(id);
  if (again) return loadSession(id);
}

// Work on a GitHub item, on the owner's tap (POST /v1/github/work/{id}/start): its job comes back,
// and the feed's next event takes the item off To do. A review in a repository Oby doesn't work in
// starts nothing yet: its Confirm comes into Home's stack, and obid's words say so for a moment.
export async function startGitHubWork(id: string) {
  try { const j = await api.startGitHubWork(id); set({ jobs: { ...state.jobs, [j.id]: j } }); } catch (e) { say(e); }
}

// A job's decision answered: its permission through obid's own route (POST /v1/jobs/{id}/permission),
// anything else in the inbox; Later puts it off an hour. A Confirm goes with this Mac's device
// signature, which main adds (#776). id: a job's, or an item's with no task (asksOf), answered from
// Home's stack.
// On Oby's ask for a check, Use it is the owner's Confirm, No check (or Keep it) their Cancel; Edit
// sends the command they typed (words) to POST /v1/repos/check, which asks for it in the ask's
// place, confirmed here at once with this Mac's signature over that ask's title.
const DECISIONS: Record<string, 'allow' | 'always' | 'deny'> = { Allow: 'allow', 'Always allow': 'always', Deny: 'deny' };
export async function answerJob(id: string, option: string, words?: string) {
  if (id.startsWith('limit:')) return answerLimitAsk(id, option).catch(say);
  const j = state.jobs[id], item = itemOf(state, id) ?? state.inbox.find((i) => i.id === id), ask = j && askOf(state, j);
  const shown = option === 'Edit' && words ? `Use ${words}` : option === TELL && words ? words : option;
  if (ask) set({ answered: { ...state.answered, [id]: { ask, option: shown, item: item?.id } } });
  else if (item) answering(item.id, true);
  try {
    if (checkOf(item) && option !== 'Later') {
      let asked = item!;
      if (option === 'Edit' && words) {
        // (the card stays answered: obid closes this ask, and the new one has no job)
        const changed = await api.changeCheck(asked.confirm!.target, words);
        if (!changed.ask) return;
        asked = changed.ask;
      }
      const next = option === 'No check' || option === 'Keep it' ? await api.answer(asked.id, { answer: 'Cancel' }) : await api.confirm(asked.id);
      set(apply(state, { kind: 'inbox', item: next }));
      void api.repos().then(({ repos }) => set({ repos }), () => {});
    } else if (item?.kind === 'confirm' && option === 'Confirm') set(apply(state, { kind: 'inbox', item: await api.confirm(item.id) }));
    else if (item?.kind === 'permission' && j?.permission && DECISIONS[option]) {
      const next = await api.permission(id, { id: j.permission.id, decision: DECISIONS[option] });
      set({ jobs: { ...state.jobs, [next.id]: next } });
    } else if (item) {
      // (an agent's question by its own item, never the job's permission, which can lag behind the
      // inbox: Tell it something sends the owner's words as its answer)
      const said = option === TELL && words ? words : option;
      const body = option === 'Later' ? { later: new Date(Date.now() + 3600_000).toISOString().replace(/\.\d+Z$/, 'Z') } : { answer: said };
      set(apply(state, { kind: 'inbox', item: await api.answer(item.id, body) }));
    } else if (option === 'Dismiss') {
      // a failed job with no item: let go
      const next = await api.dismiss(id);
      set({ jobs: { ...state.jobs, [next.id]: next } });
    }
  } catch (e) {
    // not taken: it's still yours to answer
    set({ answered: without(state.answered, id) });
    say(e);
  } finally {
    if (!j && item) answering(item.id, false);
  }
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
      set(apply(state, { kind: 'inbox', item: changed.ask }));
      set(apply(state, { kind: 'inbox', item: await api.confirm(changed.ask.id) }));
      set({ repos: (await api.repos()).repos });
    }
  } catch (e) {
    say(e);
  }
}

// an item with no task answered from Home's stack, its answer on its way (or not any more)
const answering = (item: string, on: boolean) =>
  set({ answering: new Set(on ? [...state.answering, item] : [...state.answering].filter((x) => x !== item)) });

// An offer answered (OfferCard.tsx), for each of its jobs still waiting: Dispatch is the owner's own
// Start, which only this tap gives (POST /v1/inbox/{id}/answer); Skip cancels it; Later puts it
// off an hour. What starts comes as its job's event, its card under the answer.
export async function answerOffer(offer: string, option: 'Dispatch' | 'Skip' | 'Later') {
  const items = state.inbox.filter((i) => i.offer?.id === offer);
  items.forEach((i) => answering(i.id, true));
  for (const item of items) {
    const body = option === 'Later' ? { later: new Date(Date.now() + 3600_000).toISOString().replace(/\.\d+Z$/, 'Z') } : { answer: option === 'Dispatch' ? 'Start' : 'Cancel' };
    try { set(apply(state, { kind: 'inbox', item: await api.answer(item.id, body) })); } catch (e) { say(e); } finally { answering(item.id, false); }
  }
}

export function markSeen(id: string) {
  if (!state.seen.has(id)) set({ seen: new Set([...state.seen, id]) });
}
