import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser, Page } from 'playwright-core';
import { CHROME_HOOK_MS, chrome, openMock, type Mock } from './layout/mock';
import { measure, say } from './layout/measure';

// ⌘F's find bar as a person sees it (docs/contracts/shortcuts.md, SHORTCUTS-3): in every conversation of
// the mock, open, with words typed, it lies on no message (the first one at the thread's top was under it,
// your own on the right most of all) and never on the match it went to; nor on any control. At a normal
// window and a narrow one.
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); }, CHROME_HOOK_MS);
afterAll(async () => { if (typeof browser !== 'string') await browser.close(); });

const ci = !!process.env.CI;
const LAYER = '.chat-layer:not(.is-leaving):not(.is-covered)';

// what of the thread lies under the bar: its messages' words, cards and times, and the current match
const under = (page: Page) => page.evaluate((layer) => {
  const bar = document.querySelector(`${layer} .find-bar`)?.getBoundingClientRect();
  if (!bar) return ['no find bar'];
  const scroller = document.querySelector(`${layer} .chat`)!.getBoundingClientRect();
  const hit = (r: DOMRect) => r.width > 0 && r.height > 0 && r.bottom > scroller.top + 1 && r.top < scroller.bottom
    && Math.min(r.right, bar.right) - Math.max(r.left, bar.left) > 1 && Math.min(r.bottom, bar.bottom) - Math.max(r.top, bar.top) > 1;
  const out: string[] = [];
  for (const e of document.querySelectorAll(`${layer} .chat-thread :is(.msg-bubble, .msg-task, .msg-time, .msg-face, .peer-by, p, li)`)) {
    // words: the boxes of its lines, not its whole width
    const range = document.createRange();
    range.selectNodeContents(e);
    for (const r of range.getClientRects()) if (hit(r)) { out.push(`${e.className || e.tagName}: "${(e.textContent ?? '').trim().slice(0, 40)}"`); break; }
  }
  const cur = (CSS as unknown as { highlights?: Map<string, { forEach: (f: (r: Range) => void) => void }> }).highlights?.get('oby-find-current');
  cur?.forEach((r) => { for (const b of r.getClientRects()) if (hit(b)) out.push('the current match'); });
  return [...new Set(out)];
}, LAYER);

describe('the find bar, laid out (SHORTCUTS-3)', () => {
  for (const width of [1280, 900]) {
    it(`SHORTCUTS-3: open, it lies on no message nor the match it went to, at ${width}px`, async (t) => {
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
        expect(titles.length).toBeGreaterThan(10);
        const found: string[] = [];
        let opened = 0;
        for (const [r, title] of titles.entries()) {
          await page.locator('.convo-row').nth(r).click();
          await page.waitForTimeout(150);
          if (!(await page.locator(`${LAYER} .chat-thread`).count())) continue;
          await page.locator(`${LAYER} .chat`).click({ position: { x: 4, y: 4 } });
          await page.keyboard.press('ControlOrMeta+f');
          const bar = page.locator(`${LAYER} .find-bar`);
          await bar.waitFor();
          opened++;
          await page.keyboard.type('the');
          await page.waitForTimeout(120);
          // where it went to, then the thread's very top: nothing of it under the bar
          for (const where of ['at the match', 'at the top']) {
            if (where === 'at the top') await page.locator(`${LAYER} .chat`).evaluate((e) => { e.scrollTop = 0; });
            for (const what of await under(page)) found.push(`"${title}" ${where}: ${what}`);
          }
          const m = await measure(page, LAYER);
          for (const line of say({ overlaps: m.overlaps, copiesNear: [], copyOver: [] })) found.push(`"${title}": ${line}`);
          await page.keyboard.press('Escape');
          await bar.waitFor({ state: 'detached' });
        }
        expect(opened).toBeGreaterThan(10);
        expect(found).toEqual([]);
      } finally {
        await mock?.close();
      }
    }, 240_000);
  }
});
