import type { BrowserWindow } from 'electron';
import { chooseProfile, obid, openedProfile, reason, send, streamEnded, takesKeys, type Trusted } from './obid';
import { settings, writeSettings } from './settings';
import { ipc } from './ipc';
import { loadWhat, profileId, type LoadWhat } from '../shared/decode';
import { LATEST_JOBS, answer, answerTurns, apply, synced, type ObidEvent, type ObidState } from '../shared/live-state';
import { wired, type Connection, type LiveMessage } from '../shared/live-wire';
import { parseEvent, sseReader, type Conversation, type Event, type GitHubWork, type InboxItem, type Job, type LimitDecision, type Status, type Turn } from '../shared/obid-wire';

// What obid has, kept here in main by its one follower of obid's events (docs/contracts/live-state.md,
// LIVE-10 and 11; docs/contracts/alerts.md, ALERTS-9 to 11): GET /v1/events?alerts=1 from launch
// until Oby quits, window open or not, so obid counts the Mac app as following (ALERTS-3). Once
// the stream answers it reads obid's lists, then applies each event through the one rule
// (shared/live-state.ts). The alerts (notify.ts) read it here; the window's page asks for it when
// it opens (`obid-snapshot`), then gets each message main sends it (`obid-live`), and asks main to
// read what it opens (`obid-load`). A job's session, a turn's steps and words, the room's posts and
// a thread's change are passed on as they come, never kept.

const EMPTY: ObidState = { jobs: {}, gone: new Set(), status: null, github: null, inbox: [], conversations: [], turns: {}, goneConversations: new Set(), settledInbox: {} };
let state = EMPTY;
let limits: LimitDecision[] = [];
let connection: Connection = { connection: 'connecting', problem: null };
// whether the last sync's conversation list was all of them
let complete = false;
// the conversations the page opened: their turns read again after each sync
const loaded = new Set<string>();

export const obidState = () => state;
export const limitDecisions = () => limits;

// --- to the page and the alerts

type Unnumbered = LiveMessage extends infer M ? (M extends LiveMessage ? Omit<M, 'seq'> : never) : never;
let seq = 0;
// the window trusted names (index.ts)
let page: () => BrowserWindow | null = () => null;
const listeners = new Set<(m: LiveMessage) => void>();
// every message main sends the page, in the same order (notify.ts)
export const onLive = (f: (m: LiveMessage) => void) => { listeners.add(f); };
const now = () => ({ ...connection, state: wired(state), limits, complete });
function post(m: Unnumbered) {
  const message = { ...m, seq: ++seq } as LiveMessage;
  const w = page();
  if (w && !w.isDestroyed()) w.webContents.send('obid-live', message);
  listeners.forEach((l) => l(message));
}

// --- reading obid

const words = (e: unknown) => (e instanceof Error ? e.message : String(e));
// One read: obid's answer, else an error with obid's words, or why it didn't answer
async function read<T>(path: string): Promise<T> {
  const a = await obid('GET', path);
  if (a.status === 0) throw new Error(a.text);
  let value: unknown = null;
  try { value = a.text ? JSON.parse(a.text) : null; } catch { /* not JSON: said below */ }
  if (a.status < 200 || a.status >= 300) {
    const said = (value as { error?: unknown } | null)?.error;
    throw new Error(typeof said === 'string' ? said : `Oby answered ${a.status}.`);
  }
  return value as T;
}
const id = encodeURIComponent;

// every conversation, a page at a time (obid keeps 200, more when pinned)
async function allConversations() {
  const all = new Map<string, Conversation>();
  let complete = false;
  for (let after: string | undefined, page = 0; page < 10; page++) {
    const list = await read<Conversation[]>(`/v1/conversations?${after === undefined ? '' : `after=${id(after)}&`}limit=200`);
    for (const c of list) if (!all.has(c.id)) all.set(c.id, c);
    if (list.length < 200) { complete = true; break; }
    after = list.at(-1)!.cursor;
  }
  return { list: [...all.values()], complete };
}

// A job, read and applied as an answer, then passed on as its event
async function loadJob(job: string): Promise<true | string> {
  try {
    const j = await read<Job>(`/v1/jobs/${id(job)}`);
    state = answer(state, j);
    post({ kind: 'event', event: { kind: 'job', job: j } });
    return true;
  } catch (e) { return words(e); }
}
// a job that waits on you, however old: past the latest 100, its ask would show nowhere (obid
// keeps every open item's job; a start item's isn't one until it starts)
const lookUpJobOf = (i: InboxItem) => {
  if (i.job && i.kind !== 'start' && i.state !== 'done' && !state.jobs[i.job] && !state.gone.has(i.job)) void loadJob(i.job);
};
// A conversation's turns, read and applied as an answer, then passed on
async function loadTurns(c: string, limit: number) {
  const turns = await read<Turn[]>(`/v1/conversation?id=${id(c)}&limit=${limit}`);
  state = answerTurns(state, c, turns);
  post({ kind: 'turns', conversation: c, turns });
  return turns;
}

// --- the limit decisions: one decision for the jobs stopped by an account's limit

// Options can change as accounts become available: read again after job and account events, when
// the page asks, and every 15 seconds; an older answer than a newer request changes nothing
let limitRequest = 0;
let limitTimer: ReturnType<typeof setTimeout> | undefined;
async function refreshLimits(): Promise<true | string> {
  const request = ++limitRequest;
  try {
    const list = await read<LimitDecision[]>('/v1/limit-decisions');
    if (request === limitRequest && JSON.stringify(list) !== JSON.stringify(limits)) {
      limits = list;
      post({ kind: 'limits', limits });
    }
    return true;
  } catch (e) { return words(e); }
}
function queueLimits() {
  clearTimeout(limitTimer);
  limitTimer = setTimeout(() => void refreshLimits(), 300);
}

// --- following obid

const RULE = new Set<Event['kind']>(['job', 'jobGone', 'conversation', 'conversationGone', 'turn', 'turnAsks', 'turnDelivery', 'inbox', 'accounts', 'github']);
const ruled = (e: Event): e is ObidEvent => RULE.has(e.kind);
// An event: through the rule when it's the rule's, then passed on; an item that waits on you
// brings its job
function take(e: Event, look = true) {
  if (ruled(e)) state = apply(state, e);
  post({ kind: 'event', event: e });
  if (look && e.kind === 'inbox') lookUpJobOf(e.item);
}

let generation = 0;
let retry = 0;
let stream: AbortController | null = null;

// The stream, again whenever it ends (obid restarted, or not up yet). obid takes it on before it
// answers: what it has now is read then, with the events that come meanwhile applied over it.
async function follow() {
  const mine = ++generation;
  const control = new AbortController();
  stream = control;
  const r = await send('GET', '/v1/events?alerts=1', undefined, control.signal);
  if (!(r instanceof Response)) return r.status ? refused(mine, r.status, r.text) : down(mine, r.text, r.why);
  if (!r.ok || !r.body) return refused(mine, r.status, await r.text().catch(() => ''));
  // whether obid takes keys, read again whenever its stream starts (notify.ts, device.ts)
  void takesKeys();
  const pending: Event[] = [];
  let syncing = true;
  const heard = sseReader((name, data) => {
    if (mine !== generation) return;
    if (name === 'limitDecision' || name === 'job' || name === 'accounts') queueLimits();
    const e = parseEvent(name, data);
    if (!e) return;
    if (syncing) pending.push(e);
    else take(e);
  });
  void sync(mine, pending, () => { syncing = false; });
  try {
    const reader = r.body.pipeThrough(new TextDecoderStream()).getReader();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      heard(value);
    }
    streamEnded();
    down(mine, 'obid closed the event stream.');
  } catch (err) {
    if (!control.signal.aborted) { const a = reason(err); down(mine, a.text, a.why); }
  }
}

// obid's own words ("Oby’s engine didn’t start.", a failed start's answer to the held stream).
// `starting` is only GET /v1/status's 503 while obid opens (core's Door): taken as no problem yet,
// should the stream ever get it
function refused(mine: number, status: number, text: string) {
  // the profile kept is none of obid's (gone, or another obid): obid's default profile, followed again
  if (status === 400 && openedProfile()) { openProfile(null); return; }
  if (status === 401) return down(mine, 'obid refused this Mac’s token (api-token).');
  let said: { error?: unknown; starting?: unknown } | null = null;
  try { said = JSON.parse(text); } catch { /* not obid's JSON */ }
  down(mine, typeof said?.error === 'string' ? said.error : `obid answered ${status}.`, said?.starting ? 'waiting' : undefined);
}

function down(mine: number, problem: string, why?: Connection['why']) {
  if (mine !== generation) return;
  generation++;
  stream?.abort();
  stream = null;
  connection = { connection: 'down', problem, ...(why ? { why } : {}) };
  post({ kind: 'connection', ...connection });
  // every second while obid starts before its first read (the splash), so the chat comes soon
  // after it; otherwise backing off, as a failure won't clear that fast
  setTimeout(follow, state.status || why !== 'waiting' ? Math.min(10_000, 1000 * 2 ** retry++) : 1000);
}

// What obid has now: its status, latest jobs, inbox, every conversation, GitHub's work and the
// limit decisions, over what's held (synced), then the events that came meanwhile
async function sync(mine: number, pending: Event[], onSynced: () => void) {
  try {
    const limitVersion = ++limitRequest;
    const before = state;
    const [status, jobs, inbox, conversations, github, decisions] = await Promise.all([
      read<Status>('/v1/status'), read<Job[]>(`/v1/jobs?limit=${LATEST_JOBS}`), read<InboxItem[]>('/v1/inbox').catch(() => []), allConversations(),
      // GitHub's work: not read, the last read stands, with why (as obid keeps its last lists)
      read<GitHubWork>('/v1/github/work').catch((e) => (state.github ? { ...state.github, error: words(e) } : null)),
      read<LimitDecision[]>('/v1/limit-decisions').catch(() => limits),
    ]);
    if (mine !== generation) return;
    retry = 0;
    // obid's lists win, but over a copy an answer brought while they were read: the newer of the
    // two (a restart can take a revision back: obid's then, not one from before the stream went)
    state = synced(state, before, { jobs, conversations: conversations.list, complete: conversations.complete, inbox, status, github });
    if (limitVersion === limitRequest) limits = decisions;
    complete = conversations.complete;
    connection = { connection: 'live', problem: null };
    post({ kind: 'snapshot', ...now() });
    pending.splice(0).forEach((e) => take(e, false));
    onSynced();
    state.inbox.forEach(lookUpJobOf);
    // the conversations the page opened: what they missed while obid was away; and the latest
    // words of the newest ones, for their lines in Conversations
    const latest = [...conversations.list].sort((a, b) => b.lastActivity.localeCompare(a.lastActivity)).slice(0, 20).map((c) => c.id);
    for (const c of new Set([...loaded, ...latest])) void loadTurns(c, loaded.has(c) ? 100 : 2).catch(() => {});
  } catch (e) {
    down(mine, words(e));
  }
}

// What the page opens: a conversation's turns, and the jobs they name it hasn't got; a job; or the
// limit decisions again. true once applied, else why not
async function load(what: LoadWhat): Promise<true | string> {
  if ('limits' in what) return refreshLimits();
  if ('job' in what) return state.jobs[what.job] || state.gone.has(what.job) ? true : loadJob(what.job);
  const c = what.conversation;
  let turns: Turn[];
  try { turns = await loadTurns(c, 100); } catch (e) { return words(e); }
  loaded.add(c);
  for (const j of new Set(turns.flatMap((t) => t.jobs))) {
    if (!state.jobs[j] && !state.gone.has(j)) await loadJob(j);
  }
  return true;
}

// Another profile opened (B00-40): what main holds was the last one's, so it's dropped and obid's
// stream followed again, whose sync brings the page this profile's snapshot; it's kept in
// settings.json, so main names it from the next launch on. true when it changed, false when it was
// open already
function openProfile(id: string | null) {
  if (!chooseProfile(id)) return false;
  try { writeSettings({ profile: id }); } catch (e) { console.error(`Oby: couldn’t keep the open profile: ${e}`); }
  state = EMPTY;
  limits = [];
  loaded.clear();
  complete = false;
  generation++;
  stream?.abort();
  stream = null;
  void follow();
  return true;
}
// the profile kept from the last launch, named from the first call on
const keptProfile = () => profileId(settings().profile)?.value ?? null;

export function living(trusted: Trusted, window: () => BrowserWindow | null) {
  page = window;
  const channels = ipc(trusted);
  // what main holds now, and the number of the last message it sent the page
  channels.handle('obid-snapshot', [], () => ({ ...now(), seq }), null);
  channels.handle('obid-load', [loadWhat], (_e, what) => load(what), null);
  channels.handle('obid-profile', [profileId], (_e, id) => openProfile(id), null);
  chooseProfile(keptProfile());
  setInterval(() => { if (connection.connection === 'live') void refreshLimits(); }, 15_000);
  void follow();
}
