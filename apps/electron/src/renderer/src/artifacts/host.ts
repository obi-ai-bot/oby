import { SandpackStatic } from 'oby-sandpack-static';
import { RENDERED } from './react';
import { isReact, showable, type Version } from './showable';

// The artifact preview's host page (preview.html; docs/contracts/artifacts-app.md, ARTIFACTSAPP-1
// and 3). It runs only in main's preview view (main/artifactPreview.ts): its own partition, no
// preload, no Node, sandboxed. Main calls `obyShow` with a version's files and its relay's address,
// and Sandpack's static client shows them in a frame of the relay's (`<id>-preview.localhost`), its
// own origin each time. The frame may run scripts and keep its own origin (the relay's service worker
// needs one), and nothing else: no popups, no downloads, no navigating this page, no camera or
// microphone.

type Show = Version & { bundler: string };

const SANDBOX = 'allow-scripts allow-same-origin allow-forms allow-modals';
let client: SandpackStatic | null = null;
let listening: AbortController | null = null;

function show(v: Show): void {
  client?.destroy();
  listening?.abort();
  // the last preview's frame, and the relay frame Sandpack added beside it
  for (const f of document.querySelectorAll('iframe')) f.remove();
  const frame = document.createElement('iframe');
  frame.setAttribute('sandbox', SANDBOX);
  frame.setAttribute('allow', '');
  frame.setAttribute('title', v.title);
  frame.setAttribute('data-artifact-preview', '');
  document.body.append(frame);
  const shown = showable(v);
  // drawn: main waits on it before a card's picture (ARTIFACTSAPP-18). A React page that compiled says
  // so itself, from this frame only, once its first render or its error has drawn; any other, the
  // frame's first load once Sandpack gave it the preview's address (its `done` comes before the page loads)
  listening = new AbortController();
  const { signal } = listening;
  window.obyDrawn = isReact(v) && '/App.js' in shown
    ? new Promise((done) => window.addEventListener('message', (e) => { if (e.data === RENDERED && e.source === frame.contentWindow) done(); }, { signal }))
    : new Promise((done) => frame.addEventListener('load', () => { if (frame.getAttribute('src')) done(); }));
  // bytes go as they are: the static client keeps a Uint8Array as one (its writeBuffer)
  client = new SandpackStatic(frame, { files: shown as Record<string, { code: string }>, template: 'static' }, { bundlerURL: v.bundler, showOpenInCodeSandbox: false });
}

declare global { interface Window { obyShow?: (v: Show) => void; obyDrawn?: Promise<void>; obyReload?: () => void } }
window.obyShow = show;
// the pane's Reload (ARTIFACTSAPP-20): the static client's own refresh, which reloads the frame
window.obyReload = () => client?.dispatch({ type: 'refresh' });
