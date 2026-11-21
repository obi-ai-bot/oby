import { app, shell, type BrowserWindow } from 'electron';
import { existsSync } from 'node:fs';
import { downloadsFile } from './reveal';
import { HOME, send } from './obid';
import { ipc, type Trusted } from './ipc';
import { flag, safeInteger, string } from '../shared/decode';
import { fillSecret } from './device';
import { BROWSER_EVENTS, BROWSER_RELAY, sseReader } from '../shared/obid-wire';
import { namedPartitions, relayCommand, relayHost, type RelayEvent } from './browserRelay';
import { hosting } from './browserGuest';

// The browser, through main (docs/contracts/browser-app.md, BROWSERAPP-8). Main holds obid's token,
// so it follows obid's browser for the window, GET /v1/browser/events, passing each message on as it
// comes (`browser-event`), never kept. Reads and the owner's acts go through the `obid` channel as any
// call does (obid.ts, device.ts). The page is each tab's own <webview> (below): the owner's mouse and
// keys reach it as they reach any page, never through obid.

// The relay (docs/plans/browser-t3.md): obid's agents drive the page in the window's own <webview>,
// main running their CDP on its debugger (browserRelay.ts, browserGuest.ts). obid's commands come on
// a stream of their own, GET /v1/browser/relay, with no ids and no replay: a stream that drops and
// comes back never runs an earlier command again, it only announces the live tabs again. The
// webviews' events go back on POST /v1/browser/relay, in order, on the same token. With the window
// closed nothing follows the relay: obid's agents are told `no_window`, never given another browser.

// the partition obid named for each agent tab, from its `tab` events: the only one its webview may use
const partitions = namedPartitions();
function heard(name: string, data: string) {
  if (partitions.heard(name, data) === 'refused') console.warn('The browser: a tab event named a partition main doesn’t take for that tab; the first one stands.');
}
async function post(events: RelayEvent[]): Promise<boolean> {
  const r = await send('POST', BROWSER_RELAY, { events });
  if (!(r instanceof Response)) return false;
  await r.body?.cancel();
  // an obid without the relay (404) takes none of them, ever: nothing to send again
  return r.ok || r.status === 404;
}
let relay: ReturnType<typeof relayHost> | null = null;
let webviews: ReturnType<typeof hosting> | null = null;

/** the window's webviews, once it exists: hardened, checked against obid's partitions, relayed */
export function hostBrowser(win: BrowserWindow) {
  webviews = hosting(win, relay!, (k) => partitions.of(k));
}

// --- following obid's browser for the window

type Page = () => BrowserWindow | null;
const AGAIN_MS = 1_000;

// One SSE route followed until stopped: each message to `on`, `opened` each time it (re)connects
function follow(path: string, on: (name: string, data: string) => void, opened: () => void): () => void {
  let stopped = false;
  let ctl: AbortController | null = null;
  void (async () => {
    while (!stopped) {
      ctl = new AbortController();
      try {
        const r = await send('GET', path, undefined, ctl.signal);
        if (r instanceof Response && r.ok && r.body) {
          opened();
          const heard = sseReader(on);
          const decoder = new TextDecoder();
          const reader = r.body.getReader();
          for (;;) {
            const { done, value } = await reader.read();
            if (done) break;
            heard(decoder.decode(value, { stream: true }));
          }
        } else if (r instanceof Response) {
          // an obid without the browser (404) isn't followed again until the page asks
          if (r.status === 404) return;
          await r.body?.cancel();
        }
      } catch { /* dropped: read again below */ }
      if (!stopped) await new Promise((done) => setTimeout(done, AGAIN_MS));
    }
  })();
  return () => { stopped = true; ctl?.abort(); };
}

export function browsing(trusted: Trusted, page: Page) {
  const channels = ipc(trusted);
  const to = (channel: string, message: unknown) => { const w = page(); if (w && !w.isDestroyed()) w.webContents.send(channel, message); };
  // the agent's pointer is drawn by the page over its webview (T3: AgentBrowserCursor), nothing more
  // obid's home is read only when obid names a download folder
  relay = relayHost(post, { get home() { return HOME; }, pointer: (c) => to('browser-pointer', { threadId: c.threadId, tabId: c.tabId, phase: c.phase, x: c.x, y: c.y }) });
  let events: (() => void) | null = null;
  let commands: (() => void) | null = null;
  // the browser's events from now on, once per window; `open` each time the stream (re)connects, so
  // the page reads the lists again for what it missed
  channels.handle('browser-follow', [], () => {
    events ??= follow(BROWSER_EVENTS, (name, data) => { heard(name, data); to('browser-event', { name, data }); }, () => to('browser-event', { name: 'open', data: '' }));
    // obid's commands for the window's webviews: main's alone, the page never sees the CDP its agents
    // send; each time the relay (re)connects, every live tab is announced again, nothing run again
    commands ??= follow(BROWSER_RELAY, (name, data) => { const c = relayCommand(name, data); if (c) relay?.command(c); }, () => relay?.reconnected());
    return true;
  }, null);
  // a webview the page mounted for obid's tab: main checks it and relays it (browserGuest.ts); null once it's relayed
  channels.handle('browser-register', [string, string, safeInteger], (_e, threadId, tabId, webContentsId) =>
    webviews ? webviews.register({ threadId, tabId }, webContentsId) : 'The window isn’t up yet.', 'Not this app’s page.');
  // whether a tab's webview is on screen (T3's `shown`)
  channels.handle('browser-shown', [string, string, flag], (_e, threadId, tabId, shown) => { webviews?.shown({ threadId, tabId }, shown); return true; }, null);
  // a password or code into the page's field, on the owner's press of Fill in (device.ts, BROWSERAPP-12)
  channels.handle('browser-fill', [string, string, string, string, flag], (_e, tab, field, value, ask, clicked) =>
    fillSecret(tab, field, value, ask || undefined, clicked), 'Not this app’s page.');
  // Show in Finder for a download in the owner's Downloads (BROWSERAPP-16): only a file of that name
  // there, never another folder (its name's last part only); true once shown
  channels.handle('browser-reveal', [string], (_e, name) => {
    const file = downloadsFile(app.getPath('downloads'), name);
    if (!file || !existsSync(file)) return false;
    shell.showItemInFolder(file);
    return true;
  }, false);
  // the window gone: nothing followed for it
  return () => { events?.(); events = null; commands?.(); commands = null; };
}
