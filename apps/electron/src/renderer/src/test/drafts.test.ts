import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DELAY, KEPT, KEY, box, draft, forget, keep, keeper } from '../drafts';

// The page's local storage, in memory (as apps/web's tests have it)
function storage(entries: Record<string, string> = {}): Storage {
  const map = new Map(Object.entries(entries));
  return {
    get length() { return map.size; },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null,
    removeItem: (k) => { map.delete(k); },
    setItem: (k, v) => { map.set(k, v); },
  };
}

describe('drafts', () => {
  it('keeps each box apart, until its words are sent or cleared', () => {
    const s = storage();
    keep(box.conversation('c1'), 'Fix the login bug', s);
    keep(box.conversation('c2'), 'Second thoughts', s);
    keep(box.home, 'Start on #12', s);
    expect(draft(box.conversation('c1'), s)).toBe('Fix the login bug');
    expect(draft(box.conversation('c2'), s)).toBe('Second thoughts');
    expect(draft(box.home, s)).toBe('Start on #12');
    expect(draft(box.conversation('c3'), s)).toBe('');
    // sent (the box emptied): gone, the others untouched
    keep(box.conversation('c1'), '', s);
    expect(draft(box.conversation('c1'), s)).toBe('');
    expect(draft(box.conversation('c2'), s)).toBe('Second thoughts');
    // white space alone is nothing to keep
    keep(box.conversation('c2'), '  \n ', s);
    expect(draft(box.conversation('c2'), s)).toBe('');
    forget(box.home, s);
    expect(s.getItem(KEY)).toBeNull();
  });

  it('keeps line breaks and survives a reload of the same storage', () => {
    const s = storage();
    keep(box.task('t1'), 'first line\nsecond line', s);
    const again = storage({ [KEY]: s.getItem(KEY)! });
    expect(draft(box.task('t1'), again)).toBe('first line\nsecond line');
  });

  it('lets a draft go a week after it was last typed in', () => {
    const s = storage();
    const typed = 1_700_000_000_000;
    keep(box.conversation('old'), 'stale words', s, typed);
    keep(box.conversation('new'), 'fresh words', s, typed + KEPT - 1000);
    expect(draft(box.conversation('old'), s, typed + KEPT - 1)).toBe('stale words');
    expect(draft(box.conversation('old'), s, typed + KEPT)).toBe('');
    expect(draft(box.conversation('new'), s, typed + KEPT)).toBe('fresh words');
    // a write at that time drops it for good
    keep(box.conversation('new'), 'fresh words', s, typed + KEPT);
    expect(JSON.parse(s.getItem(KEY)!)).toEqual({ [box.conversation('new')]: { text: 'fresh words', at: typed + KEPT } });
  });

  it('takes a broken or foreign value as nothing kept', () => {
    expect(draft(box.home, storage({ [KEY]: 'not json' }))).toBe('');
    expect(draft(box.home, storage({ [KEY]: '[1,2]' }))).toBe('');
    expect(draft(box.home, storage({ [KEY]: JSON.stringify({ home: { text: 7, at: Date.now() } }) }))).toBe('');
    expect(draft(box.home, storage({ [KEY]: JSON.stringify({ home: { text: 'ok' } }) }))).toBe('');
  });

  it('names the boxes apart', () => {
    expect(box.conversation()).toBe('conversation:new');
    expect(box.conversation('a')).not.toBe(box.task('a'));
    expect(box.peer('a')).not.toBe(box.conversation('a'));
    expect(box.oby('Sangam')).toBe('oby:Sangam');
  });
});

describe('a box\'s keeper', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('writes a moment after the last keystroke, not on each', () => {
    const s = storage();
    const k = keeper(s, DELAY);
    expect(k.show(box.conversation('c1'))).toBe('');
    k.typed('F');
    k.typed('Fi');
    k.typed('Fix');
    vi.advanceTimersByTime(DELAY - 1);
    expect(s.getItem(KEY)).toBeNull();
    vi.advanceTimersByTime(1);
    expect(draft(box.conversation('c1'), s)).toBe('Fix');
    // typing again holds the write back again
    k.typed('Fix it');
    vi.advanceTimersByTime(DELAY / 2);
    k.typed('Fix it now');
    vi.advanceTimersByTime(DELAY / 2);
    expect(draft(box.conversation('c1'), s)).toBe('Fix');
    vi.advanceTimersByTime(DELAY / 2);
    expect(draft(box.conversation('c1'), s)).toBe('Fix it now');
  });

  it('lets the draft go at once when the box is emptied', () => {
    const s = storage();
    const k = keeper(s, DELAY);
    k.show(box.home);
    k.typed('words');
    vi.advanceTimersByTime(DELAY);
    expect(draft(box.home, s)).toBe('words');
    k.typed('');
    expect(draft(box.home, s)).toBe('');
  });

  it('hands over the next box\'s draft, writing the one before first', () => {
    const s = storage();
    keep(box.conversation('c2'), 'Second thoughts', s);
    const k = keeper(s, DELAY);
    k.show(box.conversation('c1'));
    k.typed('half typ');
    // switched before the moment passed: c1's words are kept, c2's come back
    expect(k.show(box.conversation('c2'))).toBe('Second thoughts');
    expect(draft(box.conversation('c1'), s)).toBe('half typ');
    vi.advanceTimersByTime(DELAY);
    expect(draft(box.conversation('c2'), s)).toBe('Second thoughts');
    // the editor reporting the words put back changes nothing
    k.typed('Second thoughts');
    vi.advanceTimersByTime(DELAY);
    expect(draft(box.conversation('c1'), s)).toBe('half typ');
    expect(draft(box.conversation('c2'), s)).toBe('Second thoughts');
  });

  it('flushes what is pending (the window closing), and keeps nothing for no box', () => {
    const s = storage();
    const k = keeper(s, DELAY);
    k.typed('nowhere');
    k.flush();
    expect(s.getItem(KEY)).toBeNull();
    k.show(box.task('t1'));
    k.typed('a note');
    k.flush();
    expect(draft(box.task('t1'), s)).toBe('a note');
    // nothing pending any more: a later flush writes nothing new
    keep(box.task('t1'), 'changed elsewhere', s);
    k.flush();
    expect(draft(box.task('t1'), s)).toBe('changed elsewhere');
  });
});
