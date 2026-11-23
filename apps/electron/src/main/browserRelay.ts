import { isAbsolute, relative, resolve } from 'node:path';
import { appPage, pageOk } from './pageRule';

// The relay between obid's agents and the page in the window (docs/plans/browser-t3.md, as T3 Code
// does it: packages/contracts/src/desktopBrowser.ts). obid sends page-level CDP for a tab on its
// relay stream, GET /v1/browser/relay (`cdp`, `release`, `pointer`, `turn`, and `box`, a foreign job's door for its box
// partition, main's alone; no ids, no replay); main runs each command on that tab's
// `<webview>` debugger and sends the replies and the page's events back, in order, with what
// happened to the tab (`attached`, `detached`, `loadFailed`, `shown`), on POST /v1/browser/relay.
// No Electron here: main/browserGuest.ts hands each webview in as a Guest, so this is all tested
// with a fake one (browserRelay.test.ts).

export type TabKey = { threadId: string; tabId: string };

// The partitions obid names for its agents' tabs, as the window opens them on the computer in use
// (shared with the page, which mounts only those)
import { partitionOk, windowPartition } from '../shared/browser-own';
export { PARTITION, partitionOk } from '../shared/browser-own';

/**
 * The partition the window opens for each agent tab, from the one obid named in its `tab` events on
 * /v1/browser/events, on the computer in use (windowPartition): the only one that tab's webview may
 * use. The first one named stands: a later `tab` event naming another for the same tab is refused,
 * as is one the window opens none for (a box on another computer), and the tab's `tabClosed`
 * forgets it, as `drop` does.
 */
export function namedPartitions(computer: () => string | null) {
  const named = new Map<string, string>();
  const at = (k: TabKey) => `${k.threadId}\0${k.tabId}`;
  // the tab forgotten: its partition when no other tab names it (a box's last tab), else null
  const dropAt = (a: string): string | null => {
    const p = named.get(a);
    if (p === undefined) return null;
    named.delete(a);
    return [...named.values()].includes(p) ? null : p;
  };
  return {
    /**
     * an events-stream message: 'kept', 'refused' (another partition than the first), or null (not
     * one); `on.kept` hears the partition a kept tab names, `on.last` one no tab names any more
     */
    heard(name: string, data: string, on: { kept?: (partition: string) => void; last?: (partition: string) => void } = {}): 'kept' | 'refused' | null {
      let e: { tab?: { id?: unknown; owner?: { kind?: unknown; job?: unknown }; partition?: unknown }; tabId?: unknown };
      try { e = JSON.parse(data); } catch { return null; }
      if (name === 'tabClosed' && typeof e.tabId === 'string') {
        for (const k of [...named.keys()]) {
          if (!k.endsWith(`\0${e.tabId}`)) continue;
          const last = dropAt(k);
          if (last) on.last?.(last);
        }
        return null;
      }
      const t = e.tab;
      if (name !== 'tab' || !t || t.owner?.kind !== 'agent' || typeof t.owner.job !== 'string' || typeof t.id !== 'string' || t.partition === undefined) return null;
      const k = at({ threadId: t.owner.job, tabId: t.id });
      const partition = windowPartition(t.partition, computer());
      if (partition === null) return 'refused';
      const was = named.get(k);
      if (was !== undefined && was !== partition) return 'refused';
      named.set(k, partition);
      on.kept?.(partition);
      return 'kept';
    },
    of: (k: TabKey) => named.get(at(k)),
    /** obid released the tab: forgotten, and its partition when no other tab names it, else null */
    drop: (k: TabKey) => dropAt(at(k)),
  };
}

/** a foreign job's browser door as obid hands it: `http://oby:<token>@127.0.0.1:<port>` */
export const BOX_PROXY = /^http:\/\/oby:([0-9a-f]{64})@127\.0\.0\.1:([1-9][0-9]{0,4})$/;
export type RelayEvent = TabKey & (
  | { type: 'attached' }
  | { type: 'detached' }
  | { type: 'cdp'; message: string }
  | { type: 'loadFailed'; url: string; code: number; description: string }
  | { type: 'shown'; shown: boolean });
export type RelayCommand =
  | (TabKey & { type: 'cdp'; message: string; agentCode?: true })
  | (TabKey & { type: 'release' })
  | (TabKey & { type: 'pointer'; phase: 'move' | 'click'; x: number; y: number })
  | { type: 'turn'; threadId: string; active: boolean }
  | { type: 'box'; threadId: string; partition: string; proxy: string };
export type BoxCommand = Extract<RelayCommand, { type: 'box' }>;

// One webview, as the relay uses it: its debugger, and the few things CDP can't do on a guest.
export interface Guest {
  /** debugger.sendCommand: the result, or a thrown error with CDP's words */
  send(method: string, params: Record<string, unknown> | undefined): Promise<unknown>;
  reload(): void;
  stop(): void;
  /** whether a frame of the guest has focus: if not, keys and text go in as the guest's own input */
  focused(): boolean;
  insertText(text: string): Promise<void>;
  key(params: Record<string, unknown>): void;
  /** where its downloads go, as obid named it (or nowhere, null) */
  downloadsTo(folder: string | null): void;
  /** the values of the page's password fields now, read by main itself and never relayed */
  passwordFields(): Promise<string[]>;
  /** whether a CDP object of the page is a password input, asked by main itself */
  isPassword(objectId: string): Promise<boolean>;
  /** whether a field main filled shows what it holds now (out of the page, or no longer a password input), asked by main itself */
  shows(objectId: string): Promise<boolean>;
}

// A password never reaches an agent (docs/contracts/browser-app.md, BROWSERAPP-12 and 44): what main
// relays to obid never carries one the owner filled, nor the value of a password field on the page,
// as it is or simply encoded; and after a fill, until the page's main frame leaves that document,
// main runs none of the agent's own script there (obid marks it `agentCode`), no function on a
// password input, and no `wait_for` selector probe, since any of them could hand on the value; it
// takes no picture of the page, and passes no read of it once a field it filled shows its value.
export const NOT_PASSED = 'Oby doesn’t pass a password the owner filled.';
export const NO_SCRIPT = 'Oby runs no script of the agent’s on a page the owner filled a password into, until it leaves that page.';
// obid's `wait_for` selector probe (core/crates/obid/src/browser/window/page.rs, `["wait", …]`):
// the agent picks the selector, so its answer could tell the value a character at a time
const SELECTOR_PROBE = '!!document.querySelector(';
/** a password shorter than this isn't looked for: a few characters would match almost anything */
export const SHORTEST = 4;
/** a value as it is and in the encodings a page or a tool simply turns it into */
export function forms(value: string): string[] {
  if ([...value].length < SHORTEST) return [];
  const bytes = Buffer.from(value, 'utf8');
  const hex = bytes.toString('hex');
  return [...new Set([value, JSON.stringify(value).slice(1, -1), encodeURIComponent(value), escape(value), bytes.toString('base64'), bytes.toString('base64url'), hex, hex.toUpperCase()])];
}
/** whether a relayed message's text carries any form of these values */
export const carries = (text: string, values: string[]) => values.some((v) => forms(v).some((f) => text.includes(f)));

// The CDP methods obid sends (T3 fakes a browser for Playwright; obid speaks to the page itself),
// plus main's own Emulation.setFocusEmulationEnabled: any other method, or a command for a session,
// never reaches the debugger. BROWSERAPP-29 keeps it in step with obid's sources.
export const METHODS: ReadonlySet<string> = new Set([
  'Page.enable', 'Page.setDownloadBehavior', 'Page.navigate', 'Page.handleJavaScriptDialog', 'Page.captureScreenshot',
  'Page.getNavigationHistory', 'Page.navigateToHistoryEntry', 'Page.reload',
  'Runtime.evaluate', 'Runtime.callFunctionOn',
  'DOM.scrollIntoViewIfNeeded', 'DOM.getContentQuads', 'DOM.resolveNode', 'DOM.focus', 'DOM.setFileInputFiles',
  'Input.dispatchMouseEvent', 'Input.dispatchKeyEvent', 'Input.insertText',
  'Accessibility.getFullAXTree',
  'Emulation.setDeviceMetricsOverride', 'Emulation.setEmulatedMedia', 'Emulation.setFocusEmulationEnabled',
]);
/** a message bigger than this ends the tab's relay, as T3's does (CdpRelay.ts, 64 MB) */
export const MAX_MESSAGE = 64 * 1024 * 1024;
/** how long main waits before sending a relay again after obid didn't take it */
export const AGAIN_MS = 1_000;
/**
 * How long a command for a tab whose webview hasn't attached yet in this window waits for its first
 * attach (BROWSERAPP-49); they run one at a time, in order, once it attaches, else they're answered
 * "isn't attached". A tab that was attached and detached answers at once: obid has already failed
 * what it had sent it, and running it after would do an act twice if the agent tried again.
 */
export const ATTACH_WAIT_MS = 10_000;
/** the most commands kept for one tab while it waits; the oldest are answered first */
export const WAITING_MAX = 256;
/** the most bytes (UTF-8) of commands kept for one tab while it waits; the oldest are answered first */
export const WAITING_BYTES = 4 * 1024 * 1024;
/** the most tabs whose commands wait at once; a command for one more is answered at once */
export const WAITING_TABS = 64;
/** the most events kept while obid doesn't answer; the oldest go first */
export const KEPT = 10_000;

const key = (k: TabKey) => `${k.threadId}\0${k.tabId}`;
type Reply = { id: number; result?: unknown; error?: { code: number; message: string } };

/** whether `path` is a folder inside obid's own downloads folder (`<home>/browser/downloads/`) */
export function insideDownloads(home: string, path: unknown): path is string {
  if (typeof path !== 'string' || !isAbsolute(path)) return false;
  const rel = relative(resolve(home, 'browser', 'downloads'), resolve(path));
  return rel !== '' && !rel.startsWith('..') && !isAbsolute(rel);
}

type Tab = { k: TabKey; guest: Guest; attached: boolean; shown: boolean | null; failed: RelayEvent | null; mouse: Promise<unknown>; queue: Promise<unknown>; broken: boolean; drain?: Drain | null };
/** what waited for a tab's first attach, running one at a time: what's left, and whether it was called off */
type Drain = { left: Extract<RelayCommand, { type: 'cdp' }>[]; off: boolean };
const size = (c: { message: string }) => Buffer.byteLength(c.message, 'utf8');

/** a page's address with none of these values in it: no query or fragment, and no path if that holds one */
function bare(url: string, values: string[]): string {
  try {
    const u = new URL(url);
    return carries(u.origin + u.pathname, values) ? u.origin : u.origin + u.pathname;
  } catch { return ''; }
}

export function relayHost(post: (events: RelayEvent[]) => Promise<boolean>, options: { home: string | null; pointer?: (c: Extract<RelayCommand, { type: 'pointer' }>) => void; box?: (c: BoxCommand) => void; released?: (k: TabKey) => void; wait?: (ms: number) => Promise<void> }) {
  const tabs = new Map<string, Tab>();
  // what was said of a tab before it attached (a first load's failure, whether it shows): sent on attach
  const early = new Map<string, { failed: RelayEvent | null; shown: boolean | null }>();
  const turns = new Set<string>();
  // the passwords main filled into each tab, kept in memory for the tab's life and never logged
  const filledIn = new Map<string, string[]>();
  // the tabs whose document holds a password main filled, until its main frame leaves (outlives a detach)
  const noScript = new Set<string>();
  // the tabs Fill in is working on now, across the reload it makes
  const filling = new Set<string>();
  // the fields main filled in each tab's document (CDP objects), until its main frame leaves
  const fields = new Map<string, string[]>();
  // obid's commands for each tab not attached yet, in order, and the timer that gives up on them
  const waiting = new Map<string, { commands: Extract<RelayCommand, { type: 'cdp' }>[]; bytes: number; timer: ReturnType<typeof setTimeout> }>();
  // the tabs attached at some point in this window: one detached now answers at once
  const everAttached = new Set<string>();
  const guarded = (k: TabKey) => noScript.has(key(k)) || filling.has(key(k));
  let outbox: RelayEvent[] = [];
  let sending = false;
  const wait = options.wait ?? ((ms: number) => new Promise<void>((done) => setTimeout(done, ms)));

  // one request in flight, in order; what obid didn't take is sent again
  async function flush() {
    if (sending) return;
    sending = true;
    try {
      while (outbox.length) {
        const batch = outbox;
        outbox = [];
        if (!(await post(batch))) {
          outbox = [...batch, ...outbox].slice(-KEPT);
          await wait(AGAIN_MS);
        }
      }
    } finally {
      sending = false;
    }
  }
  const emit = (e: RelayEvent) => { outbox.push(e); if (outbox.length > KEPT) outbox = outbox.slice(-KEPT); void flush(); };
  const cdp = (t: Tab, message: unknown) => {
    const text = JSON.stringify(message);
    if (text.length > MAX_MESSAGE) {
      // too big to carry: the tab's relay ends, as T3's does, until it's attached again
      t.broken = true;
      if (t.attached) { t.attached = false; emit({ ...t.k, type: 'detached' }); }
      return;
    }
    emit({ ...t.k, type: 'cdp', message: text });
  };
  const reply = (t: Tab, r: Reply) => cdp(t, r);
  // a command that can't reach its tab: answered so, by its id
  const unattached = (c: Extract<RelayCommand, { type: 'cdp' }>) => {
    let id: unknown;
    try { id = JSON.parse(c.message).id; } catch { return; }
    if (typeof id === 'number') emit({ threadId: c.threadId, tabId: c.tabId, type: 'cdp', message: JSON.stringify({ id, error: { code: -32000, message: 'This tab isn’t attached in the window.' } }) });
  };
  // the commands waiting for a tab, all answered "isn't attached" (it didn't attach in time, or it's gone)
  const giveUp = (k: string) => {
    const w = waiting.get(k);
    if (!w) return;
    clearTimeout(w.timer);
    waiting.delete(k);
    w.commands.forEach(unattached);
  };
  // a command for a tab not attached yet waits for its first attach, ATTACH_WAIT_MS at most, within
  // WAITING_MAX commands and WAITING_BYTES for the tab and WAITING_TABS tabs (BROWSERAPP-49)
  const hold = (c: Extract<RelayCommand, { type: 'cdp' }>) => {
    const k = key(c);
    if (size(c) > WAITING_BYTES || (!waiting.has(k) && waiting.size >= WAITING_TABS)) { unattached(c); return; }
    const w = waiting.get(k) ?? { commands: [], bytes: 0, timer: setTimeout(() => giveUp(k), ATTACH_WAIT_MS) };
    waiting.set(k, w);
    w.commands.push(c);
    w.bytes += size(c);
    while (w.commands.length > WAITING_MAX || w.bytes > WAITING_BYTES) {
      const old = w.commands.shift()!;
      w.bytes -= size(old);
      unattached(old);
    }
  };
  const refuse = (t: Tab, id: number, message: string) => reply(t, { id, error: { code: -32601, message } });

  // one command of obid's on its tab, as T3's relay and GuestInput.ts run it
  async function run(t: Tab, text: string, agentCode: boolean) {
    let m: { id?: unknown; method?: unknown; params?: unknown; sessionId?: unknown };
    try { m = JSON.parse(text); } catch { return; }
    if (typeof m.id !== 'number' || typeof m.method !== 'string') return;
    const { id, method } = m as { id: number; method: string };
    const params = m.params && typeof m.params === 'object' ? m.params as Record<string, unknown> : undefined;
    if (m.sessionId !== undefined) return refuse(t, id, 'Oby relays the page itself, never a session of it.');
    if (!METHODS.has(method)) return refuse(t, id, `Oby relays only the CDP methods obid sends: ${method} isn't one of them.`);
    if (method === 'Page.navigate' && (!pageOk(params?.url) || appPage(params?.url))) return refuse(t, id, 'Oby loads only web pages, never its own: Page.navigate needs an http(s) address or about:blank.');
    if (guarded(t.k)) {
      // a picture of the page could show a field a page's Show password turned to text
      if (method === 'Page.captureScreenshot') return refuse(t, id, NOT_PASSED);
      if (agentCode) return refuse(t, id, NO_SCRIPT);
      if (method === 'Runtime.evaluate' && String(params?.expression ?? '').startsWith(SELECTOR_PROBE)) return refuse(t, id, NO_SCRIPT);
      if (method === 'Runtime.callFunctionOn' && typeof params?.objectId === 'string' && await t.guest.isPassword(params.objectId)) return refuse(t, id, NO_SCRIPT);
    }
    try {
      let result: unknown = {};
      if (method === 'Page.reload') t.guest.reload();
      else if (method === 'Page.setDownloadBehavior') {
        // downloads go where obid says, inside its own folder, and nowhere else; with no folder of
        // this Mac's obid (another computer in use: the folder obid names is that computer's),
        // nothing a page downloads is kept (BROWSERAPP-51)
        if (options.home === null) t.guest.downloadsTo(null);
        else if (params?.behavior === 'allow' && insideDownloads(options.home, params.downloadPath)) t.guest.downloadsTo(resolve(params.downloadPath as string));
        else if (params?.behavior === 'deny') t.guest.downloadsTo(null);
        else return refuse(t, id, 'Downloads go only into a folder inside obid’s own browser/downloads.');
      } else if (method === 'Input.insertText' && !t.guest.focused()) await t.guest.insertText(String(params?.text ?? ''));
      else if (method === 'Input.dispatchKeyEvent' && !t.guest.focused()) t.guest.key(params ?? {});
      else if (method === 'Input.dispatchMouseEvent') {
        // one press at a time on a guest, in the order obid sent them
        const done = t.mouse.catch(() => undefined).then(() => t.guest.send(method, params));
        t.mouse = done;
        result = await done;
      } else result = await t.guest.send(method, params);
      // nothing of a password leaves for obid: those main filled, and those on the page now
      const said = JSON.stringify(result ?? {});
      const reads = /^(Runtime|DOM|Accessibility)\./.test(method);
      const shown = reads && guarded(t.k) && (await Promise.all((fields.get(key(t.k)) ?? []).map((f) => t.guest.shows(f)))).some(Boolean);
      if (shown || carries(said, filledIn.get(key(t.k)) ?? []) || (reads && carries(said, await t.guest.passwordFields().catch(() => [])))) {
        return reply(t, { id, error: { code: -32000, message: NOT_PASSED } });
      }
      reply(t, { id, result: result ?? {} });
    } catch (err) {
      reply(t, { id, error: { code: -32000, message: err instanceof Error ? err.message : String(err) } });
    }
  }

  // a drain called off (its tab detached, or obid's relay connected again): what it hasn't run yet is
  // answered "isn't attached", never run; the one running now can't be called back
  const callOff = (t: Tab) => {
    const d = t.drain;
    if (!d) return;
    d.off = true;
    t.drain = null;
    d.left.splice(0).forEach(unattached);
  };
  // one command on an attached tab: while guarded, the tab's commands run one at a time, so no act of
  // the agent's runs between a read and main's check of what it read; a page dialog's answer never
  // waits behind a read it holds up. Settles once it has run.
  function dispatch(t: Tab, c: Extract<RelayCommand, { type: 'cdp' }>): Promise<void> {
    if (!t.attached) { unattached(c); return Promise.resolve(); }
    let method: unknown;
    try { method = JSON.parse(c.message).method; } catch { /* run refuses it */ }
    const go = () => run(t, c.message, c.agentCode === true);
    const one = guarded(t.k) && method !== 'Page.handleJavaScriptDialog' ? t.queue.then(go) : go();
    t.queue = Promise.all([t.queue, one]).then(() => undefined, () => undefined);
    return one.then(() => undefined, () => undefined);
  }

  const self = {
    /** a webview whose debugger is attached and whose first page settled: obid may drive it */
    attach(k: TabKey, guest: Guest) {
      const before = early.get(key(k));
      early.delete(key(k));
      const t: Tab = { k, guest, attached: true, shown: before?.shown ?? null, failed: before?.failed ?? null, mouse: Promise.resolve(), queue: Promise.resolve(), broken: false };
      tabs.set(key(k), t);
      emit({ ...k, type: 'attached' });
      if (t.shown !== null) emit({ ...k, type: 'shown', shown: t.shown });
      if (t.failed) emit(t.failed);
      t.failed = null;
      if (turns.has(k.threadId)) void guest.send('Emulation.setFocusEmulationEnabled', { enabled: true }).catch(() => undefined);
      everAttached.add(key(k));
      // what obid sent before it attached runs now, one at a time in the order it came; what comes
      // meanwhile runs after it
      const w = waiting.get(key(k));
      if (w) {
        clearTimeout(w.timer);
        waiting.delete(key(k));
        const d: Drain = { left: [...w.commands], off: false };
        t.drain = d;
        void (async () => {
          while (!d.off && d.left.length) await dispatch(t, d.left.shift()!);
          // nothing awaited between the last check and this: a command that came meanwhile is in `left`
          if (t.drain === d) t.drain = null;
        })();
      }
    },
    /** the webview went: closed, crashed, swapped, or DevTools took its debugger */
    detach(k: TabKey) {
      const t = tabs.get(key(k));
      if (!t) return;
      tabs.delete(key(k));
      // obid fails what it had sent this tab when it hears `detached`: nothing of it runs after, even
      // on a reattach within the debugger's 250 ms
      const was = t.attached;
      t.attached = false;
      callOff(t);
      // whether it shows outlives a detach (DevTools, a crash): it's said again when it attaches again
      early.set(key(k), { failed: null, shown: t.shown });
      if (was) emit({ ...k, type: 'detached' });
    },
    /** a CDP event of the page's: to obid as it came, unless it carries a password the owner filled */
    event(k: TabKey, method: string, params: unknown) {
      // the main frame left the document a password was filled into: the agent's script runs there again
      if (method === 'Page.frameNavigated' && !(params as { frame?: { parentId?: string } } | undefined)?.frame?.parentId) { noScript.delete(key(k)); fields.delete(key(k)); }
      const t = tabs.get(key(k));
      if (!t?.attached || t.broken || carries(JSON.stringify(params ?? {}), filledIn.get(key(k)) ?? [])) return;
      cdp(t, { method, params });
    },
    /** Fill in starts on this tab: guarded until it ends */
    fillStarts(k: TabKey) { filling.add(key(k)); },
    /** Fill in ended without filling: the guard it set is lifted */
    fillEnds(k: TabKey) { filling.delete(key(k)); },
    /** main fills a password into this tab's page (Fill in), into `field`: never relayed, and guarded until its main frame leaves */
    filled(k: TabKey, value: string, field?: string) {
      filledIn.set(key(k), [...(filledIn.get(key(k)) ?? []), value]);
      if (field) fields.set(key(k), [...(fields.get(key(k)) ?? []), field]);
      filling.delete(key(k));
      noScript.add(key(k));
    },
    /** the main frame didn't load; kept until the tab is attached, as T3 replays it */
    loadFailed(k: TabKey, url: string, code: number, description: string) {
      // a tab main filled a password into: its address goes bare (a GET form's query holds the field)
      const filled = filledIn.get(key(k)) ?? [];
      const e: RelayEvent = filled.length
        ? { ...k, type: 'loadFailed', url: bare(url, filled), code, description: carries(description, filled) ? '' : description }
        : { ...k, type: 'loadFailed', url, code, description };
      const t = tabs.get(key(k));
      if (t?.attached) emit(e);
      else early.set(key(k), { shown: early.get(key(k))?.shown ?? null, failed: e });
    },
    /** whether the webview is on screen in the window */
    shown(k: TabKey, shown: boolean) {
      const t = tabs.get(key(k));
      if (!t) { early.set(key(k), { failed: early.get(key(k))?.failed ?? null, shown }); return; }
      if (t.shown === shown) return;
      t.shown = shown;
      if (t.attached) emit({ ...k, type: 'shown', shown });
    },
    /** one of obid's commands from its relay stream */
    command(c: RelayCommand) {
      if (c.type === 'turn') {
        if (c.active) turns.add(c.threadId); else turns.delete(c.threadId);
        for (const t of tabs.values()) if (t.k.threadId === c.threadId && t.attached) void t.guest.send('Emulation.setFocusEmulationEnabled', { enabled: c.active }).catch(() => undefined);
        return;
      }
      // a box's door is main's alone: never posted, drawn or put where the page reads
      if (c.type === 'box') { options.box?.(c); return; }
      const t = tabs.get(key(c));
      if (c.type === 'pointer') { if (t?.attached) options.pointer?.(c); return; }
      // release drops only obid's use of the tab: its debugger stays, and the next command uses it
      // again; a box's last tab clears its storage (browser.ts)
      if (c.type === 'release') { if (t) t.mouse = Promise.resolve(); options.released?.({ threadId: c.threadId, tabId: c.tabId }); return; }
      // a tab not attached yet waits for its first attach; one that was (detached, or its relay ended
      // on a message too big) is answered at once
      if (!t?.attached) { if (everAttached.has(key(c))) unattached(c); else hold(c); return; }
      // behind what waited for its first attach, still running
      if (t.drain) { t.drain.left.push(c); return; }
      void dispatch(t, c);
    },
    /** obid's relay stream (re)connected: every live tab is announced again (T3's announceAll); no command runs again */
    reconnected() {
      // a restarted obid names its tabs anew, and the last stream's calls are failed: nothing it sent
      // waits for them, nor runs from a drain
      for (const k of [...waiting.keys()]) giveUp(k);
      for (const t of tabs.values()) callOff(t);
      for (const t of tabs.values()) {
        if (!t.attached) continue;
        emit({ ...t.k, type: 'attached' });
        if (t.shown !== null) emit({ ...t.k, type: 'shown', shown: t.shown });
      }
    },
    /** the webview is gone for good: nothing of it is kept */
    forget(k: TabKey) { giveUp(key(k)); everAttached.delete(key(k)); early.delete(key(k)); filledIn.delete(key(k)); noScript.delete(key(k)); filling.delete(key(k)); fields.delete(key(k)); },
    /** for tests and the window's close: what's attached */
    attached: () => [...tabs.values()].filter((t) => t.attached).map((t) => t.k),
    /** the events not yet taken by obid */
    pending: () => outbox.length + (sending ? 1 : 0),
  };
  return self;
}

export type RelayHost = ReturnType<typeof relayHost>;

/** obid's relay command from GET /v1/browser/relay (`event:` its type, `data:` its JSON), or null for another shape */
export function relayCommand(name: string, data: string): RelayCommand | null {
  if (!['cdp', 'release', 'pointer', 'turn', 'box'].includes(name)) return null;
  let e: Record<string, unknown>;
  try { e = JSON.parse(data); } catch { return null; }
  const str = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && v.length <= 256;
  if (e.type !== undefined && e.type !== name) return null;
  if (!str(e.threadId)) return null;
  if (name === 'turn') return typeof e.active === 'boolean' ? { type: 'turn', threadId: e.threadId, active: e.active } : null;
  if (name === 'box') {
    // only its own job's box, and only a door on loopback with a token of obid's shape
    if (e.partition !== `oby-box-${e.threadId}` || !partitionOk(e.partition, null)) return null;
    const door = typeof e.proxy === 'string' ? BOX_PROXY.exec(e.proxy) : null;
    return door && Number(door[2]) <= 65535 ? { type: 'box', threadId: e.threadId, partition: e.partition, proxy: e.proxy as string } : null;
  }
  if (!str(e.tabId)) return null;
  const k = { threadId: e.threadId, tabId: e.tabId };
  if (name === 'release') return { ...k, type: 'release' };
  if (name === 'cdp') {
    // the agent's own script, as obid marks it (BROWSER-33): `true` or left out, nothing else
    if (typeof e.message !== 'string' || (e.agentCode !== undefined && e.agentCode !== true)) return null;
    return e.agentCode ? { ...k, type: 'cdp', message: e.message, agentCode: true } : { ...k, type: 'cdp', message: e.message };
  }
  const n = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
  return (e.phase === 'move' || e.phase === 'click') && n(e.x) && n(e.y) ? { ...k, type: 'pointer', phase: e.phase, x: e.x, y: e.y } : null;
}
