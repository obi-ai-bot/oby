import { app, shell, type BrowserWindow } from 'electron';
import { existsSync } from 'node:fs';
import { downloadsFile } from './reveal';
import { send } from './obid';
import { ipc, type Trusted } from './ipc';
import { flag, string } from '../shared/decode';
import { fillSecret } from './device';
import { BROWSER_EVENTS, browserTabPath, sseReader } from '../shared/obid-wire';

// The browser, through main (docs/contracts/browser-app.md, BROWSERAPP-8 and 10). Main holds obid's
// token, so it follows obid's browser for the window: GET /v1/browser/events, and each tab's stream
// of frames the page draws (`…/stream`, the newest frame only), passing each message on as it comes
// (`browser-event`, `browser-frame`), never kept. Reads and the owner's acts go through the `obid`
// channel as any call does (obid.ts, device.ts). The owner's mouse and keys on a tab they hold go
// signed (`…/input`), each only right after a real input of that kind main saw itself
// (browserInput.ts): a script in the page can't drive a held tab alone.

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
  let events: (() => void) | null = null;
  const streams = new Map<string, () => void>();
  // the browser's events from now on, once per window; `open` each time the stream (re)connects, so
  // the page reads the lists again for what it missed
  channels.handle('browser-follow', [], () => {
    events ??= follow(BROWSER_EVENTS, (name, data) => to('browser-event', { name, data }), () => to('browser-event', { name: 'open', data: '' }));
    return true;
  }, null);
  // a tab's frames, while the page draws it
  channels.handle('browser-watch', [string], (_e, tab) => {
    if (!streams.has(tab)) streams.set(tab, follow(browserTabPath(tab, 'stream'), (name, data) => { if (name === 'frame') to('browser-frame', { tab, data }); }, () => {}));
    return true;
  }, null);
  channels.handle('browser-unwatch', [string], (_e, tab) => {
    streams.get(tab)?.();
    streams.delete(tab);
    return true;
  }, null);
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
  return () => { events?.(); events = null; for (const stop of streams.values()) stop(); streams.clear(); };
}
