import { app, session, WebContentsView, type BrowserWindow, type Session } from 'electron';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { send } from './obid';
import { ipc, type Trusted } from './ipc';
import { startRelay, type Relay } from './sandpackRelay';
import { allowed, canvasStatus, noFiles, oneColour, overlay, pageError, previewAsk, previewStatus, shownAs, thumbAsk, type Status } from './previewRules';

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
//
// The pane's Reload (`artifact-reload`, ARTIFACTSAPP-20) runs the version on view again, in the
// pane's view only: the host page's own refresh, a constant script, never the picture's view.
//
// Both wait for the host page's status (host.ts, ARTIFACTSAPP-23), what the bridge in the page said:
// the pane's view shows once it has drawn, so a new version never flashes white, and its answer says
// what broke (`pageError`, ARTIFACTSAPP-24); a picture is taken once it has drawn, and again while
// it comes out blank (ARTIFACTSAPP-25). The status is the artifact's own word and can be forged: it
// only times these and gives text, never more.
//
// A Claude Design design (`design`) is drawn by obid's canvas host and Oby's runtime (ARTIFACTS-13 to
// 15; ARTIFACTSAPP-29 and 30): a third view, built the same way in the same partition and on the same
// lock, opens `__canvas.html` on its version's own origin, `<id>-v<n>.localhost` on the relay's port,
// which the relay serves from obid's file route for that version only, with obid's board and host
// policies as obid sent them. The canvas host is its top page (its policy lets no page frame it), each
// board a sandboxed frame of its own in it; the pane shows that view instead of the preview's, once
// every board has drawn or 4 s passed, and what a board threw is its `pageError`. A card's picture of
// a design is none: the card keeps the board's blocks.

export const PARTITION = 'oby-artifact-preview';
const dev = process.env.ELECTRON_RENDERER_URL;
// a card's picture's page, in CSS px (ARTIFACTSAPP-18): drawn at scale 2, a 1280x370 picture
const HALF = { width: 640, height: 185 };

export type Shown = { shown: true; pageError?: string } | { error: string };
export type Thumb = { src: string } | { none: true };

type View = { view: WebContentsView; loaded: Promise<void>; relay: Relay };
let current: View | null = null;
let starting: Promise<View> | null = null;
// the one relay, and the session locked to it, both views use (a second onBeforeRequest would
// replace the first, and a second relay's port would be refused): made once per window lifetime
let shared: Promise<Relay> | null = null;
// what the session refused, each with the view that asked and its number, the last 50
const refused: { n: number; url: string; by: number | undefined }[] = [];
let refusals = 0;
// the picture's view's, whose refusals are never the pane's page's
let thumbContents: number | undefined;
// the design's view, and the versions the relay may serve on their own origin: the one on view, and
// the one being read (`<id>/<version>`)
let designView: View | null = null;
const designs = new Set<string>();
const rendererDir = join(__dirname, '../renderer');

function lockSession(s: Session, relay: Relay, hostPage: string) {
  s.webRequest.onBeforeRequest((d, done) => {
    const ok = allowed(d.url, relay.port, hostPage, dev);
    if (!ok) { refused.push({ n: ++refusals, url: d.url, by: d.webContentsId }); if (refused.length > 50) refused.shift(); console.warn(`artifact preview: refused ${d.url.slice(0, 200)}`); }
    done({ cancel: !ok });
  });
  s.setPermissionRequestHandler((_wc, _p, answer) => answer(false));
  s.setPermissionCheckHandler(() => false);
  s.on('will-download', (e) => e.preventDefault());
}

function locked(): Promise<Relay> {
  shared ??= startRelay({
    shown: (id, version) => designs.has(`${id}/${version}`),
    read: (id, version, path) => send('GET', `/v1/artifacts/${id}/${version}${path}`, undefined),
  }).then((relay) => {
    lockSession(session.fromPartition(PARTITION), relay, pathToFileURL(rendererDir).href + '/');
    return relay;
  }, (e) => { shared = null; throw e; });
  return shared;
}

async function makeView(win: BrowserWindow, thumb = false, design = false): Promise<View> {
  const relay = await locked();
  const view = new WebContentsView({
    webPreferences: {
      partition: PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false,
      nodeIntegrationInSubFrames: false, nodeIntegrationInWorker: false, webviewTag: false,
      spellcheck: false, safeDialogs: true, navigateOnDragDrop: false, devTools: !app.isPackaged,
      // the picture's view is never shown: its page's timers run on all the same
      ...(thumb ? { backgroundThrottling: false } : {}),
    },
  });
  view.setBackgroundColor('#ffffff');
  const wc = view.webContents;
  if (thumb) thumbContents = wc.id;
  wc.setWindowOpenHandler(() => ({ action: 'deny' }));
  wc.on('will-navigate', (e) => e.preventDefault());
  wc.on('will-attach-webview', (e) => e.preventDefault());
  view.setVisible(false);
  // the picture's view stays out of the window: a hidden view in it captures nothing (Electron 44)
  if (!thumb) win.contentView.addChildView(view);
  // the design's view loads its version's canvas host when it shows one
  const loaded = design ? Promise.resolve() : dev ? wc.loadURL(`${dev}/preview.html${thumb ? '?thumb' : ''}`) : wc.loadFile(join(rendererDir, 'preview.html'), thumb ? { search: 'thumb' } : undefined);
  return { view, loaded, relay };
}

// the host page's status, at most 4 s; anything but its shapes a timeout
const statusOf = (view: WebContentsView): Promise<Status> => Promise.race([
  view.webContents.executeJavaScript('window.obyStatus').then(previewStatus, (): Status => ({ timeout: true })),
  new Promise<Status>((done) => setTimeout(() => done({ timeout: true }), 4_000)),
]);

// a design's canvas host, what it says once every board drew, at most 4 s; anything else drawing still
async function canvasOf(view: WebContentsView): Promise<Status> {
  const read = () => view.webContents.executeJavaScript('window.__OBY_CANVAS ? JSON.parse(JSON.stringify({ ready: window.__OBY_CANVAS.ready, errors: window.__OBY_CANVAS.errors })) : null').catch(() => null);
  const end = Date.now() + 4_000;
  for (;;) {
    const v = await read();
    const s = canvasStatus(v, Date.now() >= end);
    if (s) return s;
    await new Promise((done) => setTimeout(done, 100));
  }
}

// an artifact's kind and its version's files, or why not
async function kindOf(id: string, version: number): Promise<string | { error: string }> {
  const r = await send('GET', `/v1/artifacts/${id}`, undefined);
  if (!(r instanceof Response)) return { error: r.text || 'obid isn’t answering.' };
  if (!r.ok) return { error: r.status === 404 ? 'That artifact isn’t there any more.' : `obid answered ${r.status}.` };
  const a = await r.json() as { kind: string; versions: { version: number }[] };
  if (!a.versions.some((x) => x.version === version)) return { error: `Version ${version} isn’t there.` };
  return a.kind;
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

/** The `artifact-preview`, `artifact-reload` and `artifact-thumb` channels; the answer: a function that takes the views away. */
export function previewing(trusted: Trusted, window: () => BrowserWindow | null): () => void {
  // the version the page wants on view ('' for none), and the one that is: an ask for the one
  // on view only moves it (the pane resized), and a read whose version is no longer wanted shows nothing
  let wanted = '';
  let onView = '';
  let reading: { key: string; done: Promise<Shown> } | null = null;
  let bounds = { x: 0, y: 0, width: 0, height: 0 };
  // whether the version on view is a design's, in the design's view
  let onDesign = false;
  const hide = () => { wanted = ''; onView = ''; current?.view.setVisible(false); designView?.view.setVisible(false); };
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
    bounds = { x: Math.round(b.x), y: Math.round(b.y), width: Math.round(b.width), height: Math.round(b.height) };
    current.view.setBounds(bounds);
    designView?.view.setBounds(bounds);
    if (key === onView) return { shown: true };
    // the same version already being read (the pane moved while it read): that read shows it
    if (reading?.key === key) return reading.done;
    onView = '';
    const done = show(key, ask.id, ask.version, ask.edits);
    reading = { key, done };
    try { return await done; } finally { if (reading?.done === done) reading = null; }
  }, null);
  // the version on view, run again; with none on view, nothing. It always answers null
  ipc(trusted).handle('artifact-reload', [], async (): Promise<null> => {
    if (onView && onDesign) designView?.view.webContents.reload();
    else if (current && onView) await current.view.webContents.executeJavaScript('window.obyReload?.()').catch(() => {});
    return null;
  }, null);
  async function show(key: string, id: string, version: number, edits?: Record<string, string>): Promise<Shown> {
    const since = refusals;
    const kind = await kindOf(id, version);
    if (wanted !== key) return { shown: true };
    if (typeof kind !== 'string') { current?.view.setVisible(false); designView?.view.setVisible(false); return kind; }
    if (kind === 'design') return showDesign(key, id, version, since);
    const { view, loaded, relay } = current!;
    const read = await readVersion(id, version);
    // each edit over its file, a file of that version only
    if (typeof read !== 'string') overlay(read.files, edits);
    // another version, or none, was asked for while this one read: that ask decides
    if (wanted !== key) return { shown: true };
    if (typeof read === 'string') { view.setVisible(false); return { error: read }; }
    await loaded;
    await view.webContents.executeJavaScript(shownAs({ ...read, bundler: relay.origin }));
    const status = await statusOf(view);
    if (wanted === key) { onView = key; onDesign = false; designs.clear(); designView?.view.setVisible(false); view.setVisible(true); }
    const said = pageError(status, refused.filter((r) => r.n > since && r.by !== thumbContents && r.by !== designView?.view.webContents.id).map((r) => r.url));
    return said ? { shown: true, pageError: said } : { shown: true };
  }
  // a design: its canvas host on its version's own origin, in the design's view (ARTIFACTSAPP-29)
  async function showDesign(key: string, id: string, version: number, since: number): Promise<Shown> {
    const win = window();
    if (!win || win.isDestroyed()) return { shown: true };
    designView ??= await makeView(win, false, true);
    const { view, relay } = designView;
    view.setBounds(bounds);
    const pair = `${id}/${version}`;
    // the relay serves this version, and the one on view until this one shows; none other
    designs.clear();
    if (onDesign && onView) designs.add(onView.split('/').slice(0, 2).join('/'));
    designs.add(pair);
    try {
      await view.webContents.loadURL(`http://${id}-v${version}.localhost:${relay.port}/__canvas.html`);
    } catch (e) {
      if (wanted !== key) return { shown: true };
      view.setVisible(false);
      return { error: `The design couldn’t open: ${e instanceof Error ? e.message : String(e)}` };
    }
    const status = await canvasOf(view);
    if (wanted !== key) return { shown: true };
    designs.clear();
    designs.add(pair);
    onView = key; onDesign = true;
    current?.view.setVisible(false);
    view.setVisible(true);
    const said = pageError(status, refused.filter((r) => r.n > since && r.by === view.webContents.id).map((r) => r.url));
    return said ? { shown: true, pageError: said } : { shown: true };
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
    // a design's card keeps the board's blocks
    if ((await kindOf(id, version)) === 'design' || gone()) return { none: true };
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
    await statusOf(view);
    if (gone()) return { none: true };
    let image = await view.webContents.capturePage();
    // blank: nudged a pixel narrower and back, as LlamaCoder repaints its frame
    // (nudgePreviewIframeGeometry), and taken again, at most 3 more times; the last is kept even
    // then, as a page that really is one colour stays so
    for (let again = 0; again < 3 && (image.isEmpty() || oneColour(image.toBitmap())); again++) {
      if (gone()) return { none: true };
      view.setBounds({ x: 0, y: 0, width: HALF.width - 1, height: HALF.height });
      await new Promise((done) => setTimeout(done, 150));
      if (gone()) return { none: true };
      view.setBounds({ x: 0, y: 0, ...HALF });
      image = await view.webContents.capturePage();
    }
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
    if (designView) drop(designView);
    designView = null;
    designs.clear();
    void thumbView?.then(drop, () => {});
    thumbView = null;
    thumbs.clear();
    void shared?.then((relay) => relay.close(), () => {});
    shared = null;
  };
}
