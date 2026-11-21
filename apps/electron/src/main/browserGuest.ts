import { basename, join } from 'node:path';
import { session, webContents, type BrowserWindow, type Session, type WebContents } from 'electron';
import type { Guest, RelayHost, TabKey } from './browserRelay';

// Each agent tab's `<webview>` in the window, as T3 Code hosts them (apps/desktop/src/window/
// DesktopWindow.ts, preview/Manager.ts; docs/plans/browser-t3.md): hardened when it attaches, in
// the partition obid named for its tab and no other, its debugger attached ("1.3") and handed to
// the relay once its first page settled, and given back when DevTools takes it.

/**
 * The partitions a page may live in, as obid names them: one per agent for the owner's own work
 * (sign-ins kept), and one per job for another Oby's or an untrusted job (not persistent). Never the
 * default session or the window's own.
 */
export const PARTITION = /^(persist:oby-agent-(claude|codex|cursor)|oby-box-[A-Za-z0-9_-]{1,128})$/;
export const partitionOk = (p: unknown): p is string => typeof p === 'string' && PARTITION.test(p);
/** a page's address the window loads: the web, or a blank page */
export const pageOk = (url: unknown) => typeof url === 'string' && /^(https?:\/\/|about:blank$)/.test(url);
/** how long the first page may take to settle before the tab is announced anyway (T3: 5 s) */
export const SETTLE_MS = 5_000;
/** the permissions a page gets (T3's list, less geolocation and the loopback network): no others */
export const PERMISSIONS = new Set(['clipboard-sanitized-write', 'fullscreen', 'pointerLock']);

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

/**
 * The window's webviews for the relay: `register` ties one the page mounted to obid's tab (the page
 * says which; main checks the webview is this window's and its partition is the one obid named),
 * `shown` passes on whether it's on screen.
 */
export function hosting(win: BrowserWindow, host: RelayHost, named: (k: TabKey) => string | undefined) {
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
      attach(wc, k);
      return null;
    },
    shown: (k: TabKey, shown: boolean) => host.shown(k, shown),
  };
}
