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
// as input). `main`: another build's out/main/index.js (an older checkout's, to see a check fail on
// the code before its fix); this checkout's by default.
export function launch({ dir, userData, env, hidden = true, main = join(appRoot, 'out/main/index.js') }) {
  const launcher = join(dir, 'electron-launcher.cjs');
  writeFileSync(launcher, `const { app } = require('electron');
app.setPath('userData', ${JSON.stringify(userData)});
${hidden ? `app.dock?.hide();
app.on('browser-window-created', (_e, win) => { win.setOpacity(0); win.setFocusable(false); win.setIgnoreMouseEvents(true); });` : ''}
import(${JSON.stringify(pathToFileURL(main).href)});
`);
  return electron.launch({ executablePath: electronBinary, args: [launcher], env });
}

// First run marked done in the page's storage (`oby.onboarded`, `oby.setUp`, `oby.welcomed`), then the page
// loaded again with it. On its first start the page reloads itself once, as it opens its profile
// (docs/contracts/profiles.md, PROFILES-41 and 42), which can take the page away under a call: tried again
// until both the marks and the reload hold.
export async function skipFirstRun(page) {
  await until('first run marked done', async () => {
    await page.waitForLoadState('domcontentloaded');
    await page.evaluate(() => { for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1'); });
    await page.reload();
    return page.evaluate(() => localStorage.getItem('oby.onboarded') === '1');
  }, 60_000, 250);
}
