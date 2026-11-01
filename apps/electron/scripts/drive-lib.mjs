// What test-core.mjs and human-drive.mjs both need: waiting on a condition, and launching the built
// app (out/main/index.js) under Playwright with an app folder of its own.
import { writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { _electron as electron } from 'playwright-core';

export const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const electronBinary = join(appRoot, 'node_modules/electron/dist/Electron.app/Contents/MacOS/Electron');
export const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

export async function until(label, f, ms = 60_000, every = 150) {
  const deadline = Date.now() + ms;
  let last;
  while (Date.now() < deadline) {
    try { const value = await f(); if (value) return value; } catch (e) { last = e; }
    await sleep(every);
  }
  throw new Error(`${label} timed out${last ? `: ${last.message}` : ''}`);
}

// The built app with `userData` as its app folder (its device key and settings), through a launcher
// written in `dir`. Hidden: never on the person's screen, no Dock icon and every window invisible
// and unfocusable (Playwright drives it through DevTools, whose key and mouse events main still sees
// as input).
export function launch({ dir, userData, env, hidden = true }) {
  const launcher = join(dir, 'electron-launcher.cjs');
  writeFileSync(launcher, `const { app } = require('electron');
app.setPath('userData', ${JSON.stringify(userData)});
${hidden ? `app.dock?.hide();
app.on('browser-window-created', (_e, win) => { win.setOpacity(0); win.setFocusable(false); win.setIgnoreMouseEvents(true); });` : ''}
import(${JSON.stringify(pathToFileURL(join(appRoot, 'out/main/index.js')).href)});
`);
  return electron.launch({ executablePath: electronBinary, args: [launcher], env });
}
