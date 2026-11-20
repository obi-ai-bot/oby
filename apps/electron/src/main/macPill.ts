import { BrowserWindow, globalShortcut, screen } from 'electron';
import { join } from 'node:path';
import { ipc, type Trusted } from './ipc';
import type { Decoder } from '../shared/decode';

// An agent using the Mac (docs/contracts/browser-app.md, BROWSERAPP-19; the design's board 15): Oby's
// pill over the screen with Stop, and an orange frame around it, each a small window of main's own over
// every app and Space, never taking the keyboard; the frame lets every click through. Esc pressed twice
// stops it too, caught by main only while the pill shows. Stop (either way) goes back to the window
// (`mac-stopped`), which ends the agent's use. Who has the Mac comes from the window (`mac-pill`): on the
// demo data now; desktop control's own contract brings it on obid.

type Use = { agent: string; action: string };
// the pill's state: null to put it away
const useOf: Decoder<Use | null> = (v) => {
  if (v === null) return { value: null };
  const u = v as Partial<Use> | undefined;
  return u && typeof u.agent === 'string' && typeof u.action === 'string' ? { value: { agent: u.agent, action: u.action } } : undefined;
};

let pill: BrowserWindow | null = null;
let frame: BrowserWindow | null = null;
// who has the Mac now, for the pill's window to ask once it listens
let current: Use | null = null;
let escaping = false;
let last = 0;
const TWICE_MS = 600;
const PILL = { width: 620, height: 64 };

function overlay(what: 'macpill' | 'macframe', bounds: Electron.Rectangle, preload: string, dev: string | undefined): BrowserWindow {
  const w = new BrowserWindow({
    ...bounds, show: false, frame: false, transparent: true, resizable: false, movable: false, focusable: false,
    skipTaskbar: true, hasShadow: false, alwaysOnTop: true,
    webPreferences: { preload, sandbox: true, contextIsolation: true, nodeIntegration: false },
  });
  w.setAlwaysOnTop(true, 'screen-saver');
  w.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  if (what === 'macframe') w.setIgnoreMouseEvents(true);
  if (dev) void w.loadURL(`${dev}?${what}`);
  else void w.loadFile(join(__dirname, '../renderer/index.html'), { search: what });
  return w;
}

export function macPill(trusted: Trusted, page: () => BrowserWindow | null, preload: string, dev: string | undefined) {
  const stopped = () => {
    hide();
    const w = page();
    if (w && !w.isDestroyed()) w.webContents.send('mac-stopped');
  };
  const hide = () => {
    if (escaping) globalShortcut.unregister('Escape');
    escaping = false;
    pill?.destroy(); frame?.destroy();
    pill = null; frame = null; current = null;
  };
  const show = (use: Use) => {
    current = use;
    const area = screen.getPrimaryDisplay().bounds;
    if (!frame) { frame = overlay('macframe', area, preload, dev); frame.once('ready-to-show', () => frame?.showInactive()); }
    if (!pill) {
      pill = overlay('macpill', { x: Math.round(area.x + (area.width - PILL.width) / 2), y: area.y + 30, ...PILL }, preload, dev);
      pill.once('ready-to-show', () => pill?.showInactive());
      // Esc twice stops it, only while the pill shows
      escaping = globalShortcut.register('Escape', () => {
        const now = Date.now();
        if (now - last < TWICE_MS) stopped();
        last = now;
      });
    } else pill.webContents.send('mac-pill', use);
  };
  // the window says who has the Mac, or null
  ipc(trusted).handle('mac-pill', [useOf], (_e, use) => { if (use) show(use); else hide(); return true; }, null);
  // the pill's own window: who has the Mac, asked once it listens, and its Stop; only from that window
  const fromPill = ipc((e) => !!pill && e.sender === pill.webContents && e.senderFrame === pill.webContents.mainFrame);
  fromPill.handle('mac-pill-now', [], () => current, null);
  fromPill.on('mac-pill-stop', [], () => { stopped(); });
  return hide;
}
