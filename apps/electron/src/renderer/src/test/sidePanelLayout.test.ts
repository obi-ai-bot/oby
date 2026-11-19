import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser } from 'playwright-core';
import { chrome, openMock, type Mock } from './layout/mock';
import { measure, say } from './layout/measure';

// The side panel as a person sees it: New conversation's ⌘N tip (docs/contracts/shortcuts.md, SHORTCUTS-6)
// holds no room while hidden, so the row reads whole, and no control in the panel lies on another, from a
// normal window down to the narrowest that still docks it (892px, 252px wide: chat-first.md, CHATFIRST-35).
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); });
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
        const rest = await label.evaluate((e) => ({ text: e.textContent, cut: e.scrollWidth - e.clientWidth }));
        expect(rest.text).toBe('New conversation');
        expect(rest.cut, 'its words cut short with nothing beside them').toBeLessThanOrEqual(0);
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
});
