import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';

// Copying from a chat pastes plain text, as in Claude Code: an <ol> pastes as "1. … 2. …", even with
// its markers hidden (docs/contracts/chat-first.md, CHATFIRST-32).
describe('a chat copies as plain text', () => {
  for (const view of ['ChatView', 'RoomView']) {
    it(`CHATFIRST-32: ${view}'s thread is no numbered list`, () => {
      const source = readFileSync(new URL(`../${view}.tsx`, import.meta.url), 'utf8');
      expect(source).toContain('role="list" onCopy={(e) => copyPlain(e)} className="chat-thread');
      expect(source).not.toMatch(/<ol\b[^>]*chat-thread/);
    });
  }

  it('CHATFIRST-32: the clipboard gets the words alone, and an empty selection is left alone', async () => {
    vi.stubGlobal('location', { search: '' });
    vi.stubGlobal('window', { app: { obid: { call: async () => ({ status: 404, text: '' }) } } });
    const { copyPlain } = await import('../room');
    const setData = vi.fn(), preventDefault = vi.fn();
    copyPlain({ clipboardData: { setData }, preventDefault }, 'Can you ask?\nI could not answer.');
    expect(setData.mock.calls).toEqual([['text/plain', 'Can you ask?\nI could not answer.']]);
    expect(preventDefault).toHaveBeenCalledOnce();
    const untouched = vi.fn();
    copyPlain({ clipboardData: { setData }, preventDefault: untouched }, '');
    expect(untouched).not.toHaveBeenCalled();
  });
});
