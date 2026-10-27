import { useSyncExternalStore } from 'react';
import { ApiError, api, parseEvent, sseReader, type Conversation, type Event, type InboxItem, type Job, type Question, type Repo, type Status, type Turn } from './obid';
import { AGENTS, type Ask, type AskKind, type Job as JobView, type Message, type StageId } from './mock';
import { replaceChats, type Chat } from './chats';
import { replaceTasks, type Task } from './tasks';
import { replaceAccounts } from './SettingsView';

// What obid has (docs/runner-api.md), kept live by its event stream, as apps/web/src/live.ts does
// it: the stream first, then the status, jobs, inbox and conversations, with the events that came
// meanwhile applied over them. Sangam's screens don't change: what obid has goes into the same
// stores the mock filled (chats.ts, tasks.ts), as conversations and tasks. Without ?everything
// (lean.ts) the app runs on this; with it, on the mock.
type Live = {
  connection: 'connecting' | 'live' | 'down';
  problem: string | null;        // why obid isn't answering (its own reason, main/obid.ts), or what last went wrong
  status: Status | null;
  repos: Repo[];                 // where a job may start (GET /v1/repos)
  jobs: Record<string, Job>;
  firstJobs: Set<string>;        // the jobs there when the app opened: never new
  seen: Set<string>;             // jobs opened since
  inbox: InboxItem[];            // open and put off
  conversations: Conversation[];
  turns: Record<string, Turn[]>; // by conversation: each opened one's, and any that came since
  loaded: Set<string>;           // the conversations whose turns were fetched
  working: Record<string, string>;            // a turn under way, by conversation: what it's on
  words: Record<string, string>;              // its words so far (delta events), by conversation
  waiting: Record<string, Question & { said: string }>;  // Oby's question back, by conversation
  // words to Oby starting a new conversation, shown at once under an id of the app's own
  // ('new:1'); obid's id for it, once known, maps onto that one (alias), so its page stays open
  starting: { id: string; said: string } | null;
  alias: Record<string, string>;
  // decisions answered here, by job: what was asked, the pick, and the inbox item it settles.
  // The cards keep showing it answered (as the mock's do), and the stack lets it go.
  answered: Record<string, { ask: Ask; option: string; item?: string }>;
  // the repository last picked for words sent here, by conversation (its app id), and when
  picked: Record<string, { repo: string; at: string }>;
};

let state: Live = {
  connection: 'connecting', problem: null, status: null, repos: [], jobs: {}, firstJobs: new Set(), seen: new Set(), inbox: [],
  conversations: [], turns: {}, loaded: new Set(), working: {}, words: {}, waiting: {}, starting: null, alias: {}, answered: {}, picked: {},
};
const listeners = new Set<() => void>();
const set = (patch: Partial<Live>) => { state = { ...state, ...patch }; publish(); listeners.forEach((l) => l()); };
export const useLive = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => state);

const minutesAgo = (iso: string | null | undefined) => (iso ? Math.max(0, (Date.now() - Date.parse(iso)) / 60_000) : 0);
const byAt = (a: Turn, b: Turn) => a.at.localeCompare(b.at);
const merged = (had: Turn[], more: Turn[]) => [...had.filter((t) => !more.some((m) => m.id === t.id)), ...more].sort(byAt);
const without = <T,>(r: Record<string, T>, key: string) => { const { [key]: _, ...rest } = r; return rest; };
// the app's id for one of obid's conversations, and back
const shownId = (c: string) => state.alias[c] ?? c;
const realId = (shown: string) => Object.keys(state.alias).find((k) => state.alias[k] === shown) ?? shown;

// --- obid's state as the screens' conversations and tasks

function chatOf(s: Live, c: Conversation): Chat {
  // a task's card under the answer that started it, not again under each that mentions it
  const carded = new Set<string>();
  const said: Message[] = (s.turns[c.id] ?? []).map((t) => {
    const jobs = t.jobs.filter((j) => !carded.has(j) && carded.add(j));
    return { who: t.who === 'user' ? 'you' : 'oby', text: t.text, ago: minutesAgo(t.at), ...(jobs.length ? { jobs } : {}) };
  });
  const words = s.words[c.id], q = s.waiting[c.id];
  if (words) said.push({ who: 'oby', text: words, ago: 0, streaming: true });
  return {
    id: s.alias[c.id] ?? c.id, title: c.title, pinned: c.pinned, at: minutesAgo(c.lastActivity), said,
    about: c.tasks?.length ? `Started ${c.tasks.length} task${c.tasks.length === 1 ? '' : 's'}` : '',
    typing: c.id in s.working, step: words ? undefined : s.working[c.id],
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
// permission or a question), else the oldest (an offer like a check to keep). The card holds one.
const itemOf = (s: Live, job: string) => {
  const items = s.inbox.filter((i) => i.job === job);
  return items.find((i) => i.kind === 'permission' || i.kind === 'question') ?? items[0];
};

// What a job needs from you: that decision (a permission's command as its detail); or, failed or
// interrupted and not let go, letting it go
function askOf(s: Live, j: Job): Ask | undefined {
  const item = itemOf(s, j.id);
  if (item) {
    return {
      kind: item.kind as AskKind, headline: item.headline ?? item.title, text: item.headline ? item.text : '', options: item.options,
      recommended: item.recommended ?? undefined, detail: item.kind === 'permission' ? j.permission?.detail : undefined,
    };
  }
  if ((j.status === 'failed' || j.status === 'interrupted') && !j.dismissed) {
    return { kind: 'failed', headline: j.status === 'failed' ? 'It failed' : 'It was interrupted', text: j.summary.split('\n')[0] || j.step, options: ['Dismiss'] };
  }
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
    check: j.check ?? undefined, status: j.status, step: j.step, waiting: j.waiting ?? undefined,
    round: j.rounds > 1 ? [j.round, j.rounds] : undefined,
    stages: (j.pipeline?.stages ?? []).map((st) => ({ id: st.id as StageId, title: st.title, state: st.state, detail: st.detail ?? undefined })),
    ask, summary: j.summary || undefined, commits: j.commits ?? undefined, published: j.published ?? undefined,
    created: minutesAgo(j.created), finished: j.finished ? minutesAgo(j.finished) : undefined, conversation: j.conversation ? shownId(j.conversation) : undefined,
  };
  return {
    id: j.id, type: job.type, title: j.title, brand: AGENTS[j.agent]?.brand ?? AGENTS.claude.brand, job,
    status: (now && !answer) || answer === 'Later' ? 'needs' : j.status === 'queued' || j.status === 'running' ? 'running' : 'done',
    fresh: !s.firstJobs.has(j.id) && !s.seen.has(j.id), answer,
  };
}
const tasksOf = (s: Live) => Object.values(s.jobs).sort((a, b) => b.created.localeCompare(a.created)).map((j) => taskOf(s, j));

// into the stores the screens read
function publish() {
  replaceChats(chatsOf(state));
  replaceTasks(tasksOf(state));
  if (state.status) replaceAccounts(state.status.accounts.map((a) => ({
    id: a.id, agent: a.agent, name: a.name, payer: a.payer ?? 'You', used: Math.round((a.sevenDay ?? a.fiveHour ?? 0) * 100),
    resets: a.sevenDayResets ? `Resets ${new Date(a.sevenDayResets).toLocaleDateString([], { weekday: 'long' })}` : a.signedIn ? 'No use reported yet' : 'Not signed in',
  })));
}

// --- events

function apply(s: Live, e: Event): Live {
  switch (e.kind) {
    case 'job':
      return { ...s, jobs: { ...s.jobs, [e.job.id]: e.job } };
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
      // Oby's answer: its turn is over, its words final
      return e.turn.who === 'oby' ? { ...next, working: without(s.working, c), words: without(s.words, c) } : next;
    }
    case 'step': {
      const c = e.step.conversation ?? Object.keys(s.turns).find((k) => s.turns[k].some((t) => t.id === e.step.turn));
      if (!c || e.step.state !== 'running') return s;
      return { ...s, working: { ...s.working, [c]: e.step.text } };
    }
    case 'delta':
      return { ...s, working: { ...s.working, [e.delta.conversation]: s.working[e.delta.conversation] ?? 'Writing' }, words: { ...s.words, [e.delta.conversation]: (s.words[e.delta.conversation] ?? '') + e.delta.text } };
    case 'accounts':
      return s.status ? { ...s, status: { ...s.status, accounts: e.accounts } } : s;
    case 'conversation': {
      const known = s.conversations.some((c) => c.id === e.conversation.id);
      // one starting while this app's words start one: theirs
      const alias = !known && s.starting && !Object.values(s.alias).includes(s.starting.id) ? { ...s.alias, [e.conversation.id]: s.starting.id } : s.alias;
      return { ...s, alias, conversations: [...s.conversations.filter((c) => c.id !== e.conversation.id), e.conversation] };
    }
    case 'conversationGone':
      return { ...s, conversations: s.conversations.filter((c) => c.id !== e.id), turns: without(s.turns, e.id) };
  }
}

// --- following obid

let generation = 0;
let retry = 0;
let reconnecting: ReturnType<typeof setTimeout> | undefined;

export function connect() {
  const mine = ++generation;
  clearTimeout(reconnecting);
  set({ connection: state.connection === 'down' ? 'down' : 'connecting' });
  const pending: Event[] = [];
  let syncing = true;
  const read = sseReader((name, data) => {
    const e = parseEvent(name, data);
    if (!e || mine !== generation) return;
    if (syncing) pending.push(e);
    else set(apply(state, e));
  });
  let first = true;
  window.app!.obid.follow(
    (chunk) => {
      read(chunk);
      // obid follows the stream before it answers anything else: fetch once its first text came
      if (first) { first = false; void sync(mine, pending, () => { syncing = false; }); }
    },
    ({ status, text }) => down(mine, status === 401 ? 'obid refused this Mac’s token (api-token).' : status ? `obid answered ${status}.` : text),
  );
}

function down(mine: number, problem: string) {
  if (mine !== generation) return;
  generation++;
  set({ connection: 'down', problem, working: {}, words: {} });
  reconnecting = setTimeout(connect, Math.min(10_000, 1000 * 2 ** retry++));
}

async function sync(mine: number, pending: Event[], onSynced: () => void) {
  try {
    const [status, jobs, inbox, conversations] = await Promise.all([api.status(), api.jobs(100), api.inbox().catch(() => []), allConversations()]);
    if (mine !== generation) return;
    retry = 0;
    let next: Live = {
      ...state, connection: 'live', problem: null, status, inbox: inbox.filter((i) => i.state !== 'done'), conversations,
      jobs: { ...state.jobs, ...Object.fromEntries(jobs.map((j) => [j.id, j])) },
      firstJobs: state.status ? state.firstJobs : new Set(jobs.map((j) => j.id)),
    };
    for (const e of pending) next = apply(next, e);
    pending.length = 0;
    set(next);
    onSynced();
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
  for (let after: string | undefined, page = 0; page < 10; page++) {
    const list = await api.conversations({ after, limit: 200 });
    for (const c of list) if (!all.has(c.id)) all.set(c.id, c);
    if (list.length < 200) break;
    after = list.at(-1)!.cursor;
  }
  return [...all.values()];
}

// what went wrong with something the app did, for a moment
let clearing: ReturnType<typeof setTimeout> | undefined;
function say(e: unknown) {
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
export function ask(said: string, shown?: string, repo?: string): string | undefined {
  if (!said.trim()) return;
  const id = shown ?? `new:${++made}`;
  const c = shown ? realId(shown) : null;
  const q = c ? state.waiting[c] : undefined;
  // an answer to Oby's question is only the answer
  const words = q ? said.trim() : inRepo(said.trim(), repo, shown);
  if (repo && !q) set({ picked: { ...state.picked, [id]: { repo, at: new Date().toISOString() } } });
  if (c) set({ waiting: without(state.waiting, c), working: { ...state.working, [c]: 'Thinking' } });
  else set({ starting: { id, said: words } });
  void (async () => {
    try {
      const a = await api.plan(q ? q.said : words, q ? { question: q.text, option: words } : undefined, c ? { conversation: c } : { new: true });
      const where = a.conversation ?? c;
      const alias = !c && a.conversation && !(a.conversation in state.alias) ? { ...state.alias, [a.conversation]: id } : state.alias;
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
      say(e);
    } finally {
      const where = c ?? Object.keys(state.alias).find((k) => state.alias[k] === id);
      set({ starting: state.starting?.id === id ? null : state.starting, ...(where ? { working: without(state.working, where), words: without(state.words, where) } : {}) });
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
export async function stopJob(id: string) {
  try { const j = await api.stop(id); set({ jobs: { ...state.jobs, [j.id]: j } }); } catch (e) { say(e); }
}

// A job's decision answered: its permission through obid's own route (POST /v1/jobs/{id}/permission),
// anything else in the inbox; Later puts it off an hour. A Confirm goes with this Mac's device
// signature, which main adds (#776).
const DECISIONS: Record<string, 'allow' | 'always' | 'deny'> = { Allow: 'allow', 'Always allow': 'always', Deny: 'deny' };
export async function answerJob(id: string, option: string) {
  const j = state.jobs[id], item = itemOf(state, id), ask = j && askOf(state, j);
  if (ask) set({ answered: { ...state.answered, [id]: { ask, option, item: item?.id } } });
  try {
    if (item?.kind === 'confirm' && option === 'Confirm') set(apply(state, { kind: 'inbox', item: await api.confirm(item.id) }));
    else if (item?.kind === 'permission' && j?.permission && DECISIONS[option]) {
      const next = await api.permission(id, { id: j.permission.id, decision: DECISIONS[option] });
      set({ jobs: { ...state.jobs, [next.id]: next } });
    } else if (item) {
      const body = option === 'Later' ? { later: new Date(Date.now() + 3600_000).toISOString().replace(/\.\d+Z$/, 'Z') } : { answer: option };
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
  }
}

export function markSeen(id: string) {
  if (!state.seen.has(id)) set({ seen: new Set([...state.seen, id]) });
}
