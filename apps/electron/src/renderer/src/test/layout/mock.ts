// The app's screens as a person sees them: the renderer on its mock (Vite, `npm run web`), laid out by a
// real browser (Chrome, through playwright-core), so a test can measure where things are drawn. happy-dom
// lays nothing out, so an overlap only shows here (docs/verification.md: driven from outside).
//
// Chrome: the one installed (`channel: 'chrome'`, GitHub's ubuntu runner has it), or OBY_CHROME. Without
// one the layout tests say so and are skipped on a person's Mac; in CI they fail, never pass on nothing.
import { createServer as listen } from 'node:net';
import { resolve } from 'node:path';
import { chromium, type Browser, type Page } from 'playwright-core';
import { createServer, type ViteDevServer } from 'vite';

const appRoot = resolve(__dirname, '../../../../..');

export interface Mock { page: Page; close: () => Promise<void> }

const freePort = () => new Promise<number>((done, fail) => {
  const s = listen();
  s.once('error', fail);
  s.listen(0, '127.0.0.1', () => { const { port } = s.address() as { port: number }; s.close(() => done(port)); });
});

// Chrome, or why there is none
export async function chrome(): Promise<Browser | string> {
  try {
    const executablePath = process.env.OBY_CHROME;
    return await chromium.launch(executablePath ? { executablePath } : { channel: 'chrome' });
  } catch (e) {
    return `no Chrome to lay the screens out (${(e as Error).message.split('\n')[0]}); set OBY_CHROME to one`;
  }
}

// The mock at `query` (`?everything&chat` by default), first run and its welcome done, in a window of
// `width` × `height`
export async function openMock(browser: Browser, { query = '?everything&chat', width = 1280, height = 820 } = {}): Promise<Mock> {
  const port = await freePort();
  const server: ViteDevServer = await createServer({
    configFile: resolve(appRoot, 'vite.config.ts'),
    logLevel: 'error',
    server: { port, strictPort: true, hmr: false, host: '127.0.0.1' },
  });
  await server.listen();
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
  return { page, close: async () => { await context.close(); await server.close(); } };
}

// A conversation of the side panel, opened by its title
export async function openChat(page: Page, title: string) {
  await page.locator('.convo-row', { has: page.locator('.connector-name', { hasText: title }) }).first().click();
  await page.waitForTimeout(500);
}
