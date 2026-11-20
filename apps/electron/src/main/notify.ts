import { app, Notification, powerMonitor, shell, type BrowserWindow } from 'electron';
import { randomUUID } from 'node:crypto';
import { closeSync, constants, openSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { obid, takesKeys, type Answer, type Trusted } from './obid';
import { answerLimit, notificationWords, writeOwn } from './device';
import { ipc } from './ipc';
import { limitDecisions, obidState, onLive } from './live';
import { settings, writeSettings } from './settings';
import { boolean, optional, orUndefined, type Decoder } from '../shared/decode';
import type { LiveMessage } from '../shared/live-wire';
import { plainId, type Event, type InboxItem, type Turn } from '../shared/obid-wire';

// Oby's alerts on this Mac (docs/contracts/alerts.md). What waits on the owner is obid's one list,
// `GET /v1/alerts` (ALERTS-1: core/crates/obid/src/push.rs, `wanted`), the same the phones are
// told, in the same words; the Mac shows what it serves and decides nothing about what alerts. It
// keeps the owner's rule of 2026-10-28 (`notifies`): nothing while the window is in front, on any
// page (ALERTS-8). And what only the Mac says: the answer to a question the owner asked another
// Oby, which its card promises (`pingOf`), and Oby's answer in a conversation, FYI the gate keeps
// in. One alert per thing (obid's `key`), replaced when what it needs (`what`) changes, gone once
// it's settled anywhere; nothing that was there when Oby first opened with this. The owner's
// word of 2026-11-15 adds three kinds of news that need nothing, each with its switch in Settings ›
// Notifications (ALERTS-12 to 18): obid serves them only when the read names them (`fyi`), and the
// Mac shows them quietly: no buttons, not on the badge, never later for what the window showed.
// What obid has (its inbox, limit decisions and conversations' titles) is main's one follower's
// (live.ts), from launch until Oby quits, with the window closed too: the alerts take its
// messages in the order it applied them, and read the list again after each sync and each burst.

// Where an alert's click opens the window: a job's page, else a conversation, else a Settings page
// (an account at its limit: the one listing accounts), else Home
type Place = { job?: string; conversation?: string; settings?: string };
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

// The news that needs nothing obid serves when asked (`fyi`), in the order the read names them
const KINDS = ['finished', 'merged', 'limit'] as const;
type Kind = typeof KINDS[number];
type Kinds = Record<Kind, boolean>;
// each kind's switch in settings.json, on unless it's false
const SWITCH: Record<Kind, string> = { finished: 'notifyFinished', merged: 'notifyMerged', limit: 'notifyLimit' };

// One alert of `GET /v1/alerts` (docs/runner-api.md)
type Served = {
  key: string; what: string; title: string; body: string; category: string; thread: string;
  job?: string; conversation?: string; item?: string; answers: string[];
  // news that needs nothing (ALERTS-12 to 14), and an account's reset
  fyi?: string; resets?: string;
};
const str = (v: unknown) => (typeof v === 'string' ? v : undefined);
const servedOf = (v: unknown): Served | null => {
  const a = v && typeof v === 'object' ? v as Record<string, unknown> : {};
  const [key, what, title, body, category, thread] = [a.key, a.what, a.title, a.body, a.category, a.thread].map(str);
  if (!key || what === undefined || title === undefined || body === undefined || !category || !Array.isArray(a.answers)) return null;
  return { key, what, title, body, category, thread: thread ?? 'oby', job: str(a.job), conversation: str(a.conversation), item: str(a.item),
    answers: a.answers.filter((w): w is string => typeof w === 'string'), fyi: str(a.fyi), resets: str(a.resets) };
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
// the kinds of news switched on, and those just turned on: their first list is no news (ALERTS-18)
let kinds: Kinds = { finished: true, merged: true, limit: true };
const turnedOn = new Set<string>();
// obid's list, with the news switched on; plain when none is (ALERTS-15)
const alertsPath = () => {
  const fyi = KINDS.filter((k) => kinds[k]);
  return fyi.length ? `/v1/alerts?fyi=${fyi.join(',')}` : '/v1/alerts';
};
const readServed = async () => json<unknown[]>(await obid('GET', alertsPath()))?.map(servedOf).filter((a): a is Served => !!a) ?? null;
// The list again, then a look; an older read that answers last changes nothing
async function refresh() {
  const request = ++servedRequest;
  const quiet = new Set(turnedOn);
  const list = await readServed();
  if (request !== servedRequest) return;
  if (!list) return again();
  failures = 0;
  served = list;
  look(quiet);
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
// a conversation's title, as main holds it
const titleOf = (c: string) => obidState().conversations.find((x) => x.id === c)?.title;
// Oby's last answer heard in each conversation, and the ones to alert at the next look
const lastAnswer = new Map<string, Turn>();
const answers = new Map<string, string>();

// One press of an inbox item's button (RECEIPTS-16): once main's own read says obid takes keys,
// the press's own key goes with its answer, and an answer obid didn't answer goes again, the same
// body and key, a few times a second apart, as the window's do (live.ts, sent). Those in SIGNED are
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

// a limit's reset as the window's limit card says it, in this Mac's time (live.ts)
const resetsAt = (at: string | undefined) => {
  const t = at ? Date.parse(at) : NaN;
  return Number.isNaN(t) ? null : `It resets ${new Date(t).toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' })}.`;
};

export const waiting = (want: Map<string, { needs: boolean }>) => {
  const n = [...want.values()].filter((a) => a.needs).length;
  return n ? String(n) : '';
};

// The answers main signs on the owner's own click of the banner's action (`notificationWords`):
// a permission's Allow and Deny, and a teammate's review request's Accept and Decline, which obid
// refuses unsigned (SPHERE-17, alerts.md ALERTS-22). Start goes with obid's token, as the window's.
const SIGNED = new Set(['Allow', 'Deny', 'Accept', 'Decline']);

// The buttons of one alert: obid's `answers`, each answered where the window answers that thing
// (its inbox item: a job's permission is one too), those in SIGNED signed as the window's are. A
// Confirm has none, whatever comes: it takes this Mac's signature on the owner's click in the
// window (device.ts), where what it carries out shows in full. A limit decision keeps its own
// choices, answered through answerLimit, which re-reads the decision and signs its revision, so a
// changed set of jobs can't be accepted by a click on an older alert.
function buttonsOf(a: Served): Button[] {
  if (a.category === 'confirm') return [];
  if (a.category === 'limitDecision') {
    const d = limitDecisions().find((x) => x.id === (a.item ?? a.key));
    // the count and "all" on every action describe the scope in the collapsed banner
    return d ? d.options.map((o): Button => ({
      text: o.kind === 'move' ? `Move all to ${plain(o.name)} · ${plain(o.agent)}` : o.kind === 'resume' ? 'Resume all when available' : 'Keep all stopped',
      does: () => answerLimit(d.id, d.revision, o.kind, o.kind === 'move' ? o.account : undefined),
    })) : [];
  }
  if (!a.item) return [];
  const path = `/v1/inbox/${id(a.item)}/answer`;
  return a.answers.map((words) => ({ text: words, does: () => pressed(path, words, SIGNED.has(words)) }));
}

// One alert of obid's as the Mac draws it: obid's words as plain text, another Oby's question as
// its card shows it (`questionOf`, answered on its card), and a click that opens its job or
// conversation. News that needs nothing has no buttons and opens its conversation, else its job,
// or for an account the Settings page listing accounts, its reset in this Mac's time.
function drawn(a: Served): Alert {
  if (a.fyi) {
    const place = a.fyi === 'limit' ? { settings: 'agents' } : a.conversation ? { conversation: a.conversation } : { job: a.job };
    const body = (a.fyi === 'limit' && resetsAt(a.resets)) || plain(a.body);
    return { needs: false, title: plain(a.title), body, place, thread: a.thread, buttons: [] };
  }
  const item = a.item ? obidState().inbox.find((i) => i.id === a.item) : undefined;
  const question = item ? questionOf(item) : null;
  return {
    needs: true, title: question?.title ?? plain(a.title), body: question?.body ?? plain(a.body),
    place: { job: a.job, conversation: a.conversation }, thread: a.thread, buttons: question ? [] : buttonsOf(a),
  };
}

// A refused alert is logged, and the Dock's badge still counts what waits (`waiting`). The Dock's
// icon never bounces for it: the owner's word, 2026-11-17 ("I hate that jumping Oby on the Dock").
export function refused(why: string, dock: { bounce?: (kind: 'critical') => unknown } | undefined = app.dock) {
  console.error(`Oby: macOS refused an alert: ${why}`);
  void dock;
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
  // a Settings page is no page alerts know: its alert goes once obid stops serving it
  if (p.settings) return false;
  if (p.job && showing.job === p.job) return true;
  if (p.conversation) return showing.conversation === p.conversation;
  // the stack shows what has no task or conversation only on Home: never over a conversation
  // (docs/contracts/chat-first.md, CHATFIRST-8)
  return !p.job && !showing.job && !showing.conversation;
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
// the ones whose thing is settled go. `quiet`: kinds just turned on, whose news in this first list
// is said without a banner
let ready = false;
function look(quiet = new Set<string>()) {
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
      // the window in front: nothing now, and it alerts once the window leaves (windowLeft); news
      // that needs nothing was on screen, so it's said with no banner, as in its kind's first list
      if (!a.fyi && !notifies(true, focused())) continue;
      if (!a.fyi || (!focused() && !quiet.has(a.fyi))) post(key, drawn(a));
    }
    next.set(key, a.what);
  }
  for (const k of quiet) turnedOn.delete(k);
  for (const key of said.keys()) if (!next.has(key)) withdraw(key);
  said = next;
  const text = JSON.stringify(Object.fromEntries(said));
  if (text !== kept) {
    try { writeOwn(SAID(), text, constants.O_TRUNC); kept = text; } catch (e) { console.error(`Oby: couldn’t keep what alerts said: ${e}`); }
  }
  // How many things wait on the owner, on the Dock's icon, whether or not macOS lets alerts through
  app.dock?.setBadge(waiting(new Map([...want].map(([key, a]) => [key, { needs: !a.fyi }]))));
  for (const [c, p] of pings) {
    if (notifies(true, focused())) post(`ask:${p.id}`, { needs: true, title: p.title, subtitle: titleOf(c), body: plain(p.body), place: { conversation: c }, thread: c, buttons: [] });
  }
  pings.clear();
  // Oby's answer asks nothing (its asks come as inbox items, in obid's list): FYI, which the gate keeps in
  for (const [c, text] of answers) {
    const place = { conversation: c };
    if (!notifies(false, focused()) || (askedAt.get(c) ?? 0) > Date.now() - 60_000) continue;
    post(`turn:${c}`, { needs: false, title: 'Oby answered', subtitle: titleOf(c), body: plain(text), place, thread: c, buttons: [] });
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
  if (e.kind === 'turn') return void (heard(e.turn), later(false));
  if (e.kind === 'conversationGone') {
    lastAnswer.delete(e.id); answers.delete(e.id); askedAt.delete(e.id);
    return void withdraw(`turn:${e.id}`);
  }
  // a job or an item changed: obid's list says what it needs now
  if (e.kind === 'job' || e.kind === 'jobGone' || e.kind === 'inbox') later();
}

// --- main's state (live.ts)

const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));
const json = <T>(a: Answer): T | null => { try { return a.status === 200 ? JSON.parse(a.text) as T : null; } catch { return null; } };

// What main sends the page, as it sends it: after each sync, what waits on the owner (a list obid
// couldn't serve, its vault unreadable, changes nothing and is read again, `again`), then a look;
// each event, in the order main applied it, the ones that came during the sync after it; and a
// change of the limit decisions, whose `limitDecision` alerts' choices it is
function heardLive(m: LiveMessage) {
  if (m.kind === 'snapshot') void synced();
  else if (m.kind === 'event') take(m.event);
  else if (m.kind === 'limits') later();
}
async function synced() {
  const request = ++servedRequest;
  const quiet = new Set(turnedOn);
  const list = await readServed();
  const current = request === servedRequest;
  if (current) {
    if (list) { failures = 0; served = list; } else again();
  }
  ready = true;
  look(current && list ? quiet : undefined);
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

// macOS's own settings for Oby's alerts (allowed, banners, sounds, Focus): Notifications › Oby
// where the app has its bundle id (a packaged one, electron-builder.yml's appId; the list in
// development), else the Notifications pane. These two are the only URLs main opens for the
// page's click: it passes none (ALERTS-19).
const PANE = 'x-apple.systempreferences:com.apple.Notifications-Settings.extension';
const OBY = `${PANE}?id=com.deskree.oby.electron`;
async function openSystemSettings() {
  for (const url of app.isPackaged ? [OBY, PANE] : [PANE]) {
    try { await shell.openExternal(url); return; } catch (e) { console.error(`Oby: couldn’t open System Settings at ${url}: ${e}`); }
  }
}

// What the window shows, as the page says: a job's or a conversation's (anything else in it
// dropped), or nothing; never refused
const place: Decoder<Place | null> = (p) => {
  const s = p && typeof p === 'object' ? p as Record<string, unknown> : null;
  const str = (v: unknown) => (typeof v === 'string' ? v : undefined);
  return { value: s && { job: str(s.job), conversation: str(s.conversation) } };
};

// A change of the kinds of news switched on: some of the three, each a boolean, nothing else
const kindsChange: Decoder<Partial<Kinds>> = (v) => {
  if (!v || typeof v !== 'object' || Array.isArray(v)) return undefined;
  const ok = Object.entries(v).every(([k, on]) => (KINDS as readonly string[]).includes(k) && typeof on === 'boolean');
  return ok ? { value: v as Partial<Kinds> } : undefined;
};

export function notifying(trusted: Trusted, window: () => BrowserWindow | null, bringUp: () => BrowserWindow) {
  current = window;
  show = bringUp;
  const kept = settings();
  on = kept.notifications !== false;
  kinds = Object.fromEntries(KINDS.map((k) => [k, kept[SWITCH[k]] !== false])) as Kinds;
  said = keptSaid();
  powerMonitor.on('lock-screen', () => { locked = true; });
  powerMonitor.on('unlock-screen', () => { locked = false; });
  const channels = ipc(trusted);
  channels.on('showing', [place], (_e, p) => {
    showing = p;
    windowSeen();
  });
  channels.handle('opened', [], () => {
    const place = opening;
    opening = null;
    return place;
  }, null);
  channels.handle('notifications', [orUndefined(boolean)], (_e, turn) => {
    if (turn !== undefined && turn !== on) {
      on = turn;
      writeSettings({ notifications: on });
      if (!on) [...shown.keys()].forEach(withdraw);
    }
    return on;
  }, null);
  // Settings › Notifications' three switches (ALERTS-15, 18): a kind turned on alerts only what
  // comes after its first list
  channels.handle('notification-kinds', [optional(kindsChange)], (_e, change) => {
    const turned = KINDS.filter((k) => change?.[k] !== undefined && change[k] !== kinds[k]);
    for (const k of turned) {
      kinds[k] = !kinds[k];
      if (kinds[k]) turnedOn.add(k); else turnedOn.delete(k);
    }
    if (turned.length) {
      writeSettings(Object.fromEntries(turned.map((k) => [SWITCH[k], kinds[k]])));
      void refresh();
    }
    return { ...kinds };
  }, null);
  channels.on('open-notification-settings', [], () => { void openSystemSettings(); });
  onLive(heardLive);
}
