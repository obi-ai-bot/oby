// @vitest-environment happy-dom
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import Said from '../Said';

// T3 Code's fixes to a reply as it renders, on Oby's own (Said.tsx): a reply still streaming renders as
// the finished one will (T3 d05e4f8e8f "streamed replies render a list's code block as the finished reply
// does", 96ffe454cd "streamed replies keep updating after a line that starts with inline triple-backtick
// code"); Markdown's marks never show as text and HTML in a reply is the text it is (docs/contracts/tasks.md
// TASKS-33, which reads a summary "as the conversation reads it"); a link goes only to https (CHATFIRST-40);
// and a long reply, a long line or a wide code block or table never breaks the window (T3 93aaad5b45,
// 5bc2d512e4, 373ec12be6).

const html = (text: string, streaming = false) => renderToStaticMarkup(<Said text={text} streaming={streaming} />).replaceAll('<!-- -->', '');
const words = (markup: string) => markup.replace(/<[^>]+>/g, '');

describe('a reply while it streams (T3: streaming render)', () => {
  it('t3_stream_a_code_block_not_closed_yet_is_code_already', () => {
    const now = html('Run this:\n\n```ts\nconst a = 1;\nconst b = 2', true);
    expect(now).toContain('class="ss-code"');
    expect(words(now)).toContain('const a = 1;');
    expect(words(now)).not.toContain('```');
    // and it reads as the finished reply will
    const done = html('Run this:\n\n```ts\nconst a = 1;\nconst b = 2\n```');
    expect(words(now)).toBe(words(done));
  });

  it('t3_stream_a_code_block_in_a_list_renders_as_the_finished_reply_does', () => {
    // half way through a code block in a list item: its ** is code, never a mark to close
    const now = html('1. Install it:\n\n   ```sh\n   npm run **build', true);
    expect(now).toMatch(/<li>[\s\S]*class="ss-code"[\s\S]*<\/li>/);
    expect(words(now)).toContain('npm run **build');
    expect(words(now)).not.toContain('build**');
    const done = html('1. Install it:\n\n   ```sh\n   npm run **build\n   ```\n2. Then test');
    expect(words(done)).toContain('npm run **build');
  });

  it('t3_stream_bold_not_closed_yet_shows_bold_never_its_stars', () => {
    const now = html('The test fails because **the token', true);
    expect(now).toContain('<strong>');
    expect(words(now)).not.toContain('**');
  });

  // Known bug #2644: Said.tsx `closed()` counts any line starting with ``` as a code
  // fence (`/^ {0,3}(?:```|~~~)/gm`), so a line that opens with inline code (```x``` …, which marked reads
  // as inline code, not a fence) makes it think a code block is open: from then until the reply ends,
  // marks not closed yet show as typed (`**`), against Said.tsx's own rule ("show as they will once
  // they're closed, never as typed so far").
  it.fails('t3_stream_after_a_line_of_inline_triple_backtick_code_bold_still_shows_bold (known bug #2644)', () => {
    const done = html('```npm ci``` installs it\n\nthen **run the tests**');
    // marked reads the first line as inline code, no block
    expect(done).not.toContain('class="ss-code"');
    const now = html('```npm ci``` installs it\n\nthen **run the', true);
    expect(now).toContain('<strong>');
    expect(words(now)).not.toContain('**');
  });
});

describe('what a reply can hold (T3: markdown, long messages)', () => {
  it('t3_markdown_html_in_a_reply_is_the_text_it_is', () => {
    const out = html('Look: <img src=x onerror="alert(1)"> and <script>alert(2)</script>\n\n<iframe src="https://example.com"></iframe>');
    expect(out).not.toMatch(/<(img|script|iframe)\b/i);
    expect(words(out)).toContain('&lt;img src=x onerror=&quot;alert(1)&quot;&gt;');
    expect(words(out)).toContain('&lt;iframe');
  });

  it('t3_markdown_only_an_https_link_is_a_link', () => {
    const out = html('[ok](https://github.com/o/r/pull/1) [js](javascript:alert(1)) [plain](http://example.com) [file](file:///etc/passwd) ![img](https://example.com/x.png)');
    const hrefs = [...out.matchAll(/<a [^>]*href="([^"]*)"/g)].map((m) => m[1]);
    expect(hrefs).toEqual(['https://github.com/o/r/pull/1', 'https://example.com/x.png']);
    // an image is never fetched: its words, linked
    expect(out).not.toMatch(/<img\b/);
  });

  it('t3_long_a_reply_too_long_or_too_deep_to_read_shows_as_its_text_and_the_window_stays', () => {
    const long = `${'word '.repeat(5000)}**end**`;
    expect(words(html(long))).toContain('**end**');
    const deep = `${'> '.repeat(5000)}deep`;
    expect(() => html(deep)).not.toThrow();
    expect(words(html(deep))).toContain('deep');
    const stars = '_a'.repeat(4000);
    const t = performance.now();
    html(stars);
    expect(performance.now() - t).toBeLessThan(2000);
  });

  it('t3_long_a_long_line_wraps_and_a_wide_code_block_or_table_scrolls_inside_the_reply', () => {
    const css = readFileSync(join(__dirname, '..', 'app.css'), 'utf8');
    const rule = (selector: string) => new RegExp(`(^|\\n)${selector.replace(/[.]/g, '\\.')} \\{([^}]*)\\}`).exec(css)?.[2] ?? '';
    expect(rule('.msg-bubble')).toMatch(/overflow-wrap: anywhere/);
    expect(rule('.ss-code')).toMatch(/white-space: pre;[\s\S]*overflow-x: auto/);
    expect(rule('.md-table')).toMatch(/max-width: 100%;[\s\S]*overflow-x: auto/);
    const table = html('| a | b |\n|---|---|\n| 1 | 2 |');
    expect(table).toMatch(/^<div class="md-table"><table>/);
  });
});
