import { basename, join } from 'node:path';
import { session, webContents, type BrowserWindow, type Session, type WebContents } from 'electron';
import { partitionOk, type Guest, type RelayHost, type TabKey } from './browserRelay';
export { PARTITION, partitionOk } from './browserRelay';
import { pageOk } from './pageRule';
export { pageOk } from './pageRule';

// Each agent tab's `<webview>` in the window, as T3 Code hosts them (apps/desktop/src/window/
// DesktopWindow.ts, preview/Manager.ts; docs/plans/browser-t3.md): hardened when it attaches, in
// the partition obid named for its tab and no other, its debugger attached ("1.3") and handed to
// the relay once its first page settled, and given back when DevTools takes it.

/** how long the first page may take to settle before the tab is announced anyway (T3: 5 s) */
export const SETTLE_MS = 5_000;
/** the permissions a page gets (T3's list, less geolocation and the loopback network): no others */
export const PERMISSIONS = new Set(['clipboard-sanitized-write', 'fullscreen', 'pointerLock']);
/** how long a page may take to answer before main leaves it anyway, stuck in a script (T3: Manager.ts:139) */
export const BUSY_PAGE_PROBE_MS = 1_000;
/** the owner's Back, Forward, Reload and Stop loading on a tab's own page */
export type NavOp = 'back' | 'forward' | 'reload' | 'stop';
/** what main tells the page of a tab's own page: is there one behind, one ahead, is it loading */
export type NavState = { threadId: string; tabId: string; canGoBack: boolean; canGoForward: boolean; loading: boolean };

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

// One partition's session, set up once: its permissions and where its downloads go
function prepare(s: Session) {
  if (ready.has(s)) return;
  ready.add(s);
  s.setPermissionRequestHandler((_wc, permission, done) => done(PERMISSIONS.has(permission)));
  s.setPermissionCheckHandler((_wc, permission) => PERMISSIONS.has(permission));
  s.on('will-download', (_e, item, wc) => {
    // an agent's download goes where obid said for its tab (Page.setDownloadBehavior), else nowhere
    const folder = wc ? folders.get(wc.id) : null;
    if (!folder) { item.cancel(); return; }
    item.setSavePath(join(folder, basename(item.getFilename()) || 'download'));
  });
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
  // a keyed tab's page state, from its own webContents (T3: Manager.ts:1214-1257)
  const told = (wc: WebContents) => {
    const k = keyed.get(wc.id);
    if (!k || wc.isDestroyed()) return;
    report({ threadId: k.threadId, tabId: k.tabId, canGoBack: wc.navigationHistory.canGoBack(), canGoForward: wc.navigationHistory.canGoForward(), loading: wc.isLoadingMainFrame() });
  };
  win.webContents.on('will-attach-webview', (e, prefs, params) => {
    if (!harden(prefs as Record<string, unknown>, params)) e.preventDefault();
  });
  win.webContents.on('did-attach-webview', (_e, wc) => {
    prepare(wc.session);
    // a page of the web stays on the web: no window of its own, no file:, javascript: or data:
    wc.setWindowOpenHandler(({ url }) => { if (pageOk(url)) void wc.loadURL(url); return { action: 'deny' }; });
    wc.on('will-navigate', (ev, url) => { if (!pageOk(url)) ev.preventDefault(); });
    wc.on('did-fail-load', (_ev, code, description, url, isMainFrame) => {
      const k = keyed.get(wc.id);
      if (k && isMainFrame && code !== -3) host.loadFailed(k, url, code, description);
    });
    // the page state, each time it may have changed (T3: Manager.ts:1662-1670)
    const tell = () => told(wc);
    wc.on('did-navigate', tell);
    wc.on('did-navigate-in-page', tell);
    wc.on('did-start-loading', tell);
    wc.on('did-stop-loading', tell);
    wc.on('did-fail-load', tell);
    wc.on('render-process-gone', () => { const k = keyed.get(wc.id); if (k) host.detach(k); });
    wc.once('destroyed', () => {
      const k = keyed.get(wc.id);
      keyed.delete(wc.id);
      folders.delete(wc.id);
      if (k) { host.detach(k); host.forget(k); }
    });
    // DevTools takes the debugger: the tab is detached until they close (T3: Manager.ts:2252-2267)
    wc.on('devtools-opened', () => { if (wc.debugger.isAttached()) wc.debugger.detach(); });
    wc.on('devtools-closed', () => { const k = keyed.get(wc.id); if (k) attach(wc, k); });
    wc.debugger.on('message', (_ev, method, params) => { const k = keyed.get(wc.id); if (k) host.event(k, method, params); });
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
    let announced = false;
    const announce = () => { if (announced || wc.isDestroyed() || keyed.get(wc.id) !== k) return; announced = true; host.attach(k, asGuest(wc)); };
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
