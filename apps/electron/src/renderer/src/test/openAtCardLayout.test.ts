import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser, Page } from 'playwright-core';
import { CHROME_HOOK_MS, chrome, openMock, type Mock } from './layout/mock';

// A nested row opens its conversation at the task's card, lit (docs/contracts/live-task.md, LIVETASK-22,
// LIVETASK-28), and the card stays in view above the drawer over the box, however tall the drawer grows:
// a question with described options makes it a third of the window (#2786). The drawer takes its own
// row under the thread, so as it grows the thread's room shrinks from the bottom; a card at the thread's
// end went out under it, Oby's message above it cut at the drawer's edge. Measured in a real Chrome, in
// the window the issue was seen in (994 × 670).
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); }, CHROME_HOOK_MS);
afterAll(async () => { if (typeof browser !== 'string') await browser.close(); });

const ci = !!process.env.CI;
// the open conversation's thread, its lit card, and the drawer, as drawn
const drawn = (page: Page) => page.evaluate(() => {
  const at = '.chat-layer:not(.is-leaving):not(.is-covered)';
  const box = (e: Element | null) => { const r = e?.getBoundingClientRect(); return r ? { top: Math.round(r.top), bottom: Math.round(r.bottom), height: Math.round(r.height) } : null; };
  return {
    chat: box(document.querySelector(`${at} .chat`)),
    card: box(document.querySelector(`${at} .msg-task-work.is-lit > .msg-task`)),
    bar: box(document.querySelector(`${at} .decision-bar`)),
  };
});

describe('opening a conversation at a task, laid out', () => {
  it('LIVETASK-28: a nested row opens its conversation with the task\'s card in view, lit, above a drawer that grows tall', async (t) => {
    if (typeof browser === 'string') {
      if (ci) throw new Error(browser);
      console.warn(`skipped: ${browser}`);
      return t.skip();
    }
    let mock: Mock | undefined;
    try {
      mock = await openMock(browser, { width: 994, height: 670 });
      const { page } = mock;
      // the task that asks to run a command: its card is at its conversation's end
      await page.locator('.convo-kid', { hasText: 'Virtualize the side panel' }).click();
      await page.locator('.msg-task-work.is-lit > .msg-task').waitFor();
      await page.waitForTimeout(400);
      const opened = await drawn(page);
      expect(opened.bar, 'its decision waits in the drawer').not.toBeNull();
      expect(opened.card!.top).toBeGreaterThanOrEqual(opened.chat!.top);
      expect(opened.card!.bottom).toBeLessThanOrEqual(opened.chat!.bottom);
      // the drawer grows to a third of the window, as a question with two described options makes it
      await page.addStyleTag({ content: '.decision-card { min-height: 260px; box-sizing: border-box; }' });
      await page.waitForTimeout(400);
      const grown = await drawn(page);
      expect(grown.bar!.height, 'the drawer grew').toBeGreaterThan(opened.bar!.height + 100);
      expect(grown.card, 'the card is still lit').not.toBeNull();
      expect(grown.card!.top, 'its card in the thread, under its top').toBeGreaterThanOrEqual(grown.chat!.top);
      expect(grown.card!.bottom, `its card above the drawer (card ${JSON.stringify(grown.card)}, thread ${JSON.stringify(grown.chat)})`).toBeLessThanOrEqual(grown.chat!.bottom);
    } finally {
      await mock?.close();
    }
  }, 120_000);
});
