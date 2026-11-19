import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { shortTitle, titled } from '../titles';

// CHATFIRST-36: a new conversation gets a short title, a few words, by rule (obid's `short_title`,
// core/crates/obid/src/conversation.rs). The app keeps the same rule for the mock and for the row it
// shows before obid has listed the conversation; obid's own test has the same table.
const CASES: [said: string, title: string][] = [
  ["Can you please take a look at the failing login retry test in the oby repo and fix it, it started failing after yesterday's merge", 'Failing login retry test'],
  ["Hey Oby, what's on my plate today?", "What's on my plate today"],
  ['review oby PR #33\nand the vault one', 'Review oby PR #33'],
  ['why did oberon’s build go from 4 to 11 minutes since the update?', 'Why did oberon’s build go'],
  ['I want you to split Engine/Jobs.swift in three', 'Split Engine/Jobs.swift in three'],
  ['please fix it', 'Fix it'],
  ['First question', 'First question'],
  ['Let’s plan the 0.9 release. Then the notes', 'Plan the 0.9 release'],
  ['ok', 'Ok'],
  [' \n ', 'New conversation'],
];

describe('a new conversation\'s title (CHATFIRST-36)', () => {
  it('chatfirst_36_a_new_conversation_is_titled_in_a_few_words', () => {
    for (const [said, title] of CASES) expect(shortTitle(said), said).toBe(title);
    const long = shortTitle('supercalifragilistic '.repeat(9));
    expect(long.split(' ').length).toBeLessThanOrEqual(6);
    expect([...long].length).toBeLessThanOrEqual(48);
  });

  it('chatfirst_36_titled_still_cuts_the_first_line_at_48', () => {
    expect(titled('why did oberon’s build go from 4 to 11 minutes since the update?')).toBe('Why did oberon’s build go from 4 to 11 minute…');
    expect(titled('report.pdf')).toBe('Report.pdf');
  });

  it('chatfirst_36_the_app_and_obid_share_one_rule', () => {
    const rust = readFileSync(join(__dirname, '../../../../../../core/crates/obid/src/conversation.rs'), 'utf8');
    const ts = readFileSync(join(__dirname, '../titles.ts'), 'utf8');
    const list = (src: string, name: string) => {
      const at = src.indexOf(`const ${name}`);
      expect(at, name).toBeGreaterThanOrEqual(0);
      const body = src.slice(src.indexOf('[', src.indexOf('=', at)), src.indexOf(']', src.indexOf('=', at)));
      return [...body.matchAll(/"([^"]*)"|'([^']*)'/g)].map((m) => m[1] ?? m[2]);
    };
    for (const name of ['OPENERS', 'DANGLING']) expect(list(ts, name), name).toEqual(list(rust, name));
    expect(ts).toContain('const TITLE_WORDS = 6;');
    expect(rust).toContain('const TITLE_WORDS: usize = 6;');
    // and obid's test holds the same cases
    for (const [, title] of CASES) expect(rust, title).toContain(JSON.stringify(title));
  });
});
