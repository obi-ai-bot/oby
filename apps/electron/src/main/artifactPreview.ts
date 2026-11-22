import { app, session, WebContentsView, type BrowserWindow, type Session } from 'electron';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { send } from './obid';
import { ipc, type Trusted } from './ipc';
import { startRelay, type Relay } from './sandpackRelay';
import { allowed, noFiles, overlay, previewAsk, shownAs, thumbAsk } from './previewRules';

// An artifact's preview, drawn by Sandpack (docs/contracts/artifacts-app.md, ARTIFACTSAPP-1 and 3).
// The page says which version and where (`artifact-preview`); main reads the version's files from
// obid with the token it holds, and shows them in a view of its own over that spot of the window:
//
// - its own partition (`oby-artifact-preview`, in memory only), so it shares no cookie, storage,
//   cache or service worker with the window's page;
// - no preload, no Node, Chromium's sandbox, context isolation: the page's bridge (`window.app`)
//   and obid are out of its reach;
// - it reaches only its host page (preview.html, this build's own file) and main's relay on
//   loopback (`<id>-preview.localhost`, main/sandpackRelay.ts): every other request of its session
//   is cancelled before it leaves, whatever the CSP says; it opens no window, downloads nothing,
//   is granted no permission, and its host page never navigates. A frame in it navigates only to
//   the relay's hosts: the host page's CSP (frame-src, vite.config.ts) refuses any other.
//
// A second view, never shown, takes the cards' pictures (`artifact-thumb`, ARTIFACTSAPP-18): built
// the same way, in the same partition, on the same one lock and one relay, its host page told apart
// by `?thumb`, and never in the window. The page gets only the picture's pixels, a PNG `data:` URL.

export const PARTITION = 'oby-artifact-preview';
const dev = process.env.ELECTRON_RENDERER_URL;
// a card's picture's page, in CSS px (ARTIFACTSAPP-18): drawn at scale 2, a 1280x370 picture
const HALF = { width: 640, height: 185 };

export type Shown = { shown: true } | { error: string };
export type Thumb = { src: string } | { none: true };

type View = { view: WebContentsView; loaded: Promise<void>; relay: Relay };
let current: View | null = null;
let starting: Promise<View> | null = null;
// the one relay, and the session locked to it, both views use (a second onBeforeRequest would
// replace the first, and a second relay's port would be refused): made once per window lifetime
let shared: Promise<Relay> | null = null;
const refused: string[] = [];
const rendererDir = join(__dirname, '../renderer');

function lockSession(s: Session, relay: Relay, hostPage: string) {
  s.webRequest.onBeforeRequest((d, done) => {
    const ok = allowed(d.url, relay.port, hostPage, dev);
    if (!ok) { refused.push(d.url); if (refused.length > 50) refused.shift(); console.warn(`artifact preview: refused ${d.url.slice(0, 200)}`); }
    done({ cancel: !ok });
  });
  s.setPermissionRequestHandler((_wc, _p, answer) => answer(false));
  s.setPermissionCheckHandler(() => false);
  s.on('will-download', (e) => e.preventDefault());
}

function locked(): Promise<Relay> {
  shared ??= startRelay().then((relay) => {
    lockSession(session.fromPartition(PARTITION), relay, pathToFileURL(rendererDir).href + '/');
    return relay;
  }, (e) => { shared = null; throw e; });
  return shared;
}

async function makeView(win: BrowserWindow, thumb = false): Promise<View> {
  const relay = await locked();
  const view = new WebContentsView({
    webPreferences: {
      partition: PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false,
      nodeIntegrationInSubFrames: false, nodeIntegrationInWorker: false, webviewTag: false,
      spellcheck: false, safeDialogs: true, navigateOnDragDrop: false, devTools: !app.isPackaged,
    },
  });
  view.setBackgroundColor('#ffffff');
  const wc = view.webContents;
  wc.setWindowOpenHandler(() => ({ action: 'deny' }));
  wc.on('will-navigate', (e) => e.preventDefault());
  wc.on('will-attach-webview', (e) => e.preventDefault());
  view.setVisible(false);
  // the picture's view stays out of the window: a hidden view in it captures nothing (Electron 44)
  if (!thumb) win.contentView.addChildView(view);
  const loaded = dev ? wc.loadURL(`${dev}/preview.html${thumb ? '?thumb' : ''}`) : wc.loadFile(join(rendererDir, 'preview.html'), thumb ? { search: 'thumb' } : undefined);
  return { view, loaded, relay };
}

// a version's files as obid serves them: text, or base64 for bytes that aren't UTF-8
async function readVersion(id: string, version: number): Promise<{ kind: string; title: string; files: Record<string, { text: string } | { base64: string }> } | string> {
  const r = await send('GET', `/v1/artifacts/${id}`, undefined);
  if (!(r instanceof Response)) return r.text || 'obid isn’t answering.';
  if (!r.ok) return r.status === 404 ? 'That artifact isn’t there any more.' : `obid answered ${r.status}.`;
  const a = await r.json() as { kind: string; title: string; versions: { version: number; files: string[] }[] };
  const v = a.versions.find((x) => x.version === version);
  if (!v) return `Version ${version} isn’t there.`;
  const files = noFiles();
  const utf8 = new TextDecoder('utf-8', { fatal: true });
  for (const path of v.files) {
    const f = await send('GET', `/v1/artifacts/${id}/${version}/${path.split('/').map(encodeURIComponent).join('/')}`, undefined);
    if (!(f instanceof Response) || !f.ok) return `${path} couldn’t be read.`;
    const bytes = new Uint8Array(await f.arrayBuffer());
    try { files[path] = { text: utf8.decode(bytes) }; } catch { files[path] = { base64: Buffer.from(bytes).toString('base64') }; }
  }
  return { kind: a.kind, title: a.title, files };
}

/** The `artifact-preview` and `artifact-thumb` channels; the answer: a function that takes the views away. */
export function previewing(trusted: Trusted, window: () => BrowserWindow | null): () => void {
  // the version the page wants on view ('' for none), and the one that is: an ask for the one
  // on view only moves it (the pane resized), and a read whose version is no longer wanted shows nothing
  let wanted = '';
  let onView = '';
  let reading: { key: string; done: Promise<Shown> } | null = null;
  const hide = () => { wanted = ''; onView = ''; current?.view.setVisible(false); };
  ipc(trusted).handle('artifact-preview', [previewAsk], async (_e, ask): Promise<Shown> => {
    const win = window();
    if (!ask || !win || win.isDestroyed()) { hide(); return { shown: true }; }
    // the owner's unsaved edits are part of what's on view (ARTIFACTSAPP-8)
    const key = `${ask.id}/${ask.version}/${ask.edits ? createHash('sha256').update(JSON.stringify(ask.edits)).digest('hex') : ''}`;
    wanted = key;
    if (!current) {
      starting ??= makeView(win);
      try { current = await starting; } finally { starting = null; }
      win.webContents.on('did-start-navigation', (d) => { if (d.isMainFrame && !d.isSameDocument) hide(); });
    }
    const b = ask.bounds;
    current.view.setBounds({ x: Math.round(b.x), y: Math.round(b.y), width: Math.round(b.width), height: Math.round(b.height) });
    if (key === onView) return { shown: true };
    // the same version already being read (the pane moved while it read): that read shows it
    if (reading?.key === key) return reading.done;
    onView = '';
    const done = show(key, ask.id, ask.version, ask.edits);
    reading = { key, done };
    try { return await done; } finally { if (reading?.done === done) reading = null; }
  }, null);
  async function show(key: string, id: string, version: number, edits?: Record<string, string>): Promise<Shown> {
    const { view, loaded, relay } = current!;
    const read = await readVersion(id, version);
    // each edit over its file, a file of that version only
    if (typeof read !== 'string') overlay(read.files, edits);
    // another version, or none, was asked for while this one read: that ask decides
    if (wanted !== key) return { shown: true };
    if (typeof read === 'string') { view.setVisible(false); return { error: read }; }
    await loaded;
    await view.webContents.executeJavaScript(shownAs({ ...read, bundler: relay.origin }));
    if (wanted === key) { onView = key; view.setVisible(true); }
    return { shown: true };
  }

  // a card's picture of a version (ARTIFACTSAPP-18), in main's memory only: each key's answer, or the
  // work taking it, the oldest dropped past 50; a version that couldn't be taken isn't taken again
  const thumbs = new Map<string, Promise<Thumb>>();
  let queue: Promise<unknown> = Promise.resolve();
  let thumbView: Promise<View> | null = null;
  // the key being answered ('' for none), and the window's lifetime (one more at each stop): a
  // capture that finds either changed since it started touches no view and answers none
  let answering = '';
  let lifetime = 0;
  ipc(trusted).handle('artifact-thumb', [thumbAsk], (_e, ask): Promise<Thumb> => {
    const key = `${ask.id}/${ask.version}`;
    let thumb = thumbs.get(key);
    if (!thumb) {
      // one at a time, in order
      thumb = queue.then(() => bounded(key, ask.id, ask.version)).catch((): Thumb => ({ none: true }));
      queue = thumb;
      thumbs.set(key, thumb);
      if (thumbs.size > 50) thumbs.delete(thumbs.keys().next().value!);
    }
    return thumb;
  }, null);
  // a card asked for a newer version of it since: its card shows that one, never this
  const older = (id: string, version: number) => [...thumbs.keys()].some((k) => k.startsWith(`${id}/`) && Number(k.slice(id.length + 1)) > version);
  // each capture cut off at 10 seconds: its view closed, so the next gets a fresh one (same session, same relay)
  async function bounded(key: string, id: string, version: number): Promise<Thumb> {
    if (older(id, version)) return { none: true };
    answering = key;
    let limit: ReturnType<typeof setTimeout> | undefined;
    const cut = new Promise<Thumb>((done) => {
      limit = setTimeout(() => {
        answering = '';
        void thumbView?.then(drop, () => {});
        thumbView = null;
        done({ none: true });
      }, 10_000);
    });
    try { return await Promise.race([take(key, id, version), cut]); } finally {
      clearTimeout(limit);
      if (answering === key) answering = '';
    }
  }
  async function take(key: string, id: string, version: number): Promise<Thumb> {
    const started = lifetime;
    const gone = () => answering !== key || lifetime !== started;
    const win = window();
    if (!win || win.isDestroyed()) return { none: true };
    const read = await readVersion(id, version);
    if (typeof read === 'string' || gone()) return { none: true };
    thumbView ??= makeView(win, true).then((v) => { v.view.setBounds({ x: 0, y: 0, ...HALF }); return v; }, (e) => { thumbView = null; throw e; });
    const { view, loaded, relay } = await thumbView;
    if (gone()) return { none: true };
    await loaded;
    if (gone()) return { none: true };
    // the card's ratio at half 1280's width and scale 2, so its words are twice a 1280-wide page's
    // shrunk to the card: emulated for this view alone, as a zoom would be every view's of the same
    // page (Chromium keeps one per host); again each time, as a new renderer would drop it
    view.webContents.enableDeviceEmulation({ screenPosition: 'desktop', screenSize: HALF, viewSize: HALF, viewPosition: { x: 0, y: 0 }, deviceScaleFactor: 2, scale: 1 });
    await view.webContents.executeJavaScript(shownAs({ ...read, bundler: relay.origin }));
    if (gone()) return { none: true };
    // the host page's own "drawn" (host.ts), at most 4 s
    const drawn = await Promise.race([view.webContents.executeJavaScript('window.obyDrawn').then(() => true), new Promise<false>((done) => setTimeout(() => done(false), 4_000))]);
    // ponytail: a frame just loaded isn't painted yet (taken at its load, the picture is blank on the
    // Mac), so a fixed settle after it; a paint signal from the frame would need a bridge into it
    if (drawn) await new Promise((done) => setTimeout(done, 250));
    if (gone()) return { none: true };
    const image = await view.webContents.capturePage();
    if (gone()) return { none: true };
    // 1280x370, made so where a display captured it larger
    const png = (image.getSize().width > 1280 ? image.resize({ width: 1280 }) : image).toPNG();
    return { src: 'data:image/png;base64,' + png.toString('base64') };
  }

  const drop = (v: View) => {
    const w = window();
    if (w && !w.isDestroyed()) w.contentView.removeChildView(v.view);
    if (!v.view.webContents.isDestroyed()) v.view.webContents.close();
  };
  return () => {
    lifetime++;
    answering = '';
    if (current) drop(current);
    current = null;
    void thumbView?.then(drop, () => {});
    thumbView = null;
    thumbs.clear();
    void shared?.then((relay) => relay.close(), () => {});
    shared = null;
  };
}
