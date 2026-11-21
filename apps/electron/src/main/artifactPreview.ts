import { app, session, WebContentsView, type BrowserWindow, type Session } from 'electron';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { send } from './obid';
import { ipc, type Trusted } from './ipc';
import { startRelay, type Relay } from './sandpackRelay';
import { allowed, previewAsk } from './previewRules';

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

export const PARTITION = 'oby-artifact-preview';
const dev = process.env.ELECTRON_RENDERER_URL;

export type Shown = { shown: true } | { error: string };

type View = { view: WebContentsView; loaded: Promise<void>; relay: Relay };
let current: View | null = null;
let starting: Promise<View> | null = null;
const refused: string[] = [];

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

async function makeView(win: BrowserWindow): Promise<View> {
  const relay = await startRelay();
  const rendererDir = join(__dirname, '../renderer');
  const hostPage = pathToFileURL(rendererDir).href + '/';
  const s = session.fromPartition(PARTITION);
  lockSession(s, relay, hostPage);
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
  win.contentView.addChildView(view);
  const loaded = dev ? wc.loadURL(`${dev}/preview.html`) : wc.loadFile(join(rendererDir, 'preview.html'));
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
  const files: Record<string, { text: string } | { base64: string }> = {};
  const utf8 = new TextDecoder('utf-8', { fatal: true });
  for (const path of v.files) {
    const f = await send('GET', `/v1/artifacts/${id}/${version}/${path.split('/').map(encodeURIComponent).join('/')}`, undefined);
    if (!(f instanceof Response) || !f.ok) return `${path} couldn’t be read.`;
    const bytes = new Uint8Array(await f.arrayBuffer());
    try { files[path] = { text: utf8.decode(bytes) }; } catch { files[path] = { base64: Buffer.from(bytes).toString('base64') }; }
  }
  return { kind: a.kind, title: a.title, files };
}

/** The `artifact-preview` channel; the answer: a function that takes the view away. */
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
    if (typeof read !== 'string' && edits) for (const [path, text] of Object.entries(edits)) if (path in read.files) read.files[path] = { text };
    // another version, or none, was asked for while this one read: that ask decides
    if (wanted !== key) return { shown: true };
    if (typeof read === 'string') { view.setVisible(false); return { error: read }; }
    await loaded;
    // the files go as data, never as code: JSON is a JavaScript literal
    await view.webContents.executeJavaScript(`window.obyShow(${JSON.stringify({ ...read, bundler: relay.origin })})`);
    if (wanted === key) { onView = key; view.setVisible(true); }
    return { shown: true };
  }
  return () => {
    const c = current;
    current = null;
    if (!c) return;
    const w = window();
    if (w && !w.isDestroyed()) w.contentView.removeChildView(c.view);
    if (!c.view.webContents.isDestroyed()) c.view.webContents.close();
    void c.relay.close();
  };
}
