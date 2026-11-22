import { writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { app, session, webContents, type BrowserWindow, type Session, type WebContents } from 'electron';
import { BOX_PROXY, partitionOk, type BoxCommand, type Guest, type RelayHost, type TabKey } from './browserRelay';
export { PARTITION, partitionOk } from './browserRelay';
import { pageOk } from './pageRule';
export { pageOk } from './pageRule';

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
  if (!partitionOk(params.partition) || !pageOk(params.src)) return false;
  delete prefs.preload;
  delete prefs.preloadURL;
  Object.assign(prefs, { sandbox: true, nodeIntegration: false, nodeIntegrationInSubFrames: false, contextIsolation: true, webSecurity: true, allowRunningInsecureContent: false, disableHtmlFullscreenWindowResize: true });
  return true;
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
    guardFile = join(app.getPath('userData'), 'box-clipboard-guard.js');
    writeFileSync(guardFile, BOX_CLIPBOARD_GUARD, { mode: 0o600 });
  }
  return guardFile;
}

/**
 * One partition's session, set up once: its permissions and where its downloads go; for a box, every
 * request held until its job's door is set, cancelled after BOX_HOLD_MS without one (T3: getSession,
 * BrowserSession.ts:578-683, its handlers at 618-643). Run before its guest attaches.
 */
export function prepare(s: Session, partition: string) {
  if (ready.has(s)) return;
  ready.add(s);
  const granted = isBox(partition) ? BOX_PERMISSIONS : PERMISSIONS;
  s.setPermissionRequestHandler((_wc, permission, done) => done(granted.has(permission)));
  s.setPermissionCheckHandler((_wc, permission) => granted.has(permission));
  s.on('will-download', (_e, item, wc) => {
    // an agent's download goes where obid said for its tab (Page.setDownloadBehavior), else nowhere
    const folder = wc ? folders.get(wc.id) : null;
    if (!folder) { item.cancel(); return; }
    item.setSavePath(join(folder, basename(item.getFilename()) || 'download'));
  });
  if (!isBox(partition)) return;
  boxes.set(s, partition);
  // the clipboard guard as the session's preload: in every frame, in its own world, before the page's
  // first script, its first document's too (the debugger's guard comes only once main attaches it)
  s.registerPreloadScript({ type: 'frame', id: 'oby-box-clipboard-guard', filePath: guardPreload() });
  s.webRequest.onBeforeRequest((_details, callback) => {
    if (doors.has(partition)) return callback({});
    const held = waiting.get(partition) ?? new Set<() => void>();
    waiting.set(partition, held);
    const go = () => { clearTimeout(timer); held.delete(go); callback({}); };
    const timer = setTimeout(() => { held.delete(go); callback({ cancel: true }); }, BOX_HOLD_MS);
    held.add(go);
  });
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
  // a keyed tab's page state, from its own webContents (T3: Manager.ts:1185-1192, 1214-1257): its
  // address too, as obid's tab keeps the address of obid's own last command
  const told = (wc: WebContents) => {
    const k = keyed.get(wc.id);
    if (!k || wc.isDestroyed()) return;
    report({ threadId: k.threadId, tabId: k.tabId, url: wc.getURL(), canGoBack: wc.navigationHistory.canGoBack(), canGoForward: wc.navigationHistory.canGoForward(), loading: wc.isLoadingMainFrame(), failed: failedLoad.get(wc.id) ?? null });
  };
  win.webContents.on('will-attach-webview', (e, prefs, params) => {
    if (!harden(prefs as Record<string, unknown>, params)) { e.preventDefault(); return; }
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
    // a page of the web stays on the web: no window of its own, no file:, javascript: or data:
    wc.setWindowOpenHandler(({ url }) => { if (pageOk(url)) void wc.loadURL(url); return { action: 'deny' }; });
    wc.on('will-navigate', (ev, url) => { if (!pageOk(url)) ev.preventDefault(); });
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
      keyed.set(wc.id, k);
      told(wc);
      attach(wc, k);
      return null;
    },
    shown: (k: TabKey, shown: boolean) => host.shown(k, shown),
    /**
     * The owner's Back, Forward, Reload or Stop loading, on that tab's own webContents in this window
     * (T3: Manager.ts:2213-2232): never through the debugger, the relay or obid, as Electron's
     * debugger would reload the embedder (T3: GuestInput.ts:182-191). null once done, else why not.
     */
    async nav(k: TabKey, op: NavOp): Promise<string | null> {
      let wc: WebContents | undefined;
      for (const [id, t] of keyed) {
        if (t.threadId !== k.threadId || t.tabId !== k.tabId) continue;
        const w = webContents.fromId(id);
        if (w && !w.isDestroyed() && w.hostWebContents === win.webContents) wc = w;
      }
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
  };
}
