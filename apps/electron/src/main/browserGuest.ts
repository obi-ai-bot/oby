import { writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { app, session, webContents, type BrowserWindow, type Session, type WebContents } from 'electron';
import { BOX_PROXY, partitionOk, type BoxCommand, type Guest, type RelayHost, type TabKey } from './browserRelay';
export { PARTITION, partitionOk } from './browserRelay';
import { appPage, pageOk } from './pageRule';
export { pageOk } from './pageRule';

// The owner's own tabs' partition (BROWSERAPP-45 to 48): theirs alone, never one obid names for an
// agent (`partitionOk` refuses it), never a box's. Its webviews are kept by the page's tab id under
// OWN_THREAD, in place of a job: never keyed to obid's tabs, never relayed, no debugger attached.
import { OWN_PARTITION, OWN_TAB, OWN_THREAD, ownPartitionOk, scratchPartition } from '../shared/browser-own';
export { OWN_PARTITION, OWN_THREAD, scratchPartition } from '../shared/browser-own';
/** how long the page's word that it mounts an owner's tab holds, unused (BROWSERAPP-45) */
export const MOUNT_MS = 10_000;
/** an address the owner's own tab may open: a page of the web, never the app's own (BROWSERAPP-28's rule) */
export const ownPageOk = (url: unknown): url is string => pageOk(url) && url !== 'about:blank' && !appPage(url);
export const NOT_A_WEB_PAGE = 'Oby opens only web pages.';

// Each agent tab's `<webview>` in the window, as T3 Code hosts them (apps/desktop/src/window/
// DesktopWindow.ts, preview/Manager.ts; docs/plans/browser-t3.md): hardened when it attaches, in
// the partition obid named for its tab and no other, its debugger attached ("1.3") and handed to
// the relay once its first page settled, and given back when DevTools takes it. A foreign job's box
// partition (`oby-box-<job>`) has its session made before its guest attaches, as T3's getSession
// makes a partition's (apps/desktop/src/preview/BrowserSession.ts:578-683): every request waits for
// its job's egress door, the proxy obid hands main alone (`box`), and the box is cleared with its
// last tab (BrowserSession.ts:689-740).

/** how long the first page may take to settle before the tab is announced anyway (T3: 5 s) */
export const SETTLE_MS = 5_000;
/** the permissions a page gets (T3's list, less geolocation and the loopback network): no others */
export const PERMISSIONS = new Set(['clipboard-sanitized-write', 'fullscreen', 'pointerLock']);
/** a box page's: never the owner's clipboard, which another Oby's task's acts could write */
export const BOX_PERMISSIONS = new Set(['fullscreen', 'pointerLock']);
/**
 * A box page's copy and cut never reach the owner's clipboard. An agent's click gives the page the
 * user activation `execCommand('copy')` needs, and Chromium then writes the selection, or what the
 * page's copy listener set, without asking any permission handler (clipboard_commands.cc). This runs
 * in a world of main's own in every frame, before the page's scripts, so the page can't reach or undo
 * it: it stops each copy, cut and their before-events ahead of every listener of the page's, and
 * drops the selection, so Chromium has nothing to write. A copy the owner makes with the keyboard or
 * the menu is stopped too (another Oby's page isn't theirs to copy from).
 */
export const BOX_CLIPBOARD_GUARD = `(() => { const stop = (e) => { e.stopImmediatePropagation(); const s = getSelection(); if (s) s.removeAllRanges(); }; for (const t of ['beforecopy', 'copy', 'beforecut', 'cut']) addEventListener(t, stop, true); })();`;
/** how long a box page's request waits for its job's door before it's cancelled */
export const BOX_HOLD_MS = 10_000;
/** how long a page may take to answer before main leaves it anyway, stuck in a script (T3: Manager.ts:139) */
export const BUSY_PAGE_PROBE_MS = 1_000;
/** the owner's Back, Forward, Reload and Stop loading on a tab's own page */
export type NavOp = 'back' | 'forward' | 'reload' | 'stop';
/** what main tells the page of a tab's own page: its address, is there one behind, one ahead, is it loading, and the address its last load failed at (null: none) */
export type NavState = { threadId: string; tabId: string; url: string; canGoBack: boolean; canGoForward: boolean; loading: boolean; failed: string | null };

// The webview's preferences, whatever the page asked for: sandboxed, no Node anywhere, no preload,
// its partition checked (T3: DesktopWindow.ts:620-645)
export function harden(prefs: Record<string, unknown>, params: { partition?: string; src?: string }): boolean {
  const owners = ownPartitionOk(params.partition);
  if (!(owners || partitionOk(params.partition)) || !pageOk(params.src)) return false;
  if (owners && !ownPageOk(params.src)) return false;
  delete prefs.preload;
  delete prefs.preloadURL;
  Object.assign(prefs, { sandbox: true, nodeIntegration: false, nodeIntegrationInSubFrames: false, contextIsolation: true, webSecurity: true, allowRunningInsecureContent: false, disableHtmlFullscreenWindowResize: true });
  return true;
}

/**
 * What Fill in does to a page before it types into it, the owner's choice of 2026-11-21 (D2(a),
 * BROWSERAPP-43): `reload`, the page loads again, so no script the agent ran in it is in the document
 * the password goes into (what the page held, a typed name say, is lost). `none`: the page as it is.
 */
export const FILL_PREP: 'reload' | 'none' = 'reload';
/** how long Fill in waits for the page to load again before it gives up */
export const RELOAD_MS = 15_000;
/** the values of the page's password fields, for main's own check of what it relays (never relayed) */
export const PASSWORDS = '[...document.querySelectorAll(\'input[type="password" i]\')].map((e) => e.value)';
const IS_PASSWORD = 'function () { return this instanceof HTMLInputElement && this.type.toLowerCase() === \'password\'; }';
// a field main filled still hides what it holds: in the page, and a password input (no Show password)
const HIDES = 'function () { return this.isConnected && this instanceof HTMLInputElement && this.type.toLowerCase() === \'password\'; }';
/** how many characters the focused field holds: Fill in reads back only that, never the value */
export const FIELD_LENGTH = '(() => { const el = document.activeElement; return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement ? [...el.value].length : -1; })()';
/**
 * The page's own JavaScript that puts the keyboard on the field an ask names (a CSS selector, the
 * agent's own): for a password only a password input, for a one-time code (`code`) a text input or
 * area too, in the page. The field itself once it has focus, else null. The selector goes in as a
 * string literal; the value never goes into any script (Fill in types it, Input.insertText).
 */
export const focusField = (field: string, code = false) =>
  `(() => { const el = document.querySelector(${JSON.stringify(field)}); if (!(el instanceof HTMLInputElement && (${code} || el.type.toLowerCase() === 'password')) && !(${code} && el instanceof HTMLTextAreaElement)) return null; if (!el.isConnected) return null; el.focus(); return document.activeElement === el ? el : null; })()`;
/** the parts of a tab's webview Fill in types through: its WebContents, or a fake one (browserFill.test.ts) */
export type FillPage = {
  debugger: { sendCommand(method: string, params?: object): Promise<unknown> };
  reload(): void;
  focus(): void;
  insertText(text: string): Promise<void>;
  once(event: 'did-finish-load', listener: () => void): unknown;
  off(event: 'did-finish-load', listener: () => void): unknown;
};

// the page loaded again, from the start (Fill in's reload): true once it has, false past RELOAD_MS
function reloaded(wc: FillPage): Promise<boolean> {
  return new Promise((done) => {
    const timer = setTimeout(() => { wc.off('did-finish-load', ok); done(false); }, RELOAD_MS);
    const ok = () => { clearTimeout(timer); done(true); };
    wc.once('did-finish-load', ok);
    wc.reload();
  });
}

/**
 * Fill in (BROWSERAPP-12 and 43): the value typed into the field of the tab's own page, through its
 * debugger, as keys typed there would; it goes to no script, no log and never to obid. Only into the
 * owner's own agents' tabs, never another Oby's box. Null once typed, else why not.
 */
export async function typeInto(wc: FillPage, k: TabKey, partition: string | undefined, host: Pick<RelayHost, 'fillStarts' | 'fillEnds' | 'filled'>, field: string, value: string, code = false): Promise<string | null> {
  if (!partition?.startsWith('persist:oby-agent-')) return 'This page isn’t open in the window.';
  // the agent's script doesn't run on the page from here: not while it reloads, nor once it's filled
  host.fillStarts(k);
  const read = async (expression: string) => ((await wc.debugger.sendCommand('Runtime.evaluate', { expression, returnByValue: true })) as { result?: { value?: unknown } } | undefined)?.result?.value;
  try {
    if (FILL_PREP === 'reload' && !(await reloaded(wc))) { host.fillEnds(k); return 'The page didn’t load again in time.'; }
    // the guest gets the keyboard first, as T3's Take control gives it (preview/Manager.ts:2482)
    wc.focus();
    const focused = ((await wc.debugger.sendCommand('Runtime.evaluate', { expression: focusField(field, code) })) as { result?: { objectId?: unknown } } | undefined)?.result?.objectId;
    if (typeof focused !== 'string') { host.fillEnds(k); return 'That field isn’t on the page.'; }
    // the relay knows the value and the field before a key of it is in the page: whatever the page
    // does with it while it's typed (an alert of it, say) never leaves for obid
    host.filled(k, value, focused);
    await wc.debugger.sendCommand('Input.insertText', { text: value });
    // only its length is read back, never the value: typed once more as the guest's own input if
    // CDP's didn't land (T3: preview/GuestInput.ts:199-200)
    if (await read(FIELD_LENGTH) === 0) await wc.insertText(value);
    return (await read(FIELD_LENGTH)) === [...value].length ? null : 'The page didn’t take what you typed.';
  } catch (err) {
    host.fillEnds(k);
    return `Oby couldn’t fill the page: ${err instanceof Error ? err.message : String(err)}`;
  }
}

const keyed = new Map<number, TabKey>();
const folders = new Map<number, string | null>();
const ready = new WeakSet<Session>();
const isBox = (partition: string) => partition.startsWith('oby-box-');
// each box session's partition, and each box's door once its session's proxy is set
const boxes = new WeakMap<Session, string>();
const doors = new Map<string, { port: number; token: string }>();
// each box's requests waiting for its door
const waiting = new Map<string, Set<() => void>>();
// how many times each box was cleared: a door still being set when it was isn't kept
const clears = new Map<string, number>();

// the guard as a file, for a box session's preload: written once a run, where only the owner's
// user writes (the app's own folder), never a box's
let guardFile: string | null = null;
function guardPreload(): string {
  if (!guardFile) {
    const file = join(app.getPath('userData'), 'box-clipboard-guard.js');
    writeFileSync(file, BOX_CLIPBOARD_GUARD, { mode: 0o600 });
    guardFile = file;
  }
  return guardFile;
}
// the boxes whose guard couldn't be set up (its file not written, say): every request of theirs is
// cancelled, so none of their pages loads unguarded
const unguarded = new Set<string>();

/**
 * One partition's session, set up once: its permissions and where its downloads go; for a box, every
 * request held until its job's door is set, cancelled after BOX_HOLD_MS without one (T3: getSession,
 * BrowserSession.ts:578-683, its handlers at 618-643). Run before its guest attaches.
 */
export function prepare(s: Session, partition: string) {
  if (ready.has(s)) return;
  ready.add(s);
  const box = isBox(partition);
  // a box's hold first, before anything that can fail, so a setup that fails holds or cancels
  if (box) {
    boxes.set(s, partition);
    s.webRequest.onBeforeRequest((_details, callback) => {
      if (unguarded.has(partition)) return callback({ cancel: true });
      if (doors.has(partition)) return callback({});
      const held = waiting.get(partition) ?? new Set<() => void>();
      waiting.set(partition, held);
      const go = () => { clearTimeout(timer); held.delete(go); callback(unguarded.has(partition) ? { cancel: true } : {}); };
      const timer = setTimeout(() => { held.delete(go); callback({ cancel: true }); }, BOX_HOLD_MS);
      held.add(go);
    });
  }
  const granted = box ? BOX_PERMISSIONS : PERMISSIONS;
  s.setPermissionRequestHandler((_wc, permission, done) => done(granted.has(permission)));
  s.setPermissionCheckHandler((_wc, permission) => granted.has(permission));
  s.on('will-download', (_e, item, wc) => {
    // an agent's download goes where obid said for its tab (Page.setDownloadBehavior), else nowhere
    const folder = wc ? folders.get(wc.id) : null;
    if (!folder) { item.cancel(); return; }
    item.setSavePath(join(folder, basename(item.getFilename()) || 'download'));
  });
  if (!box) return;
  // the clipboard guard as the session's preload: in every frame, in its own world, before the page's
  // first script, its first document's too (the debugger's guard comes only once main attaches it).
  // One that can't be set up leaves the box reaching nothing, its door still set (boxDoor).
  try {
    s.registerPreloadScript({ type: 'frame', id: 'oby-box-clipboard-guard', filePath: guardPreload() });
  } catch {
    unguarded.add(partition);
  }
}

/** obid's `box`: the box's session made ready, its proxy set to the job's door, then its held requests let go */
export async function boxDoor(c: BoxCommand) {
  const door = BOX_PROXY.exec(c.proxy);
  if (!door) return;
  const s = session.fromPartition(c.partition);
  prepare(s, c.partition);
  const port = Number(door[2]);
  const cleared = clears.get(c.partition) ?? 0;
  await s.setProxy({ mode: 'fixed_servers', proxyRules: `127.0.0.1:${port}`, proxyBypassRules: '<-loopback>' });
  // the box was cleared meanwhile: its requests wait for the door obid sends again
  if ((clears.get(c.partition) ?? 0) !== cleared) return;
  doors.set(c.partition, { port, token: door[1] });
  for (const go of [...(waiting.get(c.partition) ?? [])]) go();
}

/**
 * What main does with a partition as obid's tabs name it and let it go (`drop`, namedPartitions'):
 * a box's session is made ready when a tab names it, and cleared once none does
 */
export function boxing(drop: (k: TabKey) => string | null) {
  const last = (partition: string) => { void clearBox(partition).catch(() => undefined); };
  return {
    kept: (partition: string) => { if (isBox(partition)) prepare(session.fromPartition(partition), partition); },
    last,
    released: (k: TabKey) => { const was = drop(k); if (was) last(was); },
  };
}

/** a box's last tab went: its storage, cache and logins cleared (T3: BrowserSession.ts:689-740), then its door forgotten */
/** a server tab's throwaway partition, its page gone: its storage, cache and logins cleared (BROWSERAPP-47) */
export async function clearScratch(partition: string) {
  const s = session.fromPartition(partition);
  await Promise.all([
    s.clearStorageData({ storages: ['cookies', 'localstorage', 'indexdb', 'serviceworkers', 'cachestorage'] }),
    s.clearCache(),
    s.clearAuthCache(),
  ]);
}

export async function clearBox(partition: string) {
  if (!isBox(partition)) return;
  const s = session.fromPartition(partition);
  const cleared = Promise.all([
    s.clearStorageData({ storages: ['cookies', 'localstorage', 'indexdb', 'serviceworkers', 'cachestorage'] }),
    s.clearCache(),
    s.clearAuthCache(),
  ]);
  doors.delete(partition);
  clears.set(partition, (clears.get(partition) ?? 0) + 1);
  await cleared;
}

/**
 * The clipboard guard on one debugger session (the page's, or a frame's or worker's of it): in every
 * frame now and every document after, and in every frame or worker the page starts, held by Chromium
 * until it's guarded (`waitForDebuggerOnStart`), so a cross-site frame gets it too
 */
async function guardClipboard(d: Electron.Debugger, sessionId?: string) {
  try {
    await d.sendCommand('Page.enable', {}, sessionId);
    await d.sendCommand('Page.addScriptToEvaluateOnNewDocument', { source: BOX_CLIPBOARD_GUARD, worldName: 'oby-box-guard', runImmediately: true }, sessionId);
  } catch { /* a worker has no page, and no clipboard command */ }
  await d.sendCommand('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }, sessionId).catch(() => undefined);
  if (sessionId) await d.sendCommand('Runtime.runIfWaitingForDebugger', {}, sessionId).catch(() => undefined);
}

function asGuest(wc: WebContents): Guest {
  return {
    send: (method, params) => wc.debugger.sendCommand(method, params),
    reload: () => wc.reload(),
    stop: () => wc.stop(),
    focused: () => wc.isFocused(),
    insertText: (text) => wc.insertText(text),
    key: (p) => {
      const type = p.type === 'keyUp' ? 'keyUp' : p.type === 'char' ? 'char' : 'keyDown';
      const keyCode = String(p.key ?? p.text ?? '');
      if (keyCode) wc.sendInputEvent({ type, keyCode });
    },
    downloadsTo: (folder) => folders.set(wc.id, folder),
    // main's own reads, through the same debugger, never relayed: what the page's password fields hold
    passwordFields: async () => {
      const r = await wc.debugger.sendCommand('Runtime.evaluate', { expression: PASSWORDS, returnByValue: true }) as { result?: { value?: unknown } };
      return Array.isArray(r?.result?.value) ? r.result.value.filter((v): v is string => typeof v === 'string' && v.length > 0) : [];
    },
    shows: async (objectId) => {
      try {
        const r = await wc.debugger.sendCommand('Runtime.callFunctionOn', { objectId, functionDeclaration: HIDES, returnByValue: true }) as { result?: { value?: unknown } };
        return r?.result?.value !== true;
      } catch { return true; }
    },
    isPassword: async (objectId) => {
      try {
        const r = await wc.debugger.sendCommand('Runtime.callFunctionOn', { objectId, functionDeclaration: IS_PASSWORD, returnByValue: true }) as { result?: { value?: unknown } };
        return r?.result?.value === true;
      } catch { return false; }
    },
  };
}

// Before Back, Forward or Reload: a page stuck in a script never commits a navigation, so main asks
// it something with its own debugger and, with no answer within BUSY_PAGE_PROBE_MS, ends the script,
// not waiting for that either; with no debugger (DevTools have it) it acts at once (T3: Manager.ts:2077-2093)
async function leaveBusyPage(wc: WebContents) {
  if (!wc.debugger.isAttached()) return;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const answered = await Promise.race([
    wc.debugger.sendCommand('Runtime.evaluate', { expression: '0' }).then(() => true, () => true),
    new Promise<false>((done) => { timer = setTimeout(() => done(false), BUSY_PAGE_PROBE_MS); }),
  ]);
  clearTimeout(timer);
  if (!answered) wc.debugger.sendCommand('Runtime.terminateExecution').catch(() => undefined);
}

/**
 * The window's webviews for the relay: `register` ties one the page mounted to obid's tab (the page
 * says which; main checks the webview is this window's and its partition is the one obid named),
 * `shown` passes on whether it's on screen, `nav` moves a tab's own page on the owner's click, and
 * `report` hears each tab's page state.
 */
export function hosting(win: BrowserWindow, host: RelayHost, named: (k: TabKey) => string | undefined, report: (s: NavState) => void) {
  // the address each webview's last main-frame load failed at, or its page crashed at, until a load
  // starts again: did-stop-loading after did-fail-load proves no page (T3: Manager.ts:1226-1231)
  const failedLoad = new Map<number, string>();
  // the owner's own tabs' webviews, by webContents id: the page's tab id for each (BROWSERAPP-45)
  const owned = new Map<number, string>();
  // the owner's own tabs the page said it mounts, unused yet (one webview each, MOUNT_MS at most), and
  // the partition each mounted in; the owner's partitions' sessions, for their pages' stricter rule
  const mounting = new Map<string, { partition: string; src: string; until: number }>();
  const mountedIn = new Map<string, string>();
  const ownerSessions = new WeakSet<Session>();
  // a webview may attach in an owner's partition only as one the page said it mounts, at that address
  const mounts = (partition: string, src: string): boolean => {
    const now = Date.now();
    for (const [tabId, m] of mounting) {
      if (m.until < now) { mounting.delete(tabId); continue; }
      if (m.partition !== partition || m.src !== src) continue;
      mounting.delete(tabId);
      mountedIn.set(tabId, partition);
      ownerSessions.add(session.fromPartition(partition));
      return true;
    }
    return false;
  };
  const keyOf = (id: number): TabKey | undefined => keyed.get(id) ?? (owned.has(id) ? { threadId: OWN_THREAD, tabId: owned.get(id)! } : undefined);
  // this window's webview for a tab: obid's by its job and tab, the owner's own under OWN_THREAD
  const pageOf = (k: TabKey): WebContents | undefined => {
    const ids = k.threadId === OWN_THREAD
      ? [...owned].filter(([, tabId]) => tabId === k.tabId).map(([id]) => id)
      : [...keyed].filter(([, t]) => t.threadId === k.threadId && t.tabId === k.tabId).map(([id]) => id);
    return ids.map((id) => webContents.fromId(id)).filter((w): w is WebContents => !!w && !w.isDestroyed() && w.hostWebContents === win.webContents).at(-1);
  };
  // a keyed tab's page state, from its own webContents (T3: Manager.ts:1185-1192, 1214-1257): its
  // address too, as obid's tab keeps the address of obid's own last command
  const told = (wc: WebContents) => {
    const k = keyOf(wc.id);
    if (!k || wc.isDestroyed()) return;
    report({ threadId: k.threadId, tabId: k.tabId, url: wc.getURL(), canGoBack: wc.navigationHistory.canGoBack(), canGoForward: wc.navigationHistory.canGoForward(), loading: wc.isLoadingMainFrame(), failed: failedLoad.get(wc.id) ?? null });
  };
  win.webContents.on('will-attach-webview', (e, prefs, params) => {
    if (!harden(prefs as Record<string, unknown>, params)) { e.preventDefault(); return; }
    // an owner's partition only for an owner's own tab the page said it mounts: never one obid named (BROWSERAPP-24)
    if (ownPartitionOk(params.partition) && !mounts(params.partition, params.src!)) { e.preventDefault(); return; }
    prepare(session.fromPartition(params.partition!), params.partition!);
    // a box page's frames each run its session's preload (the clipboard guard), not only its top
    // one: still sandboxed, so no Node in any of them
    if (isBox(params.partition!)) (prefs as Record<string, unknown>).nodeIntegrationInSubFrames = true;
  });
  win.webContents.on('did-attach-webview', (_e, wc) => {
    const box = boxes.get(wc.session);
    if (box) {
      // a box page's connections go only through its door: no WebRTC beside the proxy
      wc.setWebRTCIPHandlingPolicy('disable_non_proxied_udp');
      // the door's own login, only to that door; any other login a box page meets gets none
      wc.on('login', (ev, _details, authInfo, callback) => {
        ev.preventDefault();
        const door = doors.get(box);
        if (door && authInfo.isProxy && authInfo.host === '127.0.0.1' && authInfo.port === door.port) callback('oby', door.token);
        else callback();
      });
      // no TLS identity of the owner's to another Oby's page: it goes on with no client certificate
      // (the same callback as app's, whose typings say what Electron does: none goes on without one)
      wc.on('select-client-certificate', (ev, _url, _list, callback) => { ev.preventDefault(); (callback as () => void)(); });
    }
    // a page of the web stays on the web: no window of its own, no file:, javascript: or data:; an
    // owner's tab's never reaches the app's own page either, by a link, a new window or a redirect (BROWSERAPP-46)
    const ok = ownerSessions.has(wc.session) ? ownPageOk : pageOk;
    wc.setWindowOpenHandler(({ url }) => { if (ok(url)) void wc.loadURL(url); return { action: 'deny' }; });
    wc.on('will-navigate', (ev, url) => { if (!ok(url)) ev.preventDefault(); });
    if (ok === ownPageOk) wc.on('will-redirect', (ev, url) => { if (!ownPageOk(url)) ev.preventDefault(); });
    wc.on('did-fail-load', (_ev, code, description, url, isMainFrame) => {
      if (!isMainFrame || code === -3) return;
      failedLoad.set(wc.id, url);
      const k = keyed.get(wc.id);
      if (k) host.loadFailed(k, url, code, description);
    });
    // the page state, each time it may have changed (T3: Manager.ts:1662-1670)
    const tell = () => told(wc);
    wc.on('did-navigate', tell);
    wc.on('did-navigate-in-page', tell);
    wc.on('did-start-loading', () => { failedLoad.delete(wc.id); tell(); });
    wc.on('did-stop-loading', tell);
    wc.on('did-fail-load', tell);
    wc.on('render-process-gone', () => { failedLoad.set(wc.id, wc.getURL()); tell(); const k = keyed.get(wc.id); if (k) host.detach(k); });
    wc.once('destroyed', () => {
      const k = keyed.get(wc.id);
      keyed.delete(wc.id);
      // a server tab's throwaway partition goes with its page: nothing of it is kept (BROWSERAPP-47)
      const mine = owned.get(wc.id);
      const was = mine === undefined ? undefined : mountedIn.get(mine);
      if (mine !== undefined) mountedIn.delete(mine);
      if (was && was !== OWN_PARTITION) void clearScratch(was).catch(() => undefined);
      owned.delete(wc.id);
      folders.delete(wc.id);
      failedLoad.delete(wc.id);
      if (k) { host.detach(k); host.forget(k); }
    });
    // DevTools takes the debugger: the tab is detached until they close (T3: Manager.ts:2252-2267)
    wc.on('devtools-opened', () => { if (wc.debugger.isAttached()) wc.debugger.detach(); });
    wc.on('devtools-closed', () => { const k = keyed.get(wc.id); if (k) attach(wc, k); });
    wc.debugger.on('message', (_ev, method, params, sessionId) => {
      // a frame or worker a box page started: guarded before it runs, and never obid's to hear
      if (method === 'Target.attachedToTarget') { if (box) void guardClipboard(wc.debugger, (params as { sessionId: string }).sessionId); return; }
      if (sessionId || method.startsWith('Target.')) return;
      const k = keyed.get(wc.id);
      if (k) host.event(k, method, params);
    });
    wc.debugger.on('detach', () => {
      const k = keyed.get(wc.id);
      if (!k) return;
      host.detach(k);
      // Chromium dropped it (not DevTools, not the guest going): attached again, as T3 recovers it
      setTimeout(() => { if (!wc.isDestroyed() && !wc.isDevToolsOpened() && keyed.get(wc.id) === k) attach(wc, k); }, 250);
    });
  });

  // The debugger on, then the tab announced once its first page settled (T3: DesktopBrowserHost.ts:569-606)
  function attach(wc: WebContents, k: TabKey) {
    if (wc.isDestroyed() || wc.isDevToolsOpened()) return;
    try { if (!wc.debugger.isAttached()) wc.debugger.attach('1.3'); } catch { return; }
    // a box tab is obid's to drive only once its clipboard guard is on
    const guarded = boxes.has(wc.session) ? guardClipboard(wc.debugger) : Promise.resolve();
    let announced = false;
    const announce = () => {
      if (announced) return;
      announced = true;
      void guarded.then(() => { if (!wc.isDestroyed() && keyed.get(wc.id) === k) host.attach(k, asGuest(wc)); });
    };
    if (!wc.isLoading()) return announce();
    wc.once('dom-ready', announce);
    wc.once('did-stop-loading', announce);
    wc.once('did-fail-load', announce);
    setTimeout(announce, SETTLE_MS);
  }

  return {
    /** the page mounted a webview for obid's tab: null once it's the relay's, else why not */
    register(k: TabKey, webContentsId: number): string | null {
      const wc = webContents.fromId(webContentsId);
      if (!wc || wc.isDestroyed() || wc.getType() !== 'webview' || wc.hostWebContents !== win.webContents) return 'Not one of this window’s pages.';
      const partition = named(k);
      if (!partition || !partitionOk(partition) || wc.session !== session.fromPartition(partition)) return 'Not the partition obid named for this tab.';
      const was = keyed.get(wc.id);
      if (was && (was.threadId !== k.threadId || was.tabId !== k.tabId)) return 'This page is already another tab’s.';
      if (owned.has(wc.id)) return 'This page is the owner’s own tab.';
      keyed.set(wc.id, k);
      told(wc);
      attach(wc, k);
      return null;
    },
    shown: (k: TabKey, shown: boolean) => host.shown(k, shown),
    /**
     * The page mounted a webview for an owner's own tab (BROWSERAPP-45): kept as theirs when it's this
     * window's and in the owner's partition, never keyed to obid's tab, never relayed, no debugger.
     * null once kept, else why not.
     */
    own(tabId: string, webContentsId: number): string | null {
      const wc = webContents.fromId(webContentsId);
      if (!tabId || !wc || wc.isDestroyed() || wc.getType() !== 'webview' || wc.hostWebContents !== win.webContents) return 'Not one of this window’s pages.';
      const partition = mountedIn.get(tabId);
      if (keyed.has(wc.id) || !partition || wc.session !== session.fromPartition(partition)) return 'Not the owner’s own tab.';
      const was = owned.get(wc.id);
      if (was !== undefined && was !== tabId) return 'This page is already another tab’s.';
      owned.set(wc.id, tabId);
      told(wc);
      return null;
    },
    /**
     * The page is about to mount an owner's own tab's webview (BROWSERAPP-45, 47): one webview may
     * then attach in that partition at that address, within MOUNT_MS. Only the owner's partition, or
     * that tab's own throwaway one, and only a web page. null once it may, else why not.
     */
    ownMount(tabId: string, partition: string, src: string): string | null {
      if (!OWN_TAB.test(tabId)) return 'Not one of the owner’s tabs.';
      if (partition !== OWN_PARTITION && partition !== scratchPartition(tabId)) return 'Not the owner’s own tab.';
      if (!ownPageOk(src)) return NOT_A_WEB_PAGE;
      mounting.set(tabId, { partition, src, until: Date.now() + MOUNT_MS });
      return null;
    },
    /** the owner's own tab to the address they typed (BROWSERAPP-46): a web page only, by its own webContents; null once it goes, else why not */
    go(tabId: string, url: string): string | null {
      if (!ownPageOk(url)) return NOT_A_WEB_PAGE;
      const wc = pageOf({ threadId: OWN_THREAD, tabId });
      if (!wc) return 'This tab has no page in this window.';
      // a load that fails is told as any is (did-fail-load): the page's own state
      wc.loadURL(url).catch(() => undefined);
      return null;
    },
    /**
     * The owner's Back, Forward, Reload or Stop loading, on that tab's own webContents in this window
     * (T3: Manager.ts:2213-2232): never through the debugger, the relay or obid, as Electron's
     * debugger would reload the embedder (T3: GuestInput.ts:182-191). null once done, else why not.
     */
    async nav(k: TabKey, op: NavOp): Promise<string | null> {
      const wc = pageOf(k);
      if (!wc) return 'This tab has no page in this window.';
      try {
        if (op !== 'stop') await leaveBusyPage(wc);
        if (op === 'back') { if (wc.navigationHistory.canGoBack()) wc.navigationHistory.goBack(); }
        else if (op === 'forward') { if (wc.navigationHistory.canGoForward()) wc.navigationHistory.goForward(); }
        else if (op === 'reload') wc.reload();
        else wc.stop();
        return null;
      } catch (e) {
        return (e as Error).message;
      }
    },
    /** Fill in on obid's tab, in this window's webview for it (typeInto): null once typed, else why not */
    type(tabId: string, field: string, value: string, code: boolean): Promise<string | null> {
      const id = [...keyed.entries()].find(([, k]) => k.tabId === tabId)?.[0];
      const wc = id === undefined ? undefined : webContents.fromId(id);
      if (!wc || wc.isDestroyed() || wc.hostWebContents !== win.webContents || !wc.debugger.isAttached()) return Promise.resolve('This page isn’t open in the window.');
      const k = keyed.get(wc.id)!;
      return typeInto(wc, k, named(k), host, field, value, code);
    },
  };
}
