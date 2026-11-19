import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { withEntries, withLine, withOutput, withTail } from '../liveTask';
import { parseEvent } from '../../../shared/obid-wire';
import { sessionOf } from '../jobPage';
import { Line } from '../SessionView';
import type { TranscriptEntry } from '../obid';
import type { Entry } from '../session';
import { BRANDS } from '../brands';

// What streams while a task's agent works (docs/contracts/live-task.md, LIVETASK-32 to 36): the words
// past their last whole line, and a running command's last lines, as obid masks and sends them.
const page = vi.hoisted(() => {
  // the browser's globals the session view reads as it loads
  const globals = { location: { search: '' }, window: {}, document: { createElement: () => ({}) }, localStorage: { getItem: () => null, setItem: () => {}, removeItem: () => {} }, addEventListener: () => {}, removeEventListener: () => {}, matchMedia: () => ({ matches: true, addEventListener: () => {}, removeEventListener: () => {} }) };
  for (const [name, value] of Object.entries(globals)) (globalThis as Record<string, unknown>)[name] = value;
  return { obid: true };
});
vi.mock('../lean', async (original) => ({ ...await original<typeof import('../lean')>(), get onObid() { return page.obid; } }));

const wire = (n: number, more: Partial<TranscriptEntry> = {}): TranscriptEntry => ({ n, at: '2026-11-17T13:50:00Z', kind: 'said', summary: '', ...more });
const command = (n: number, more: Partial<TranscriptEntry> = {}) => wire(n, { kind: 'tool', tool: 'Bash', activity: 'command', summary: 'Running for i in 1 2 3', state: 'running', ...more });
const lineOf = (entries: TranscriptEntry[], n: number) => sessionOf(entries).find((e) => e.n === n)!;

describe('a task streaming', () => {
  it('LIVETASK-32: the words past the last whole line are read, show after the entry\'s words, and the whole message ends them', () => {
    expect(parseEvent('transcript', JSON.stringify({ job: 'j1', n: 2, tail: 'Checking the ' }))).toEqual({ kind: 'transcriptTail', job: 'j1', n: 2, tail: 'Checking the ' });
    // made with its first words: no line whole yet
    let had = withTail([wire(2, { text: '', summary: 'Checking the' })], 2, 'Checking the ');
    expect(lineOf(had, 2)).toMatchObject({ kind: 'said', text: 'Checking the ', live: true });
    had = withTail(had, 2, 'Checking the scan ');
    expect(lineOf(had, 2)).toMatchObject({ text: 'Checking the scan ', live: true });
    // its first whole line comes as the entry again: what was past it is in it now
    had = withEntries(had, [wire(2, { text: 'Checking the scan first.\n', summary: 'Checking the scan first.' })]);
    expect(lineOf(had, 2)).toMatchObject({ text: 'Checking the scan first.\n' });
    had = withTail(had, 2, 'Then the ');
    expect(lineOf(had, 2)).toMatchObject({ text: 'Checking the scan first.\nThen the ', live: true });
    // a line appended has what was past the last one: still being written
    had = withLine(had, 2, 'Then the push.\n');
    expect(lineOf(had, 2)).toMatchObject({ text: 'Checking the scan first.\nThen the push.\n', live: true });
    // a fetch answered meanwhile doesn't take the words being written away
    had = withTail(had, 2, 'Done ');
    had = withEntries(had, [{ ...wire(2, { text: 'Checking the scan first.\n' }), fetched: true }]);
    expect(lineOf(had, 2)).toMatchObject({ text: 'Checking the scan first.\nThen the push.\nDone ', live: true });
    // the whole message replaces them all
    had = withEntries(had, [wire(2, { text: 'Checking the scan first.\nThen the push.\nDone now.' })]);
    expect(lineOf(had, 2)).toMatchObject({ text: 'Checking the scan first.\nThen the push.\nDone now.' });
    expect(lineOf(had, 2)).not.toHaveProperty('live', true);
  });

  it('LIVETASK-32: on obid the words show as they came, with the caret, not a word at a time', () => {
    const said = { n: 2, kind: 'said', text: 'Checking the scan ', at: '1:50 PM', live: true } as Entry;
    const html = renderToStaticMarkup(<ul><Line entry={said} job="j1" brand={BRANDS.claude} live={{ shown: '', tick: 0 }} /></ul>);
    const text = (html: string) => html.replace(/<[^>]+>/g, '');
    expect(text(html)).toContain('Checking the scan');
    expect(html).toContain('ss-caret');
    // the mock reveals its words itself, a word at a time, from what it has shown so far
    page.obid = false;
    const mock = renderToStaticMarkup(<ul><Line entry={said} job="j1" brand={BRANDS.claude} live={{ shown: 'Checking ', tick: 0 }} /></ul>);
    page.obid = true;
    expect(text(mock)).toContain('Checking');
    expect(text(mock)).not.toContain('scan');
  });

  it('LIVETASK-34: a running command\'s last lines are read and shown on its line while it runs, its own output once it ends', () => {
    expect(parseEvent('transcript', JSON.stringify({ job: 'j1', n: 3, output: { text: 'line 1\nline 2', lines: 2 } })))
      .toEqual({ kind: 'transcriptOutput', job: 'j1', n: 3, output: { text: 'line 1\nline 2', lines: 2 } });
    expect(parseEvent('transcript', JSON.stringify({ job: 'j1', n: 3, output: 'not lines' }))).toBeNull();
    let had = withOutput([command(3)], 3, { text: 'line 1', lines: 1 });
    expect(lineOf(had, 3)).toMatchObject({ kind: 'tool', state: 'running', live: { output: 'line 1', lines: 1 } });
    had = withOutput(had, 3, { text: 'line 1\nline 2', lines: 2 });
    expect(lineOf(had, 3)).toMatchObject({ live: { output: 'line 1\nline 2', lines: 2 } });
    // a fetch of it while it runs keeps them
    had = withEntries(had, [{ ...command(3), fetched: true }]);
    expect(lineOf(had, 3)).toMatchObject({ live: { output: 'line 1\nline 2' } });
    // ended, its line has none: its output is its detail's, fetched when it opens
    had = withEntries(had, [command(3, { state: 'done', seconds: 3 })]);
    expect((lineOf(had, 3) as Extract<Entry, { kind: 'tool' }>).live).toBeUndefined();
    // and lines that come late for an ended call are dropped
    expect(withOutput(had, 3, { text: 'late', lines: 9 })[0].output).toBeUndefined();
  });
});
