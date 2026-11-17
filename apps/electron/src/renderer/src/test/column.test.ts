import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

// CHATFIRST-34: one width for the thread and its box, set once in tokens.css. The CSS is the contract,
// so this reads it: no layout engine here to measure.
const css = (f: string) => readFileSync(join(__dirname, '..', f), 'utf8');
const rule = (file: string, selector: string) => {
  const text = css(file), at = text.indexOf(`${selector} {`);
  expect(at, `${selector} in ${file}`).toBeGreaterThanOrEqual(0);
  return text.slice(at, text.indexOf('}', at));
};

describe('the conversation column (CHATFIRST-34)', () => {
  it('chatfirst_34_column_grows_from_540_to_768', () => {
    const tokens = css('tokens.css');
    expect(tokens).toMatch(/--column:\s*clamp\(540px,\s*calc\(540px \+ \(100vw - 994px\) \* 0\.6\),\s*768px\);/);
  });

  it('chatfirst_34_thread_and_box_share_it', () => {
    for (const [file, selector] of [
      ['app.css', '.chat-column'], ['app.css', '.chat-composer'], ['app.css', '.chat-ask'],
      ['liveTask.css', '.agent-layer .lt-thread-column'], ['liveTask.css', '.agent-layer .chat-composer.lt-box'],
    ] as const) {
      const r = rule(file, selector);
      expect(r, `${selector} uses --column`).toContain('var(--column)');
      expect(r, `${selector} has no fixed 540px`).not.toMatch(/\b540px\b/);
    }
  });

  it('chatfirst_34_home_box_and_its_cards_follow_it', () => {
    expect(rule('app.css', '.composer')).toContain('var(--column) - 20px');
    expect(rule('app.css', '.asks')).toContain('var(--column) - 60px');
  });
});
