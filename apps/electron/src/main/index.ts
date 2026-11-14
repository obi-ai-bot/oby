import { app, BrowserWindow, dialog, Menu, screen, shell } from 'electron';
import { createRequire } from 'node:module';
import { lstatSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { bridge, startObid, stopFollowing, type Trusted } from './obid';
import { confirming, ownersWords } from './device';
import { input, space, picked } from './words';
import { notifying, windowLeft, windowSeen } from './notify';
import { updating } from './updates';
import { menu } from '../shared/menu';
import { sphering } from './sphere';
import { ipc } from './ipc';
import { flag, https } from '../shared/decode';

// One small window: Oby, what needs you, and a way to talk. The renderer gets
// the cursor's position wherever it is on screen, so Oby can watch it outside
// its own frame.

type Chrome = { lightsLeft: number; lightsTop: number; lightsCenterY: number; lightsRight: number };

// The page holds obid's API and signs Confirms with this Mac's device key: a packaged app takes no
// DevTools port or pipe from whoever opens it (the fuses don't cover them), unless an admin allowed
// it on this Mac with a root-owned file nobody else may write (scripts/vm-test.sh makes one with
// sudo in its VM). Nor, ever, another program run as one of its own processes, or its windows
// out of the sandbox (#1641).
const ALLOW_DEVTOOLS = '/Library/Application Support/Oby/allow-devtools-port';
const rootOnly = (path: string) => { const s = lstatSync(path); return s.uid === 0 && !s.isSymbolicLink() && !(s.mode & 0o022); };
const given = (switches: string[]) => switches.some((s) => app.commandLine.hasSwitch(s));
if (app.isPackaged && given(['browser-subprocess-path', 'renderer-cmd-prefix', 'gpu-launcher', 'utility-cmd-prefix', 'no-sandbox'])) app.exit(1);
// every window sandboxed, one added later too, not only the one createWindow makes (before ready)
app.enableSandbox();
if (app.isPackaged && given(['remote-debugging-port', 'remote-debugging-pipe'])) {
  let allowed = false;
  try { allowed = rootOnly(dirname(ALLOW_DEVTOOLS)) && rootOnly(ALLOW_DEVTOOLS); } catch { /* not there */ }
  if (!allowed) app.exit(1);
}

// macOS: the native module that gives the window Safari's window type (round
// macOS 26 corners, traffic lights placed by macOS). Built by `npm run build:native`.
// Elsewhere, or if it isn't built, the window keeps Electron's own style.
const windowStyle: { useToolbarWindow(handle: Buffer): Chrome | null } | null = (() => {
  if (process.platform !== 'darwin') return null;
  try {
    return createRequire(import.meta.url)(join(__dirname, '../../native/window-style/build/Release/window_style.node'));
  } catch (err) {
    console.warn('window-style native module not loaded:', (err as Error).message);
    return null;
  }
})();

// In dev, keep drawing while the window is behind others, so automated checks
// (scroll, animations, timers) see the same thing a visible window does.
const dev = !!process.env.ELECTRON_RENDERER_URL;
if (dev) app.commandLine.appendSwitch('disable-backgrounding-occluded-windows');

// the size the window opens at
const WIDTH = 994, HEIGHT = 670;
// while setting up (the page's FirstRun.tsx, Sangam's): narrower, and held at that size; done,
// it grows to WIDTH, around its middle
const SETUP_WIDTH = 700;

let win: BrowserWindow | null = null;
let chrome: Chrome | null = null;
let tracker: ReturnType<typeof setInterval> | null = null;
let last = '';

// obid, through main (obid.ts): only for the window's own page, never a frame in it; and its
// Confirm and the owner's words, signed with this Mac's device key (device.ts)
const trusted: Trusted = (e) => !!win && e.sender === win.webContents && e.senderFrame === win.webContents.mainFrame;
bridge(trusted, ownersWords);
confirming(trusted);
const channels = ipc(trusted);
// the page asks for both before it draws, to lay its title bar out around the lights
channels.on('window-chrome', [], () => chrome, null);
channels.on('full-screen', [], () => win?.isFullScreen() ?? false, false);
// setting up, or not: the page says so (synchronously, before the window first shows) and the
// window takes the setup's size, held there (no resizing, zooming or full screen), or its own back.
// The answer: the width it's on its way to, so the page knows when it's there (null: it's there)
channels.on('setup-window', [flag], (_e, setup): number | null => {
  if (!win || win.isDestroyed() || win.isFullScreen()) return null;
  const b = win.getBounds(), width = setup ? SETUP_WIDTH : WIDTH;
  let answer: number | null = null;
  win.setResizable(true);
  if (b.width !== width) {
    win.setBounds({ x: Math.round(b.x + (b.width - width) / 2), y: b.y, width, height: b.height }, win.isVisible());
    answer = width;
  }
  win.setResizable(!setup);
  win.setMaximizable(!setup);
  win.setFullScreenable(!setup);
  return answer;
}, null);
// a sign-in page (an agent's, GitHub's, Tetrix's) in the person's browser: https only
channels.on('open-external', [https], (_e, url) => { void shell.openExternal(url); });
// the folders the work is in (the message box's Select working folder…): the Mac's own folder
// picker over the window, several at once; the paths picked, none when it's cancelled
// (from: where it opens, the folder the last one was picked in, when it's an absolute path)
const folder = (from: unknown) => ({ value: typeof from === 'string' && from.startsWith('/') ? from : undefined });
channels.handle('pick-folders', [folder], async (_e, from) => {
  if (!win || win.isDestroyed()) return [];
  const r = await dialog.showOpenDialog(win, {
    properties: ['openDirectory', 'multiSelections', 'createDirectory'], buttonLabel: 'Select', message: 'Choose the folders the work is in',
    ...(from !== undefined ? { defaultPath: from } : {}),
  });
  if (!r.canceled) picked(r.filePaths);
  return r.canceled ? [] : r.filePaths;
}, []);

function track(on: boolean) {
  if (tracker) clearInterval(tracker);
  tracker = null;
  if (!on) return;
  // 60 times a second, twice Oby's frame rate, so every frame it draws has a
  // fresh position; only sends when it moved
  tracker = setInterval(() => {
    if (!win || win.isDestroyed()) return;
    const p = screen.getCursorScreenPoint();
    const b = win.getContentBounds();
    const key = `${p.x - b.x},${p.y - b.y}`;
    if (key === last) return;
    last = key;
    win.webContents.send('cursor', { x: p.x - b.x, y: p.y - b.y });
  }, 1000 / 60);
}

function createWindow() {
  win = new BrowserWindow({
    width: WIDTH,
    height: HEIGHT,
    minWidth: 420,
    minHeight: 520,
    show: false,
    backgroundColor: '#0b0b0b',
    // A native window with its title bar hidden; the native module then makes
    // it a toolbar window (see windowStyle), and handles full screen for it.
    titleBarStyle: 'hidden',
    // the click that brings the window forward from another app also reaches
    // the page: without it, macOS keeps that click, Chromium puts the focus back
    // in the message box, and a click outside it seems to do nothing
    acceptFirstMouse: true,
    // scrollBounce: macOS's rubber-band at the ends of a scroll (off by default in Electron).
    // sandbox: the page's process, the preload's world too, has no Node and runs in Chromium's
    // sandbox, so a script that takes it over reaches only what the preload hands main over IPC
    webPreferences: { preload: join(__dirname, '../preload/index.cjs'), sandbox: true, contextIsolation: true, nodeIntegration: false, backgroundThrottling: !dev, scrollBounce: true },
  });

  chrome = windowStyle?.useToolbarWindow(win.getNativeWindowHandle()) ?? null;
  if (chrome) {
    // keep the traffic lights where macOS put them for this window type;
    // otherwise Electron moves them back to its own default spot
    win.setWindowButtonPosition({ x: Math.round(chrome.lightsLeft), y: Math.round(chrome.lightsTop) });
    // adding the toolbar grows the window by its height; size it back
    win.setSize(WIDTH, HEIGHT);
  }
  win.once('ready-to-show', () => win?.show());
  // stop watching the cursor while nobody can see Oby
  win.on('show', () => track(true));
  win.on('restore', () => track(true));
  win.on('hide', () => track(false));
  win.on('minimize', () => track(false));
  win.on('closed', () => { track(false); stopFollowing(); win = null; windowLeft(); });
  // Oby's alerts (notify.ts): none for what the window shows while it's in front
  win.on('focus', windowSeen);
  for (const ev of ['blur', 'hide', 'minimize'] as const) win.on(ev as 'blur', windowLeft);
  // the page holds obid's API through main: it never leaves for another page or opens one (in dev,
  // only to itself with other words after ?, like ?welcome)
  win.webContents.on('will-navigate', (e, url) => {
    if (!dev || new URL(url).origin !== new URL(win!.webContents.getURL()).origin) e.preventDefault();
  });
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  // the owner's real Enter and clicks, which the page's own process can't make: each lets main sign
  // one request of the owner's words right after it (words.ts, device.ts)
  win.webContents.on('before-input-event', (_, i) => {
    if (i.type !== 'keyDown' || i.isAutoRepeat) return;
    if (i.key === 'Enter') input();
    if (i.code === 'Space') space();
  });
  win.webContents.on('before-mouse-event', (_, m) => { if (m.type === 'mouseDown') input(); });
  // full screen hides the traffic lights: the page moves its title bar over.
  // Besides entering and leaving, re-check whenever the window comes back
  // (switching Spaces between full-screen apps), so the page never drifts.
  const sendFullScreen = () => { if (win && !win.isDestroyed()) win.webContents.send('full-screen', win.isFullScreen()); };
  for (const ev of ['enter-full-screen', 'leave-full-screen', 'focus', 'blur', 'show', 'resize'] as const) win.on(ev as 'focus', sendFullScreen);

  if (dev) win.loadURL(process.env.ELECTRON_RENDERER_URL!);
  else win.loadFile(join(__dirname, '../renderer/index.html'));
}

app.whenReady().then(() => {
  // the Dock icon while developing (a packaged app takes apps/icon/AppIcon.icon, electron-builder.yml)
  if (process.platform === 'darwin' && !app.isPackaged) app.dock?.setIcon(join(__dirname, '../../../icon/exports/oby-1024.png'));
  if (process.platform === 'darwin') Menu.setApplicationMenu(Menu.buildFromTemplate(menu));
  startObid();
  // the window, opened again if it was closed, in front
  const front = () => {
    if (!win) createWindow();
    if (win!.isMinimized()) win!.restore();
    win!.show();
    win!.focus();
    return win!;
  };
  // a clicked alert opens it
  notifying(trusted, () => win, front);
  // Oby's own updates (updates.ts), told to About while the window is there
  updating(trusted, () => win);
  // The window opens only while Oby is the active app (ALERTS-3). obid's `open -g -j … --background`
  // starts it with none, and a running Oby that `open` reaches gets activate (a reopen) without
  // becoming active: neither opens it. The owner's Dock click or app switch makes Oby active, which
  // does. ponytail: a reopen while Oby is already the active app with no window (just closed, still in
  // front) opens it, as a Dock click then would; telling those apart needs a mark from obid's `open`
  let active = false;
  app.on('did-resign-active', () => { active = false; });
  app.on('did-become-active', () => { active = true; if (!win) createWindow(); });
  if (!process.argv.includes('--background')) createWindow();
  // an Obysphere's invite link (sphere.ts), joined on the owner's click
  sphering(trusted, front);
  app.on('activate', () => { if (!win && active) createWindow(); });
});

app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
