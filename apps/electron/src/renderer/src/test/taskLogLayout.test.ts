import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser } from 'playwright-core';
import { chrome, openMock, type Mock } from './layout/mock';
import { measure, say } from './layout/measure';

// What a task did, under its card (docs/contracts/live-task.md, LIVETASK-31), as a person sees it:
// its agent's words sit in the log as tightly as its other lines (the log's 4px between entries, not a
// paragraph's own margins on top), and nothing in the log lies on anything
// else. Every conversation of the mock, at a normal window and a narrow one.
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); });
afterAll(async () => { if (typeof browser !== 'string') await browser.close(); });

const ci = !!process.env.CI;
const LAYER = '.chat-layer:not(.is-leaving):not(.is-covered)';
// the room above or below an agent's words, past which it reads as a gap in the log
const ROOM = 16;

describe('a task\'s log, laid out', () => {
  for (const width of [1280, 900]) {
    it(`LIVETASK-31: an agent's words sit in the log as its other lines do, at ${width}px`, async (t) => {
      if (typeof browser === 'string') {
        if (ci) throw new Error(browser);
        console.warn(`skipped: ${browser}`);
        return t.skip();
      }
      let mock: Mock | undefined;
      try {
        mock = await openMock(browser, { width });
        const { page } = mock;
        const titles = await page.locator('.convo-row .connector-name').allInnerTexts();
        const found: string[] = [];
        let words = 0;
        for (const [r, title] of titles.entries()) {
          await page.locator('.convo-row').nth(r).click();
          await page.waitForTimeout(150);
          if (!(await page.locator(`${LAYER} .lt-log`).count())) continue;
          const seen = await page.evaluate(([layer, ROOM]) => {
            const out: string[] = [];
            let n = 0;
            // where text is drawn: its lines' boxes
            const lines = (e: Element) => { const range = document.createRange(); range.selectNodeContents(e); return [...range.getClientRects()].filter((b) => b.width > 0 && b.height > 0); };
            for (const said of document.querySelectorAll(`${layer} .lt-log .ss-said`)) {
              const body = said.querySelector('.lt-fold-body');
              const text = body && lines(body);
              if (!text?.length) continue;
              n++;
              const name = (body!.textContent ?? '').trim().slice(0, 40);
              const top = Math.min(...text.map((b) => b.top)), bottom = Math.max(...text.map((b) => b.bottom));
              const box = said.getBoundingClientRect();
              if (top - box.top > ROOM / 2) out.push(`"${name}": ${Math.round(top - box.top)}px of room above its first line, in its own box`);
              // a fold that's cut keeps its fade and Show more under the words: measure the open ones
              if (!said.querySelector('.lt-fold.is-long:not(.is-open)')) {
                const under = box.bottom - bottom;
                if (under > ROOM / 2) out.push(`"${name}": ${Math.round(under)}px of room under its last line, in its own box`);
              }
            }
            return { out, n };
          }, [LAYER, ROOM] as const);
          words += seen.n;
          for (const line of seen.out) found.push(`"${title}" ${line}`);
          const m = await measure(page, `${LAYER} .lt-log`);
          for (const line of say({ overlaps: m.overlaps, copiesNear: [], copyOver: [] })) found.push(`"${title}": ${line}`);
          for (const c of m.clipped) found.push(`"${title}": cut: ${c.what} "${c.text}" ${c.over}`);
        }
        // it looked at agents' words: the mock has three, under its cards
        expect(words).toBeGreaterThanOrEqual(3);
        expect(found).toEqual([]);
      } finally {
        await mock?.close();
      }
    }, 240_000);
  }
});
