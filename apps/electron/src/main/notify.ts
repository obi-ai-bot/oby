import { app, ipcMain, Notification, powerMonitor, shell, type BrowserWindow } from 'electron';
import { randomUUID } from 'node:crypto';
import { closeSync, constants, openSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { obid, send, takesKeys, type Answer, type Trusted } from './obid';
import { answerLimit, notificationWords, writeOwn } from './device';
import { parseEvent, plainId, sseReader, type Conversation, type Event, type InboxItem, type LimitDecision, type Turn } from '../shared/obid-wire';

// Oby's alerts on this Mac (docs/contracts/alerts.md). What waits on the owner is obid's one list,
// `GET /v1/alerts` (ALERTS-1: core/crates/obid/src/push.rs, `wanted`), the same the phones are
// told, in the same words; the Mac shows what it serves and decides nothing about what alerts. It
// keeps the owner's rule of 2026-10-28 (`notifies`): nothing while the window is in front, on any
// page (ALERTS-8). And what only the Mac says: the answer to a question the owner asked another
// Oby, which its card promises (`pingOf`), and Oby's answer in a conversation, FYI the gate keeps
// in. One alert per thing (obid's `key`), replaced when what it needs (`what`) changes, gone once
// it's settled anywhere; nothing that was there when Oby first opened with this.
// Main follows obid's event stream for them by itself, from launch until Oby quits, with the
// window closed too (the page's own stream ends with the window), and reads the list again after
// each burst. A ping every 15 seconds is all it hears while nothing happens.

// Where an alert's click opens the window: a job's page, else a conversation, else Home
type Place = { job?: string; conversation?: string };
// A button: obid's answer, as the same tap in the window gives it
type Button = { text: string; does: () => Promise<Answer> };
type Alert = {
  // whether it needs the owner to decide or act (the gate's one question, `notifies`): what obid
  // serves does; Oby's answer (`needs` false) is FYI, which the gate keeps in
  needs: boolean;
  title: string;
  subtitle?: string;
  body: string;
  place: Place;
  // its stack in Notification Center: obid's thread (a conversation, else jobs or the inbox)
  thread: string;
  buttons: Button[];
};

// One alert of `GET /v1/alerts` (docs/runner-api.md)
type Served = {
  key: string; what: string; title: string; body: string; category: string; thread: string;
  job?: string; conversation?: string; item?: string; answers: string[];
};
const str = (v: unknown) => (typeof v === 'string' ? v : undefined);
const servedOf = (v: unknown): Served | null => {
  const a = v && typeof v === 'object' ? v as Record<string, unknown> : {};
  const [key, what, title, body, category, thread] = [a.key, a.what, a.title, a.body, a.category, a.thread].map(str);
  if (!key || what === undefined || title === undefined || body === undefined || !category || !Array.isArray(a.answers)) return null;
  return { key, what, title, body, category, thread: thread ?? 'oby', job: str(a.job), conversation: str(a.conversation), item: str(a.item),
    answers: a.answers.filter((w): w is string => typeof w === 'string') };
};

// --- words: obid's text is data, shown as plain text

// Each line without control or format characters (a bidi override, a zero-width space) that could
// hide or reorder what's shown, and no blank ones (Alerts.swift, compact)
const plain = (text: string) => text.split('\n')
  .map((l) => l.replace(/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]/gu, ' ').replace(/\s+/g, ' ').trim()).filter(Boolean).join('\n');
const cut = (text: string, most: number) => { const c = [...text]; return c.length <= most ? text : `${c.slice(0, most - 1).join('')}…`; };

// --- what alerts: obid's list, drawn

// obid's list as last read; null: not read yet, or obid couldn't serve it (nothing changes then)
let served: Served[] | null = null;
let servedRequest = 0;
const readServed = async () => json<unknown[]>(await obid('GET', '/v1/alerts'))?.map(servedOf).filter((a): a is Served => !!a) ?? null;
// The list again, then a look; an older read that answers last changes nothing
async function refresh() {
  const request = ++servedRequest;
  const list = await readServed();
  if (request !== servedRequest) return;
  if (!list) return again();
  failures = 0;
  served = list;
  look();
}
// obid couldn't serve its list (its vault unreadable, a request lost): the last one stands, and
// it's read again by itself, 2 seconds later, then twice as long each time, at most a minute apart,
// so what waits still reaches the owner with no other event to wake a look
let retry: ReturnType<typeof setTimeout> | undefined;
let failures = 0;
function again() {
  if (retry) return;
  const wait = Math.min(60_000, 2000 * 2 ** failures++);
  retry = setTimeout(() => { retry = undefined; void refresh(); }, wait);
}
const inbox = new Map<string, InboxItem>();
// The limit decisions, for a `limitDecision` alert's choices; obid's list is read again only when
// they change, so the 15 seconds' poll reads nothing more while nothing happens
const limits = new Map<string, LimitDecision>();
let limitsRequest = 0;
async function refreshLimits() {
  const request = ++limitsRequest;
  const active = json<LimitDecision[]>(await obid('GET', '/v1/limit-decisions'));
  if (request !== limitsRequest || !active) return;
  const before = JSON.stringify([...limits.values()]);
  limits.clear();
  for (const d of active) limits.set(d.id, d);
  if (JSON.stringify(active) !== before) later();
}
const titles = new Map<string, string>();
// Oby's last answer heard in each conversation, and the ones to alert at the next look
const lastAnswer = new Map<string, Turn>();
const answers = new Map<string, string>();

// One press of an inbox item's button (RECEIPTS-16): once main's own read says obid takes keys,
// the press's own key goes with its answer, and an answer obid didn't answer goes again, the same
// body and key, a few times a second apart, as the window's do (live.ts, sent). Allow and Deny are
// signed afresh each time; any answer of obid's is final.
async function pressed(path: string, words: string, signs: boolean, tries = 3, pause = 1000): Promise<Answer> {
  const body = await takesKeys() ? { answer: words, clientRequestId: randomUUID() } : { answer: words };
  for (let n = 1; ; n++) {
    const r = await (signs ? notificationWords('POST', path, body) : obid('POST', path, body));
    if (r.status !== 0 || !('clientRequestId' in body) || n >= tries) return r;
    await sleep(pause);
  }
}
// Another Oby's question: who by their owner id as its card shows it, and their words as plain text,
// since obid's own say they're "quoted below", and here nothing is. Answered on its card, where the
// click signs it, so no button (PEER-33).
const questionOf = ({ kind, obi: o }: InboxItem) => (kind === 'obiRequest' && o?.type === 'question' && typeof o.quoted?.text === 'string' && typeof o.fromOwner === 'string'
  ? { title: `Question from ${plainId(o.fromOwner)}`, body: plain(o.quoted.text) } : null);
const id = encodeURIComponent;

export const waiting = (want: Map<string, { needs: boolean }>) => {
  const n = [...want.values()].filter((a) => a.needs).length;
  return n ? String(n) : '';
};

// The buttons of one alert: obid's `answers`, each answered where the window answers that thing
// (its inbox item: a job's permission is one too), Allow and Deny signed as the window's are. A
// Confirm has none, whatever comes: it takes this Mac's signature on the owner's click in the
// window (device.ts), where what it carries out shows in full. A limit decision keeps its own
// choices, answered through answerLimit, which re-reads the decision and signs its revision, so a
// changed set of jobs can't be accepted by a click on an older alert.
function buttonsOf(a: Served): Button[] {
  if (a.category === 'confirm') return [];
  if (a.category === 'limitDecision') {
    const d = limits.get(a.item ?? a.key);
    // the count and "all" on every action describe the scope in the collapsed banner
    return d ? d.options.map((o): Button => ({
      text: o.kind === 'move' ? `Move all to ${plain(o.name)} · ${plain(o.agent)}` : o.kind === 'resume' ? 'Resume all when available' : 'Keep all stopped',
      does: () => answerLimit(d.id, d.revision, o.kind, o.kind === 'move' ? o.account : undefined),
    })) : [];
  }
  if (!a.item) return [];
  const path = `/v1/inbox/${id(a.item)}/answer`;
  return a.answers.map((words) => ({ text: words, does: () => pressed(path, words, words === 'Allow' || words === 'Deny') }));
}

// One alert of obid's as the Mac draws it: obid's words as plain text, another Oby's question as
// its card shows it (`questionOf`, answered on its card), and a click that opens its job or
// conversation
function drawn(a: Served): Alert {
  const item = a.item ? inbox.get(a.item) : undefined;
  const question = item ? questionOf(item) : null;
  return {
    needs: true, title: question?.title ?? plain(a.title), body: question?.body ?? plain(a.body),
    place: { job: a.job, conversation: a.conversation }, thread: a.thread, buttons: question ? [] : buttonsOf(a),
  };
}

// A refused alert still gets the owner's attention: the Dock's icon bounces until they look.
export function refused(why: string, dock: { bounce?: (kind: 'critical') => unknown } | undefined = app.dock) {
  console.error(`Oby: macOS refused an alert: ${why}`);
  dock?.bounce?.('critical');
}

// --- the gate

// The one gate every alert passes (the owner's rule of 2026-10-28): nothing while the window is
// in front (what's there is on screen, the rest a click away), and away from it only what waits
// on the owner to decide or act. What needs the owner is obid's list (ALERTS-1), where what
// alerts changes; Oby's answers are FYI (`look`).
export const notifies = (needs: boolean, focused: boolean): boolean => !focused && needs;

// --- on screen

let on = true;
// the window, if it's open; and the window brought up, opened again if it was closed (index.ts)
let current: () => BrowserWindow | null = () => null;
let show!: () => BrowserWindow;
// what the window's page shows ({}: Home), as it says ('showing'); null: something else
let showing: Place | null = null;
let locked = false;
// what was said for each of obid's keys (its `what`), kept across launches in the app's own folder
// (ALERTS-2): what waited while Oby was closed alerts once when it opens, and a restart repeats
// nothing. null: nothing kept yet (the first launch with this), so the first look's is the
// baseline, not news. `-v2`: the keys became obid's, so what the Mac said under its own keys
// isn't news again.
// And the alerts on screen, kept so their clicks still come and they stay in Notification Center
let said: Map<string, string> | null = null;
const SAID = () => join(app.getPath('userData'), 'alerts-said-v2.json');
let kept = '';
// What the last launch said, read without following a link: a file that can't be read or parsed
// (a link among them) is an empty map (what waits alerts), never a throw and never a reason to
// stay quiet
function keptSaid(): typeof said {
  try {
    const fd = openSync(SAID(), constants.O_RDONLY | constants.O_NOFOLLOW);
    try { kept = readFileSync(fd, 'utf8'); } finally { closeSync(fd); }
  } catch (e) { if ((e as NodeJS.ErrnoException).code === 'ENOENT') return null; }
  try {
    return new Map(Object.entries(JSON.parse(kept) as Record<string, unknown>).filter((e): e is [string, string] => typeof e[1] === 'string'));
  } catch { return new Map(); }
}
const shown = new Map<string, { n: Notification; place: Place }>();
// when an alert that needs the owner last came in each conversation: Oby's words beside it
// ("It waits for your Start") say the same, so they don't alert within a minute
const askedAt = new Map<string, number>();

// Whether the window is in front now, on any page: open, not in the Dock, and the one with focus
function focused() {
  const w = current();
  return !!w && w.isVisible() && !w.isMinimized() && w.isFocused();
}

// Whether what the window's page shows has that place on it
export function onPage(showing: Place, p: Place) {
  if (p.job && showing.job === p.job) return true;
  if (p.conversation) return showing.conversation === p.conversation;
  // the stack shows what has no task: on Home, and chat first over a conversation too
  // (docs/contracts/chat-first.md, CHATFIRST-8). ponytail: with ?home a conversation has no stack, and
  // this takes such an alert away all the same; only Sangam's comparison sees it
  return !p.job && !showing.job;
}
// Whether the window is in front on that place now
const looking = (p: Place) => !!showing && focused() && onPage(showing, p);

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
  // macOS can refuse (notifications off for Oby, or never allowed): the Dock still says so, and the log why
  n.on('failed', (_e, why) => refused(why));
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

// What obid serves now against what was said: the alerts that are new or say something else, and
// the ones whose thing is settled go
let ready = false;
function look() {
  if (!ready || !served) return;
  const want = new Map(served.map((a) => [a.key, a]));
  // nothing kept: what's there isn't news, and the badge still counts it
  if (!said) {
    said = new Map([...want].map(([key, a]) => [key, a.what]));
    answers.clear();
  }
  const next = new Map<string, string>();
  for (const [key, a] of want) {
    if (said.get(key) !== a.what) {
      // the window in front: nothing now, and it alerts once the window leaves (windowLeft)
      if (!notifies(true, focused())) continue;
      post(key, drawn(a));
    }
    next.set(key, a.what);
  }
  for (const key of said.keys()) if (!next.has(key)) withdraw(key);
  said = next;
  const text = JSON.stringify(Object.fromEntries(said));
  if (text !== kept) {
    try { writeOwn(SAID(), text, constants.O_TRUNC); kept = text; } catch (e) { console.error(`Oby: couldn’t keep what alerts said: ${e}`); }
  }
  // How many things wait on the owner, on the Dock's icon, whether or not macOS lets alerts through
  app.dock?.setBadge(waiting(new Map([...want.keys()].map((key) => [key, { needs: true }]))));
  for (const [c, p] of pings) {
    if (notifies(true, focused())) post(`ask:${p.id}`, { needs: true, title: p.title, subtitle: titles.get(c), body: plain(p.body), place: { conversation: c }, thread: c, buttons: [] });
  }
  pings.clear();
  // Oby's answer asks nothing (its asks come as inbox items, in obid's list): FYI, which the gate keeps in
  for (const [c, text] of answers) {
    const place = { conversation: c };
    if (!notifies(false, focused()) || (askedAt.get(c) ?? 0) > Date.now() - 60_000) continue;
    post(`turn:${c}`, { needs: false, title: 'Oby answered', subtitle: titles.get(c), body: plain(text), place, thread: c, buttons: [] });
  }
  answers.clear();
}

// The answer to a question the owner asked another Oby, once it's in: its card said "I'll ping you".
// Only its end: "still waiting" is no answer. Their words stay data: plain text in an alert, and a
// name from their card, cleaned as the inbox's are.
export const pingOf = (t: Pick<Turn, 'asks' | 'text'>) => {
  const a = t.asks?.find((c) => ['answered', 'declined', 'expired'].includes(c.state));
  return a && t.text.trim() ? { title: `${plainId(a.name)}’s Oby answered`, body: t.text } : null;
};
const pings = new Map<string, { id: string; title: string; body: string }>();

// A burst of changes (a job's steps, its end and publish) settles first, as obid's push waits;
// then obid's list is read again. A turn alone only looks again: obid scans every alert it serves
// for secrets, and a reply streams many turns
let timer: ReturnType<typeof setTimeout> | undefined;
let reread = false;
const later = (read = true) => {
  reread ||= read;
  timer ??= setTimeout(() => { timer = undefined; const r = reread; reread = false; if (r) void refresh(); else look(); }, 2000);
};

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
  if (Date.parse(t.at) > Date.now() - 600_000) {
    answers.set(c, t.text);
    const ping = pingOf(t);
    if (ping) pings.set(c, { id: t.id, ...ping });
  }
}

function take(e: Event) {
  if (e.kind === 'inbox' && e.item.state === 'done') inbox.delete(e.item.id);
  else if (e.kind === 'inbox') inbox.set(e.item.id, e.item);
  else if (e.kind === 'turn') return void (heard(e.turn), later(false));
  else if (e.kind === 'conversation') return void titles.set(e.conversation.id, e.conversation.title);
  else if (e.kind === 'conversationGone') {
    titles.delete(e.id); lastAnswer.delete(e.id); answers.delete(e.id); askedAt.delete(e.id);
    return void withdraw(`turn:${e.id}`);
  // a job changed: obid's list says what it needs now
  } else if (e.kind !== 'job' && e.kind !== 'jobGone') return;
  later();
}

// --- following obid

const FOLLOWED = new Set(['job', 'inbox', 'turn', 'conversation']);
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));
const json = <T>(a: Answer): T | null => { try { return a.status === 200 ? JSON.parse(a.text) as T : null; } catch { return null; } };

// What obid has now: what waits on the owner, the inbox, the limit decisions and the
// conversations' titles. A list obid couldn't serve (its vault unreadable) changes nothing, and is
// read again (`again`).
async function sync(): Promise<boolean> {
  // whether obid takes keys, read again whenever obid's stream starts (pressed, device.ts)
  void takesKeys();
  const limitVersion = ++limitsRequest, servedVersion = ++servedRequest;
  const [list, [items, cs, decisions]] = await Promise.all([readServed(), Promise.all(['/v1/inbox', '/v1/conversations?limit=200', '/v1/limit-decisions'].map((p) => obid('GET', p)))]);
  const open = json<InboxItem[]>(items);
  if (!open) return false;
  if (servedVersion === servedRequest) {
    if (list) { failures = 0; served = list; } else again();
  }
  inbox.clear();
  for (const i of open) if (i.state !== 'done') inbox.set(i.id, i);
  const active = json<LimitDecision[]>(decisions);
  if (active && limitVersion === limitsRequest) { limits.clear(); for (const d of active) limits.set(d.id, d); }
  titles.clear();
  for (const c of json<Conversation[]>(cs) ?? []) titles.set(c.id, c.title);
  return true;
}

// The stream, again whenever it ends (obid restarted, or not up yet): what came meanwhile is
// compared with what was said, so nothing alerts twice. `alerts=1` tells obid it's the Mac app
// following: it opens Oby hidden only while none is (ALERTS-3)
async function follow() {
  for (let retry = 0; ; await sleep(Math.min(30_000, 1000 * 2 ** retry++))) {
    const control = new AbortController();
    const r = await send('GET', '/v1/events?alerts=1', undefined, control.signal);
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

// Oby's own settings in the app's own folder: Settings › Notifications' alerts here, About's Update
// automatically (updates.ts)
export const SETTINGS = () => join(app.getPath('userData'), 'settings.json');
export const settings = (): Record<string, unknown> => { try { return JSON.parse(readFileSync(SETTINGS(), 'utf8')); } catch { return {}; } };
// macOS's own settings for Oby's alerts (allowed, banners, sounds, Focus). ponytail: the packaged
// app's bundle id, electron-builder.yml's appId; in development, the list
const SYSTEM = `x-apple.systempreferences:com.apple.Notifications-Settings.extension${app.isPackaged ? '?id=com.deskree.oby.electron' : ''}`;

export function notifying(trusted: Trusted, window: () => BrowserWindow | null, bringUp: () => BrowserWindow) {
  current = window;
  show = bringUp;
  on = settings().notifications !== false;
  said = keptSaid();
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
