import { SandpackStatic } from 'oby-sandpack-static';
import { showable, type Version } from './showable';

// The artifact preview's host page (preview.html; docs/contracts/artifacts-app.md, ARTIFACTSAPP-1
// and 3). It runs only in main's preview view (main/artifactPreview.ts): its own partition, no
// preload, no Node, sandboxed. Main calls `obyShow` with a version's files and its relay's address,
// and Sandpack's static client shows them in a frame of the relay's (`<id>-preview.localhost`), its
// own origin each time. The frame may run scripts and keep its own origin (the relay's service worker
// needs one), and nothing else: no popups, no downloads, no navigating this page, no camera or
// microphone.

type Show = Version & { bundler: string };
/** What main waits on before it shows the pane's view or takes a card's picture (ARTIFACTSAPP-23). */
type Status = { drawn: true } | { error: string; console: string[] } | { timeout: true };

const SANDBOX = 'allow-scripts allow-same-origin allow-forms allow-modals';
// every page gets the bridge (ARTIFACTSAPP-22), from main's relay: Sandpack puts its tag in each
// .html's head, after its console hook and before the page's own scripts
const BRIDGE = '/__csb_oby/bridge.js';
const MOST = 4_000;
let client: SandpackStatic | null = null;
let clients: SandpackStatic[] = [];
let waiting: AbortController | null = null;

function show(v: Show): void {
  // a show still waiting: its word counts for nothing now
  waiting?.abort();
  // every frame before this show (the last preview's, and the relay frame Sandpack added beside it):
  // on view until this one settles, so a new version never flashes white
  const before = [...document.querySelectorAll('iframe')];
  const frame = document.createElement('iframe');
  frame.setAttribute('sandbox', SANDBOX);
  frame.setAttribute('allow', '');
  frame.setAttribute('title', v.title);
  frame.setAttribute('data-artifact-preview', '');
  // laid out at full size under the last one (preview.html's style)
  document.body.prepend(frame);
  // bytes go as they are: the static client keeps a Uint8Array as one (its writeBuffer)
  const mine = new SandpackStatic(frame, { files: showable(v) as Record<string, { code: string }>, template: 'static' }, { bundlerURL: v.bundler, showOpenInCodeSandbox: false, externalResources: [BRIDGE] });
  client = mine;
  clients.push(mine);
  // The status, from this show's frame only: what the bridge in it posts. That is the artifact's own
  // code and can be forged, so it only times the view and the picture, and is text shown or attached,
  // never more (ARTIFACTSAPP-23)
  const listening = new AbortController();
  waiting = listening;
  const { signal } = listening;
  let error: string | undefined, loaded = false;
  const later: string[] = [];
  window.obyStatus = new Promise<Status>((done) => {
    const settle = (s: Status) => {
      listening.abort();
      for (const c of clients) if (c !== mine) c.destroy();
      clients = [mine];
      for (const f of before) f.remove();
      done(s);
    };
    const failed = () => settle({ error: error!, console: later });
    const ceiling = setTimeout(() => (error === undefined ? settle({ timeout: true }) : failed()), 4_000);
    signal.addEventListener('abort', () => clearTimeout(ceiling));
    // its first load once Sandpack gave it the preview's address: errors of the same parse are all in by then
    frame.addEventListener('load', () => { if (!frame.getAttribute('src')) return; loaded = true; if (error !== undefined) failed(); }, { signal });
    window.addEventListener('message', (e) => {
      if (e.source !== frame.contentWindow || !e.data || typeof e.data !== 'object') return;
      const { oby, message } = e.data as { oby?: unknown; message?: unknown };
      if (oby === 'drawn') { if (error === undefined) settle({ drawn: true }); else failed(); return; }
      if ((oby !== 'error' && oby !== 'console-error') || typeof message !== 'string') return;
      if (oby === 'error' && error === undefined) error = message.slice(0, MOST);
      else { later.push(message.slice(0, MOST)); if (later.length > 10) later.shift(); }
      if (error !== undefined && loaded) failed();
    }, { signal });
  });
}

declare global { interface Window { obyShow?: (v: Show) => void; obyStatus?: Promise<Status>; obyReload?: () => void } }
window.obyShow = show;
// the pane's Reload (ARTIFACTSAPP-20): the static client's own refresh, which reloads the frame
window.obyReload = () => client?.dispatch({ type: 'refresh' });
