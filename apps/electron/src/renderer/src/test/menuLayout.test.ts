import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser, Page } from 'playwright-core';
import { CHROME_HOOK_MS, chrome, openMock, type Mock } from './layout/mock';
import { measure } from './layout/measure';

// A menu stays inside the window (Menu.tsx: menuPlace up and down, menuShift sideways), as a person sees
// it: each of the message box's menus (docs/contracts/composer.md: its +, its Mode, the talk pill, the
// usage gauge) and the side panel's (the profile's, a conversation's ⋮), opened at a normal window and a
// narrow one, lies wholly inside it, 8px from its edges, with none of its words cut.
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); }, CHROME_HOOK_MS);
afterAll(async () => { if (typeof browser !== 'string') await browser.close(); });

const ci = !!process.env.CI;
const LAYER = '.chat-layer:not(.is-leaving):not(.is-covered)';
const OPENERS: [string, string][] = [
  ['the message box\'s +', `${LAYER} button[aria-label="Add to the message"]`],
  ['the message box\'s Mode', `${LAYER} button[aria-label^="Mode for tasks"]`],
  ['the talk pill', `${LAYER} button[aria-label^="Talking through"]`],
  ['the usage gauge', `${LAYER} button[aria-label^="Usage"]`],
  ['the profile\'s', '.sidebar button[aria-haspopup="menu"]:has(svg) >> nth=0'],
  ['a conversation\'s ⋮', '.sidebar .sidebar-item-more >> nth=0'],
];

// every menu or panel drawn over the page, where it is
const popups = (page: Page) => page.evaluate(() => [...document.querySelectorAll('.row-menu, [role=menu], .usage-panel, [role=dialog]')]
  .map((e) => ({ e, r: e.getBoundingClientRect() }))
  .filter(({ r }) => r.width > 0 && r.height > 0)
  .map(({ e, r }) => ({ name: (e.getAttribute('aria-label') ?? e.className).toString().slice(0, 40), left: r.left, right: r.right, top: r.top, bottom: r.bottom, w: innerWidth, h: innerHeight })));

describe('a menu, laid out', () => {
  it('COMPOSER-1: menuShift: a menu running past the window\'s edge moves back inside it, 8px from it', async () => {
    const { menuShift } = await import('../Menu');
    expect(menuShift(690, 970, 900)).toBe(-78);   // the Mode menu in a 900px window
    expect(menuShift(100, 380, 900)).toBe(0);
    expect(menuShift(-20, 260, 900)).toBe(28);
    // wider than the window: its left edge kept in, as a page's start is
    expect(menuShift(10, 1000, 900)).toBe(-2);
  });

  for (const width of [1280, 900]) {
    it(`COMPOSER-1: every menu of the message box and the side panel opens wholly inside the window, at ${width}px`, async (t) => {
      if (typeof browser === 'string') {
        if (ci) throw new Error(browser);
        console.warn(`skipped: ${browser}`);
        return t.skip();
      }
      let mock: Mock | undefined;
      try {
        mock = await openMock(browser, { width });
        const { page } = mock;
        const found: string[] = [];
        let opened = 0;
        for (const [name, opener] of OPENERS) {
          await page.locator(opener).first().click();
          await page.waitForTimeout(250);
          const shown = await popups(page);
          if (!shown.length) { found.push(`${name}: nothing opened`); continue; }
          opened++;
          for (const m of shown) {
            if (m.left < 8 - 0.5 || m.right > m.w - 8 + 0.5 || m.top < 0 || m.bottom > m.h) {
              found.push(`${name} (${m.name}) runs past the window: ${Math.round(m.left)}–${Math.round(m.right)} of ${m.w}px wide, ${Math.round(m.top)}–${Math.round(m.bottom)} of ${m.h}px tall`);
            }
          }
          const r = await measure(page, '.row-menu, [role=menu]');
          for (const c of r.clipped) found.push(`${name}: words cut: ${c.what} "${c.text}" ${c.over}`);
          await page.keyboard.press('Escape');
          await page.waitForTimeout(150);
        }
        expect(found).toEqual([]);
        expect(opened).toBe(OPENERS.length);
      } finally {
        await mock?.close();
      }
    }, 180_000);
  }
});
