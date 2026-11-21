// The app's screens as a person sees them: the renderer on its mock (Vite, `npm run web`), laid out by a
// real browser (Chrome, through playwright-core), so a test can measure where things are drawn. happy-dom
// lays nothing out, so an overlap only shows here (docs/verification.md: driven from outside).
//
// Chrome: the one installed (`channel: 'chrome'`, GitHub's ubuntu runner has it), or OBY_CHROME. Without
// one the layout tests say so and are skipped on a person's Mac; in CI they fail, never pass on nothing.
import { createServer as httpServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { resolve } from 'node:path';
import { chromium, type Browser, type Page } from 'playwright-core';
import { createServer, type ViteDevServer } from 'vite';

const appRoot = resolve(__dirname, '../../../../..');

export interface Mock { page: Page; close: () => Promise<void> }

// How long Chrome may take to start: it's waited for until it says it's ready (its DevTools endpoint,
// as Playwright reads it), never cut at vitest's 30 s hook default. The pinned headless shell starts in
// under 1 s at load ~215 on the build VM; GitHub's full Chrome, six at once on 4 cores, took past 30 s
// (run 44766966178). A layout file's beforeAll waits this long, and a bit more
export const CHROME_START_MS = 120_000;
export const CHROME_HOOK_MS = CHROME_START_MS + 10_000;

// Chrome, or why there is none. A slow start says how slow, so the next one is measured
export async function chrome(): Promise<Browser | string> {
  const t = Date.now();
  try {
    const executablePath = process.env.OBY_CHROME;
    const browser = await chromium.launch({ ...(executablePath ? { executablePath } : { channel: 'chrome' as const }), timeout: CHROME_START_MS });
    const ms = Date.now() - t;
    if (ms > 5_000) process.stderr.write(`layout tests: Chrome took ${ms} ms to start\n`);
    return browser;
  } catch (e) {
    return `no Chrome to lay the screens out (${(e as Error).message.split('\n')[0]}); set OBY_CHROME to one`;
  }
}

// The mock at `query` (`?everything&chat` by default), first run and its welcome done, in a window of
// `width` × `height`
export async function openMock(browser: Browser, { query = '?everything&chat', width = 1280, height = 820 } = {}): Promise<Mock> {
  // the mock's own server, on a port the system gives it as it binds and it keeps: a port picked first
  // and freed for Vite could be taken in between by another layout file's ("Port … is already in use"),
  // and Vite reads port 0 as its default 5173. Vite runs inside it, as middleware, its client's
  // websocket on the same server, as it was on Vite's own
  const server = httpServer();
  await new Promise<void>((done, fail) => { server.once('error', fail); server.listen(0, '127.0.0.1', () => done()); });
  const { port } = server.address() as AddressInfo;
  const vite: ViteDevServer = await createServer({
    configFile: resolve(appRoot, 'vite.config.ts'),
    logLevel: 'error',
    appType: 'spa',
    server: { middlewareMode: true, hmr: { server } },
  });
  server.on('request', vite.middlewares);
  const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: 2 });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const base = `http://127.0.0.1:${port}/`;
  await page.goto(base);
  await page.evaluate(() => { for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1'); });
  await page.goto(`${base}${query}`);
  // in development first run always opens (FirstRun.tsx, ALWAYS): closed as its own button closes it
  await page.waitForFunction(() => document.querySelector('#root')?.childElementCount, undefined, { timeout: 60_000 });
  await page.evaluate("import('/src/FirstRun.tsx').then((m) => m.closeOnboarding())");
  // a fade is not what's measured: things are where they end up at once
  await page.addStyleTag({ content: '*, *::before, *::after { transition: none !important; }' });
  await page.waitForTimeout(400);
  if (errors.length) throw new Error(`the mock threw: ${errors.join('; ')}`);
  return { page, close: async () => { await context.close(); await vite.close(); await new Promise<void>((done) => server.close(() => done())); } };
}

// A conversation of the side panel, opened by its title
export async function openChat(page: Page, title: string) {
  await page.locator('.convo-row', { has: page.locator('.connector-name', { hasText: title }) }).first().click();
  await page.waitForTimeout(500);
}
