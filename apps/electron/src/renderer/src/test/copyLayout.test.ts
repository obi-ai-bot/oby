import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser, Locator, Page } from 'playwright-core';
import { chrome, openMock, type Mock } from './layout/mock';
import { measure, say } from './layout/measure';

// CHATFIRST-37, as a person sees it: in every conversation of the mock, a message shows one Copy at a
// time, the one for the words under the pointer (an agent's under its card, else the reply's), never two
// Copy icons near each other, and no Copy lies on a button, a link or a task's card, at a normal window and a
// narrow one. With the pointer on the message, then on each of an agent's words in it (the owner's
// "Fix CI" case: Oby's reply, a task card whose agent's last words carry their own Copy, the next message).
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); });
afterAll(async () => { if (typeof browser !== 'string') await browser.close(); });

const ci = !!process.env.CI;
const MESSAGES = '.chat-layer:not(.is-leaving):not(.is-covered) [role=listitem].msg';

async function eachSpot(page: Page, f: (where: string) => Promise<void>) {
  const titles = await page.locator('.convo-row .connector-name').allInnerTexts();
  expect(titles.length).toBeGreaterThan(10);
  for (const [r, title] of titles.entries()) {
    await page.locator('.convo-row').nth(r).click();
    await page.waitForTimeout(150);
    const n = await page.locator(MESSAGES).count();
    for (let i = 0; i < n; i++) {
      const m = page.locator(MESSAGES).nth(i);
      await m.scrollIntoViewIfNeeded().catch(() => {});
      const spots: Locator[] = [m, ...(await m.locator('.ss-said').all())];
      for (const [k, spot] of spots.entries()) {
        await spot.hover({ force: true, position: { x: 8, y: 6 } }).catch(() => {});
        await m.evaluate((e) => e.setAttribute('data-measuring', ''));
        await f(`"${title.trim()}" message ${i + 1}${k ? `, the agent's words ${k}` : ''}`);
        await m.evaluate((e) => e.removeAttribute('data-measuring'));
      }
    }
  }
}

describe('a reply\'s Copy, laid out (CHATFIRST-37)', () => {
  for (const width of [1280, 900]) {
    it(`CHATFIRST-37: one Copy shows at a time, none near another, none on a control or a card, at ${width}px`, async (t) => {
      if (typeof browser === 'string') {
        if (ci) throw new Error(browser);
        console.warn(`skipped: ${browser}`);
        return t.skip();
      }
      let mock: Mock | undefined;
      try {
        mock = await openMock(browser, { width });
        const found: string[] = [];
        let spots = 0, copiesSeen = 0, one = 0;
        await eachSpot(mock.page, async (where) => {
          const r = await measure(mock!.page, '[data-measuring]');
          spots++;
          copiesSeen += await mock!.page.locator('[data-measuring] button[aria-label="Copy"]').count();
          // the Copies showing with the pointer here: one at most
          const shown = await mock!.page.evaluate(() => [...document.querySelectorAll('[data-measuring] button[aria-label="Copy"]')]
            .filter((e) => { const b = e.getBoundingClientRect(); if (!b.width) return false; for (let a: Element | null = e; a; a = a.parentElement) if (Number(getComputedStyle(a).opacity) < 0.05) return false; return true; }).length);
          if (shown > 1) found.push(`${where}: ${shown} Copies show at once`);
          if (shown === 1) one++;
          for (const line of say({ overlaps: [], copiesNear: r.copiesNear, copyOver: r.copyOver })) found.push(`${where}: ${line}`);
        });
        // it looked at something: messages, and Copies in them
        expect(spots).toBeGreaterThan(40);
        expect(copiesSeen).toBeGreaterThan(20);
        // and a Copy does show where the pointer is on words that have one
        expect(one).toBeGreaterThan(20);
        expect([...new Set(found)]).toEqual([]);
      } finally {
        await mock?.close();
      }
    }, 240_000);
  }
});
