import { app, ipcMain, Notification, powerMonitor, shell, type BrowserWindow } from 'electron';
import { constants, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { obid, send, type Answer, type Trusted } from './obid';
import { answerLimit, writeOwn } from './device';
import { parseEvent, sseReader, type Conversation, type Event, type InboxItem, type Job, type LimitDecision, type Turn } from '../shared/obid-wire';

// Oby's alerts on this Mac, as the Swift app posted them (apps/mac/app/Alerts.swift and
// RunnerFeed.swift), by the owner's rule of 2026-10-28 (`notifies`, the one gate): nothing while
// the window is in front, on any page; away from it, only what waits on the owner to decide or
// act (an agent's ask, a Confirm, a Start, a question, a job that failed or couldn't publish);
// never progress, a result with nothing to decide, or Oby's own words. What alerts is what obid
// pushes to the phones (core/crates/obid/src/push.rs, `wanted` and its `notifies`), so the Mac and
// the iPhone agree; the words are the Swift app's. One alert per thing, replaced when what it
// needs changes, gone once it's settled anywhere; nothing that was there when Oby opened.
// Main follows obid's event stream for them by itself, from launch until Oby quits, with the
// window closed too (the page's own stream ends with the window). A ping every 15 seconds is all
// it hears while nothing happens.

// Where an alert's click opens the window: a job's page, else a conversation, else Home
type Place = { job?: string; conversation?: string };
// A button: obid's answer, as the same tap in the window gives it
type Button = { text: string; does: () => Promise<Answer> };
type Alert = {
  // what it says (sent again only when this changes), and whether it needs the owner to decide or
  // act (the gate's one question, `notifies`): it goes once that's settled, and alerts once the
  // window leaves the front if it came while the window was there; FYI (`needs` false) is kept
  // for the window and never alerts
  what: string;
  needs: boolean;
  title: string;
  subtitle?: string;
  body: string;
  place: Place;
  // its stack in Notification Center: one per job, else per conversation
  thread: string;
  buttons: Button[];
};

// "Job", with the keys only alerts read
type ObidJob = Job & {
  obi?: unknown;
  reason?: string | null;
  publishError?: string | null;
  pushed?: number | null;
  pullRequest?: { state: string } | null;
  reviewNeedsYou?: boolean;
  reviewError?: string | null;
  mergeNeedsYou?: boolean;
  mergeError?: string | null;
  outgoing?: { action: string; branch?: string | null; id: string }[];
  permission: (NonNullable<Job['permission']> & { question?: unknown }) | null;
};

// --- words: obid's text is data, shown as plain text

// Each line without control or format characters (a bidi override, a zero-width space) that could
// hide or reorder what's shown, and no blank ones (Alerts.swift, compact)
const plain = (text: string) => text.split('\n')
  .map((l) => l.replace(/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]/gu, ' ').replace(/\s+/g, ' ').trim()).filter(Boolean).join('\n');
const cut = (text: string, most: number) => { const c = [...text]; return c.length <= most ? text : `${c.slice(0, most - 1).join('')}…`; };
const firstLine = (text?: string | null) => (text ? plain(text).split('\n')[0] : '') || undefined;
// Whether a collapsed banner shows all of it, where its buttons are pressed: nothing is allowed or
// started that the owner couldn't read. ponytail: 3 lines of about 40 characters, from the Swift
// app's measure of the banner (Alerts.fits); measure it again if macOS draws it differently.
const fits = (text: string) => [...text].length <= 120 && text.split('\n').length <= 3;

// --- what alerts (push.rs, wanted)

const jobs = new Map<string, ObidJob>();
const inbox = new Map<string, InboxItem>();
const limits = new Map<string, LimitDecision>();
let limitsRequest = 0;
async function refreshLimits() {
  const request = ++limitsRequest;
  const active = json<LimitDecision[]>(await obid('GET', '/v1/limit-decisions'));
  if (request !== limitsRequest || !active) return;
  limits.clear();
  for (const d of active) limits.set(d.id, d);
  later();
}
const titles = new Map<string, string>();
// Oby's last answer heard in each conversation, and the ones to alert at the next look
const lastAnswer = new Map<string, Turn>();
const answers = new Map<string, string>();

const unpublished = (j: ObidJob) => {
  const commits = j.commits ?? 0;
  const on = j.published ? j.pushed ?? commits : j.publishError ? 0 : j.pushed ?? 0;
  return Math.max(0, commits - on);
};
// Done with commits its open pull request hasn't got: obid couldn't publish it (job.rs, can_publish)
const canPublish = (j: ObidJob) => j.status === 'done' && j.kind !== 'research' && j.kind !== 'review' && j.base != null
  && unpublished(j) > 0 && !j.dismissed && (!j.pullRequest || j.pullRequest.state === 'OPEN');
const pushedHead = (j: ObidJob) => j.outgoing?.filter((o) => o.action === 'push' && o.branch === j.branch).at(-1)?.id ?? '';

const call = (path: string, body: unknown) => () => obid('POST', path, body);
const id = encodeURIComponent;

function wanted(): Map<string, Alert> {
  const want = new Map<string, Alert>();
  for (const j of jobs.values()) {
    // another Obi's, or let go: nothing
    if (j.obi || j.dismissed) continue;
    const place = { job: j.id, conversation: j.conversation ?? undefined };
    const say = (what: string, needs: boolean, title: string, body: string, buttons: Button[] = []) =>
      want.set(`job:${j.id}`, { what, needs, title: `${title}: ${j.title}`, body, place, thread: `job-${j.id}`, buttons });
    const p = j.permission;
    if (p && j.status !== 'stopped') {
      const shown = plain(p.detail && !p.title.includes(p.detail) ? `${p.title}\n${p.detail}` : p.title);
      // Allow and Deny for a command or an edit shown whole; the agent's question and Plan mode's
      // plan are answered on its page
      const decide = (decision: 'allow' | 'deny') => call(`/v1/jobs/${id(j.id)}/permission`, { id: p.id, decision });
      const buttons = !p.question && p.tool !== 'ExitPlanMode' && fits(shown) ? [{ text: 'Allow', does: decide('allow') }, { text: 'Deny', does: decide('deny') }] : [];
      say(`permission:${p.id}`, true, p.question ? 'Question' : 'Needs your OK', shown, buttons);
      continue;
    }
    // Oby's review left it to the owner, once per head
    if (j.status === 'done' && !canPublish(j) && j.reviewNeedsYou) {
      if (j.reviewError) say(`review:${pushedHead(j)}`, true, 'Couldn’t review', plain(j.reviewError));
      else say(`review:${pushedHead(j)}`, true, 'Changes requested', 'Oby’s review still asks for changes. Take it from here.');
      continue;
    }
    // Oby's merge stopped where only the owner takes it on (the Swift app's; obid doesn't push it yet)
    if (j.status === 'done' && !canPublish(j) && j.mergeNeedsYou) {
      say(`merge:${pushedHead(j)}`, true, 'Not merged', firstLine(j.mergeError) ?? 'Not merged');
      continue;
    }
    // how it ended, once per end
    const end = (how: string) => `${how}:${j.finished ?? j.created}`;
    const why = (otherwise: string) => firstLine(j.reason) ?? firstLine(j.summary) ?? otherwise;
    if (canPublish(j)) say(end('ready'), true, j.publishError ? 'Couldn’t publish' : 'Ready to publish', firstLine(j.publishError) ?? firstLine(j.summary) ?? 'Done');
    else if (j.status === 'failed') say(end('failed'), true, 'Failed', why('Failed'));
    else if (j.status === 'interrupted' && !j.accountLimit && ![...limits.values()].some((d) => d.jobs.some((job) => job.id === j.id))) say(end('interrupted'), true, 'Needs you', why('Interrupted'));
    else if (j.status === 'done') say(end('done'), false, 'Done', firstLine(j.summary) ?? 'Done');
  }
  for (const d of limits.values()) {
    // The count and "all" on every action describe the scope in the collapsed banner.
    // answerLimit re-reads the decision and signs its revision, so a changed set of jobs
    // cannot be accepted by a click on an older notification.
    const body = `${d.jobs.length} ${d.jobs.length === 1 ? 'task is' : 'tasks are'} waiting on ${plain(d.agent)}'s limit.`;
    const buttons = d.options.map((o): Button => ({
      text: o.kind === 'move' ? `Move all to ${plain(o.name)} · ${plain(o.agent)}` : o.kind === 'resume' ? 'Resume all when available' : 'Keep all stopped',
      does: () => answerLimit(d.id, d.revision, o.kind, o.kind === 'move' ? o.account : undefined),
    }));
    want.set(`limit:${d.id}`, { what: String(d.revision), needs: true, title: 'Usage limit', body, place: {}, thread: 'oby', buttons });
  }
  // From the open inbox: a Confirm, a Start, Allow for the conversation's shell command, and what
  // obid sorts as needing the owner. A job's permission alerts as its job; what one turn offered
  // (oby_offer) is one alert, however many jobs.
  const open = [...inbox.values()].filter((i) => i.state === 'open');
  const offered = new Set<string>();
  for (const i of open) {
    const offer = i.offer?.id;
    if (offer && offered.has(offer)) continue;
    if (offer) offered.add(offer);
    const job = i.job || undefined;
    if (i.kind === 'limit' && job && [...limits.values()].some((d) => d.jobs.some((j) => j.id === job))) continue;
    if (i.kind === 'permission' && job) continue;
    if (!['permission', 'confirm', 'start'].includes(i.kind) && i.triage !== 'needs_you') continue;
    const conversation = i.conversation ?? undefined;
    const many = offer ? open.filter((x) => x.offer?.id === offer).length : 1;
    const text = plain(i.text);
    const answer = (words: string) => ({ text: words, does: call(`/v1/inbox/${id(i.id)}/answer`, { answer: words }) });
    let title = many > 1 ? `Oby offers ${many} jobs` : i.title, body = text, buttons: Button[] = [];
    if (i.kind === 'permission' && fits(text)) buttons = ['Allow', 'Deny'].filter((o) => i.options.includes(o)).map(answer);
    // Start for one job the banner shows whole, from words that read nothing untrusted
    if (i.kind === 'start' && many === 1 && fits(text) && !i.offer?.untrusted && i.options.includes('Start')) buttons = [answer('Start')];
    // What Confirm does is its title: both in the body. Never answered from here: Confirm takes
    // this Mac's signature on the owner's click in the window (device.ts), where what it carries
    // out shows in full.
    if (i.kind === 'confirm') { title = 'Confirm'; body = plain(`${i.title}\n${i.text}`); }
    want.set(offer ? `offer:${offer}` : `item:${i.id}`, {
      what: 'open', needs: true, title, body, place: { job, conversation }, buttons,
      thread: job ? `job-${job}` : conversation ? `conversation-${conversation}` : 'oby',
    });
  }
  return want;
}

// --- the gate

// The one gate every alert passes (the owner's rule of 2026-10-28): nothing while the window is
// in front (what's there is on screen, the rest a click away), and away from it only what waits
// on the owner to decide or act. What each alert needs is decided where it's built (`wanted`, and
// Oby's answers in `look`); change what alerts here, in one place.
export const notifies = (needs: boolean, focused: boolean): boolean => !focused && needs;

// --- on screen

let on = true;
// the window, if it's open; and the window brought up, opened again if it was closed (index.ts)
let current: () => BrowserWindow | null = () => null;
let show!: () => BrowserWindow;
// what the window's page shows ({}: Home), as it says ('showing'); null: something else
let showing: Place | null = null;
let locked = false;
// what was said for each thing (null until the first look: what's there then isn't news), and
// the alerts on screen, kept so their clicks still come and they stay in Notification Center
let said: Map<string, { what: string; needs: boolean }> | null = null;
const shown = new Map<string, { n: Notification; place: Place }>();
// when an alert that needs the owner last came in each conversation: Oby's words beside it
// ("It waits for your Start") say the same, so they don't alert within a minute
const askedAt = new Map<string, number>();

// Whether the window is in front now, on any page: open, not in the Dock, and the one with focus
function focused() {
  const w = current();
  return !!w && w.isVisible() && !w.isMinimized() && w.isFocused();
}

// Whether the window is in front on that place now
function looking(p: Place) {
  if (!showing || !focused()) return false;
  if (p.job && showing.job === p.job) return true;
  if (p.conversation) return showing.conversation === p.conversation;
  // Home, whose stack shows what has no task
  return !p.job && !showing.job && !showing.conversation;
}

function withdraw(key: string) {
  const s = shown.get(key);
  shown.delete(key);
  s?.n.close();
}

function post(key: string, a: Alert) {
  if (a.needs && a.place.conversation) askedAt.set(a.place.conversation, Date.now());
  if (!on || !Notification.isSupported()) return;
  withdraw(key);
  const n = new Notification({
    title: cut(a.title, 100), subtitle: a.subtitle && cut(a.subtitle, 100), body: cut(a.body, 200), groupId: a.thread,
    actions: a.buttons.map((b) => ({ type: 'button' as const, text: b.text })),
  });
  const done = () => { if (shown.get(key)?.n === n) shown.delete(key); };
  n.on('click', () => { done(); open(a.place); });
  n.on('action', async ({ actionIndex }) => {
    done();
    const b = a.buttons[actionIndex];
    // nothing is answered from a locked Mac's alert: the window opens, behind its login
    if (!b || locked || powerMonitor.getSystemIdleState(60) === 'locked') return open(a.place);
    // not taken (answered elsewhere meanwhile, obid away): the window, on it
    const r = await b.does();
    if (r.status < 200 || r.status >= 300) open(a.place);
  });
  shown.set(key, { n, place: a.place });
  n.show();
}

// What each thing needs now against what was said: the alerts that are new or say something
// else, and the ones whose thing is settled go
let ready = false;
function look() {
  if (!ready) return;
  const want = wanted();
  if (!said) {
    said = new Map([...want].map(([key, a]) => [key, { what: a.what, needs: a.needs }]));
    answers.clear();
    return;
  }
  const next = new Map<string, { what: string; needs: boolean }>();
  for (const [key, a] of want) {
    const before = said.get(key);
    // published, reviewed or merged elsewhere: done, and that alert only goes
    if (!a.needs && /^(ready|review|merge):/.test(before?.what ?? '')) withdraw(key);
    else if (before?.what !== a.what) {
      if (notifies(a.needs, focused())) post(key, a);
      // the window in front: nothing now, and what needs the owner alerts once it leaves
      // (windowLeft); what needs nothing is told by the window and never alerts
      else if (a.needs) continue;
    }
    next.set(key, { what: a.what, needs: a.needs });
  }
  for (const [key, s] of said) if (s.needs && !next.has(key)) withdraw(key);
  said = next;
  // Oby's answer asks nothing (its asks come as inbox items, above): FYI, which the gate keeps in
  for (const [c, text] of answers) {
    const place = { conversation: c };
    if (!notifies(false, focused()) || (askedAt.get(c) ?? 0) > Date.now() - 60_000) continue;
    post(`turn:${c}`, { what: '', needs: false, title: 'Oby answered', subtitle: titles.get(c), body: plain(text), place, thread: `conversation-${c}`, buttons: [] });
  }
  answers.clear();
}

// A burst of changes (a job's steps, its end and publish) settles first, as obid's push waits
let timer: ReturnType<typeof setTimeout> | undefined;
const later = () => { timer ??= setTimeout(() => { timer = undefined; look(); }, 2000); };

// A turn of any conversation: the owner's words, from any app, settle Oby's answer there; Oby's
// answer alerts when it's news (a turn comes again when a job it started changes), recent, and
// neither a job's end (its own alert says it) nor a routine's run
function heard(t: Turn) {
  const c = t.conversation;
  if (!c) return;
  if (t.who === 'user') { answers.delete(c); withdraw(`turn:${c}`); return; }
  if (t.ended || t.routine != null || !t.text.trim()) return;
  const before = lastAnswer.get(c);
  if (before && (t.at < before.at || t.id === before.id)) return;
  lastAnswer.set(c, t);
  if (Date.parse(t.at) > Date.now() - 600_000) answers.set(c, t.text);
}

function take(e: Event) {
  if (e.kind === 'job') jobs.set(e.job.id, e.job as ObidJob);
  else if (e.kind === 'jobGone') jobs.delete(e.id);
  else if (e.kind === 'inbox' && e.item.state === 'done') inbox.delete(e.item.id);
  else if (e.kind === 'inbox') inbox.set(e.item.id, e.item);
  else if (e.kind === 'turn') heard(e.turn);
  else if (e.kind === 'conversation') return void titles.set(e.conversation.id, e.conversation.title);
  else if (e.kind === 'conversationGone') {
    titles.delete(e.id); lastAnswer.delete(e.id); answers.delete(e.id); askedAt.delete(e.id);
    return void withdraw(`turn:${e.id}`);
  } else return;
  later();
}

// --- following obid

const FOLLOWED = new Set(['job', 'inbox', 'turn', 'conversation']);
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));
const json = <T>(a: Answer): T | null => { try { return a.status === 200 ? JSON.parse(a.text) as T : null; } catch { return null; } };

// What obid has now: the jobs, the inbox and the conversations' titles
async function sync(): Promise<boolean> {
  const limitVersion = ++limitsRequest;
  const [js, items, cs, decisions] = await Promise.all(['/v1/jobs', '/v1/inbox', '/v1/conversations?limit=200', '/v1/limit-decisions'].map((p) => obid('GET', p)));
  const listed = json<ObidJob[]>(js), open = json<InboxItem[]>(items);
  if (!listed || !open) return false;
  jobs.clear();
  for (const j of listed) jobs.set(j.id, j);
  inbox.clear();
  for (const i of open) if (i.state !== 'done') inbox.set(i.id, i);
  const active = json<LimitDecision[]>(decisions);
  if (active && limitVersion === limitsRequest) { limits.clear(); for (const d of active) limits.set(d.id, d); }
  titles.clear();
  for (const c of json<Conversation[]>(cs) ?? []) titles.set(c.id, c.title);
  return true;
}

// The stream, again whenever it ends (obid restarted, or not up yet): what came meanwhile is
// compared with what was said, so nothing alerts twice
async function follow() {
  for (let retry = 0; ; await sleep(Math.min(30_000, 1000 * 2 ** retry++))) {
    const control = new AbortController();
    const r = await send('GET', '/v1/events', undefined, control.signal);
    if (!(r instanceof Response)) continue;
    if (!r.ok || !r.body) { await r.body?.cancel().catch(() => {}); continue; }
    let synced = false;
    const pending: Event[] = [];
    const read = sseReader((name, data) => {
      if (name === 'limitDecision' || name === 'job' || name === 'accounts') {
        void refreshLimits();
      }
      const e = FOLLOWED.has(name) ? parseEvent(name, data) : null;
      if (e && synced) take(e);
      else if (e) pending.push(e);
    });
    const reader = r.body.pipeThrough(new TextDecoderStream()).getReader();
    const reading = (async () => { for (;;) { const { value, done } = await reader.read(); if (done) return; read(value); } })();
    // obid follows the stream before it answers: what it has now, with the events that came
    // meanwhile over it
    if (await sync()) {
      synced = true;
      pending.splice(0).forEach(take);
      ready = true;
      retry = 0;
      look();
    } else control.abort();
    await reading.catch(() => {});
  }
}

// --- the window

// Where a clicked alert opens, until the page asks for it ('opened')
let opening: Place | null = null;
function open(place: Place) {
  opening = place;
  show().webContents.send('open');
}

// The window went (closed, minimized, behind another app): what needs the owner and waited while
// it was in front alerts now
export const windowLeft = () => look();
// The window is in front: the alerts for what it shows go
export function windowSeen() {
  for (const [key, s] of shown) if (looking(s.place)) withdraw(key);
}

// Settings › Notifications: Oby's alerts on or off, kept in the app's own folder
const SETTINGS = () => join(app.getPath('userData'), 'settings.json');
const settings = (): Record<string, unknown> => { try { return JSON.parse(readFileSync(SETTINGS(), 'utf8')); } catch { return {}; } };
// macOS's own settings for Oby's alerts (allowed, banners, sounds, Focus). ponytail: the packaged
// app's bundle id, electron-builder.yml's appId; in development, the list
const SYSTEM = `x-apple.systempreferences:com.apple.Notifications-Settings.extension${app.isPackaged ? '?id=com.deskree.oby.electron' : ''}`;

export function notifying(trusted: Trusted, window: () => BrowserWindow | null, bringUp: () => BrowserWindow) {
  current = window;
  show = bringUp;
  on = settings().notifications !== false;
  powerMonitor.on('lock-screen', () => { locked = true; });
  powerMonitor.on('unlock-screen', () => { locked = false; });
  ipcMain.on('showing', (e, p: unknown) => {
    if (!trusted(e)) return;
    const s = p && typeof p === 'object' ? p as Record<string, unknown> : null;
    const str = (v: unknown) => (typeof v === 'string' ? v : undefined);
    showing = s && { job: str(s.job), conversation: str(s.conversation) };
    windowSeen();
  });
  ipcMain.handle('opened', (e) => {
    if (!trusted(e)) return null;
    const place = opening;
    opening = null;
    return place;
  });
  ipcMain.handle('notifications', (e, turn?: unknown) => {
    if (!trusted(e)) return null;
    if (typeof turn === 'boolean' && turn !== on) {
      on = turn;
      writeOwn(SETTINGS(), JSON.stringify({ ...settings(), notifications: on }), constants.O_TRUNC);
      if (!on) [...shown.keys()].forEach(withdraw);
    }
    return on;
  });
  ipcMain.on('open-notification-settings', (e) => { if (trusted(e)) void shell.openExternal(SYSTEM); });
  setInterval(() => { if (ready) void refreshLimits(); }, 15_000);
  void follow();
}
