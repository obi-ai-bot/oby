import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { owners, wordsOf } from '../../../shared/words';

// What went with obid's own Chrome (docs/contracts/browser-app.md, BROWSERAPP-10): the page is each
// tab's own <webview>, so the window streams no picture of it and signs none of the owner's mouse
// and keys for obid. This fails if any of it comes back into the app's source. Take control and
// Give back stay signed (BROWSERAPP-9, browserLive.test.ts).

const SRC = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const sources = (dir: string): string[] => readdirSync(dir).flatMap((name) => {
  const path = join(dir, name);
  if (statSync(path).isDirectory()) return /^(test|node_modules)$/.test(name) ? [] : sources(path);
  return /\.(ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : [];
});

// each retired piece, and the words that would mean it's back
const RETIRED: [string, RegExp][] = [
  ['the live frame (the streamed picture of the page)', /\bLiveFrame\b|data-browser="frame(-input)?"|\bBrowserFrame\b/],
  ['main\'s stream followers and their channels', /browser-(watch|unwatch|frame)\b|\/v1\/browser\/tabs\/[^'"`\s]*\/stream|browserTabPath\([^)]*'stream'|tabs\\\/\[\^\/\]\+\\\/stream/],
  ['the owner\'s input signed for obid (browserInput)', /\b(sawInput|mayInput|INPUT_PATH|BrowserInput)\b|browserInput|\/v1\/browser\/tabs\/[^'"`\s]*\/input|browserTabPath\([^)]*'input'/],
];

describe('BROWSERAPP-10: what went with obid\'s own browser stays gone', () => {
  it('browserapp_10: main/browserInput.ts is gone', () => {
    expect(existsSync(join(SRC, 'main', 'browserInput.ts'))).toBe(false);
  });

  it('browserapp_10: no source of the app has the live frame, a stream follower or the owner\'s input for obid', () => {
    const files = sources(SRC);
    expect(files.length).toBeGreaterThan(100);
    const back = RETIRED.flatMap(([what, words]) => files
      .filter((f) => words.test(readFileSync(f, 'utf8')))
      .map((f) => `${what}: ${relative(SRC, f)}`));
    expect(back).toEqual([]);
  });

  it('browserapp_10: main signs no input or stream, whatever label the owner pressed', () => {
    for (const verb of ['input', 'stream']) {
      const path = `/v1/browser/tabs/t1/${verb}`;
      expect(wordsOf('POST', path, {})).toBeNull();
      expect(owners('POST', path, {}, ['Take control', 'Give back', 'Take control to answer', 'I’ll choose in the page'], [])).toBeNull();
    }
  });
});
