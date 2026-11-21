import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Browser } from 'playwright-core';
import { CHROME_HOOK_MS, chrome, openMock, type Mock } from './layout/mock';
import { measure, say } from './layout/measure';

// Oby's first conversation, setting up (onboarding.ts, SetupCard.tsx), as a person sees it, and where a
// reply's Copy goes (docs/contracts/chat-first.md, CHATFIRST-37: words that end their message, under them):
// played by the control center to a task waiting on a merge, every step answered. A step that was only
// its picks (the first task's, the Obysphere's) leaves no empty card behind its words, so they end their
// message: their Copy under them, no blank row of its own under them, and no gap before the next message.
let browser: Browser | string;
beforeAll(async () => { browser = await chrome(); }, CHROME_HOOK_MS);
afterAll(async () => { if (typeof browser !== 'string') await browser.close(); });

const ci = !!process.env.CI;
const THREAD = '.chat-layer:not(.is-leaving):not(.is-covered) .chat-thread';

describe('setting up, laid out', () => {
  for (const width of [1280, 900]) {
    it(`CHATFIRST-37: an answered step leaves no empty card nor a row of its own under Oby's words, at ${width}px`, async (t) => {
      if (typeof browser === 'string') {
        if (ci) throw new Error(browser);
        console.warn(`skipped: ${browser}`);
        return t.skip();
      }
      let mock: Mock | undefined;
      try {
        mock = await openMock(browser, { width, query: '?welcome&dev=merge-waiting' });
        const { page } = mock;
        // the script played to its end: the merge's question is up
        await page.locator(THREAD, { hasText: 'Okay. When your team has one' }).waitFor({ timeout: 30_000 });
        const seen = await page.evaluate((thread) => {
          const out: string[] = [];
          let answered = 0;
          for (const msg of document.querySelectorAll(`${thread} > [role=listitem].msg.is-oby`)) {
            const words = (msg.querySelector('.msg-bubble')?.textContent ?? '').trim().slice(0, 40);
            for (const c of msg.querySelectorAll('.setup')) if (c.getBoundingClientRect().height < 1) out.push(`"${words}": an empty card`);
            // a row holding only the message's Copy, with nothing between it and the words
            const foot = msg.querySelector(':scope > .msg-foot, :scope > .msg-end > .msg-foot');
            const before = foot?.previousElementSibling;
            if (foot && before?.classList.contains('msg-bubble')) out.push(`"${words}": its Copy on a row of its own right under its words`);
            if (/^(While Claude Code works|You’re set)/.test(words)) answered++;
          }
          return { out, answered };
        }, THREAD);
        // it looked at the two steps that are only picks, both answered
        expect(seen.answered).toBe(2);
        const r = await measure(page, THREAD);
        expect([...seen.out, ...say({ overlaps: r.overlaps, copiesNear: r.copiesNear, copyOver: r.copyOver })]).toEqual([]);
      } finally {
        await mock?.close();
      }
    }, 120_000);
  }
});
