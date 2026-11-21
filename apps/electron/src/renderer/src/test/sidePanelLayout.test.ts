import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser } from 'playwright-core';
import { CHROME_HOOK_MS, chrome, openMock, type Mock } from './layout/mock';
import { measure, say } from './layout/measure';

// The side panel as a person sees it: New conversation's ⌘N tip (docs/contracts/shortcuts.md, SHORTCUTS-6)
// holds no room while hidden, so the row reads whole, and no control in the panel lies on another, from a
// normal window down to the narrowest that still docks it (892px, 252px wide: chat-first.md, CHATFIRST-35).
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); }, CHROME_HOOK_MS);
afterAll(async () => { if (typeof browser !== 'string') await browser.close(); });

const ci = !!process.env.CI;
// a row's expand chevron sits in its own row's left margin, on purpose (Sidebar.tsx, convo-twist)
const meant = (line: string) => /convo-twist/.test(line);

describe('the side panel, laid out', () => {
  for (const width of [1280, 892]) {
    it(`SHORTCUTS-6: New conversation reads whole at rest, its ⌘N only on the pointer, at ${width}px`, async (t) => {
      if (typeof browser === 'string') {
        if (ci) throw new Error(browser);
        console.warn(`skipped: ${browser}`);
        return t.skip();
      }
      let mock: Mock | undefined;
      try {
        mock = await openMock(browser, { width });
        const { page } = mock;
        const row = page.locator('.sidebar .sidebar-new.chat-new').first();
        const label = row.locator('.sidebar-new-label');
        await page.mouse.move(width - 10, 400);
        const panel = await page.locator('.sidebar').boundingBox();
        // it docks at its narrowest here: the case this guards
        if (width === 892) expect(panel!.width).toBeLessThanOrEqual(256);
        // at rest: its words cut only where the row has no more room for them, never beside an empty
        // space the hidden tip holds (the row's flex gap, 10px, is all that may lie past the label). How
        // wide the words are is the font's (a Mac's fits them whole at 252px; Linux's draws them wider)
        const rest = await row.evaluate((e) => {
          const l = e.querySelector('.sidebar-new-label')!, r = e.getBoundingClientRect(), s = getComputedStyle(e);
          return { text: l.textContent, cut: l.scrollWidth - l.clientWidth, empty: r.right - parseFloat(s.paddingRight) - l.getBoundingClientRect().right };
        });
        expect(rest.text).toBe('New conversation');
        if (rest.cut > 0) expect(rest.empty, `its words cut short by ${rest.cut}px beside ${Math.round(rest.empty)}px of empty room`).toBeLessThanOrEqual(12);
        // with the pointer on it, the shortcut shows, in room of its own
        await row.hover();
        const hint = await row.evaluate((e) => { const s = getComputedStyle(e, '::after'); return { opacity: s.opacity, content: s.content, room: s.maxWidth }; });
        expect(hint.opacity).toBe('1');
        expect(hint.content).toMatch(/N/);
        expect(hint.room).not.toBe('0px');
        await page.mouse.move(width - 10, 400);
        // nothing in the panel drawn on anything else
        const r = await measure(page, '.sidebar');
        expect(say({ overlaps: r.overlaps, copiesNear: r.copiesNear, copyOver: r.copyOver }).filter((l) => !meant(l))).toEqual([]);
        expect(r.clipped).toEqual([]);
      } finally {
        await mock?.close();
      }
    }, 120_000);
  }

  // The drawn key is CSS content, which a browser counts in a control's accessible name unless it has alt
  // text of its own (#2789): the key is told once, by aria-keyshortcuts, and the name is the row's words.
  it('SHORTCUTS-6: the drawn ⌘N and ⌘, are no part of New conversation\'s and Settings\' names', async (t) => {
    if (typeof browser === 'string') {
      if (ci) throw new Error(browser);
      console.warn(`skipped: ${browser}`);
      return t.skip();
    }
    let mock: Mock | undefined;
    try {
      mock = await openMock(browser, { width: 1280 });
      const { page } = mock;
      const row = page.locator('.sidebar .sidebar-new.chat-new').first();
      await row.hover();
      expect(await row.evaluate((e) => getComputedStyle(e, '::after').opacity)).toBe('1');
      expect(await page.getByRole('button', { name: 'New conversation', exact: true }).count()).toBe(1);
      expect(await row.getAttribute('aria-keyshortcuts')).toBeTruthy();
      await page.locator('.rail-account').click();
      const settings = page.locator('[role="menuitem"].has-kbd');
      await settings.waitFor();
      expect(await settings.evaluate((e) => getComputedStyle(e, '::after').content)).toMatch(/,/);
      expect(await page.getByRole('menuitem', { name: 'Settings', exact: true }).count()).toBe(1);
    } finally {
      await mock?.close();
    }
  }, 120_000);
});
