import { describe, expect, it } from 'vitest';
import { stays } from '../../../main/navigate';

// Where the window's page may go (main/navigate.ts, docs/contracts/profiles.md PROFILES-42): a built
// app's page reloads itself once main has opened another profile (PROFILES-41), and never leaves for
// another page.
const page = 'file:///Applications/Oby.app/Contents/Resources/app.asar/out/renderer/index.html';
describe('the window’s page', () => {
  it('profiles_42_the_page_reloads_itself_in_a_built_app', () => {
    // a reload is a navigation to the URL it is at: it goes through, built or in dev
    expect(stays(page, page, false)).toBe(true);
    expect(stays('http://localhost:5173/', 'http://localhost:5173/', true)).toBe(true);
    // anywhere else, built: refused, another file, the same file with other words, a site
    expect(stays(page.replace('index.html', 'oby/oby.html'), page, false)).toBe(false);
    expect(stays(`${page}?welcome`, page, false)).toBe(false);
    expect(stays('https://example.com/', page, false)).toBe(false);
    // in dev, itself with other words after ? (?welcome), never another site
    expect(stays('http://localhost:5173/?welcome', 'http://localhost:5173/', true)).toBe(true);
    expect(stays('https://example.com/', 'http://localhost:5173/', true)).toBe(false);
    expect(stays('not a url', 'http://localhost:5173/', true)).toBe(false);
  });
});
