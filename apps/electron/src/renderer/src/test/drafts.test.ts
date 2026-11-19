import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DELAY, FILE_KEPT, FILES_KEPT, KEPT, KEY, box, dataOf, draft, extras, fileOf, forget, hold, keep, keeper } from '../drafts';

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
    // the editor reporting the words put back changes nothing, their time neither
    const at = JSON.parse(s.getItem(KEY)!)[box.conversation('c2')].at;
    k.typed('Second thoughts');
    vi.advanceTimersByTime(DELAY);
    expect(JSON.parse(s.getItem(KEY)!)[box.conversation('c2')].at).toBe(at);
    // typed back to what's kept before the moment passed: nothing to write
    k.typed('Second thought');
    k.typed('Second thoughts');
    vi.advanceTimersByTime(DELAY);
    expect(JSON.parse(s.getItem(KEY)!)[box.conversation('c2')].at).toBe(at);
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

// A draft keeps its tags and choices with its words (docs/contracts/drafts.md, DRAFTS-4 to 8)
describe('DRAFTS-4 to 8: a draft keeps its files, tags and project', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('drafts_4_tasks_a_mac_and_a_repository_tagged_come_back_after_a_relaunch', () => {
    const s = storage();
    const k = keeper(s, DELAY);
    k.show(box.conversation('c1'));
    k.typed('Look at these');
    k.carry({ tags: [{ kind: 'task', id: 't9' }, { kind: 'place', name: 'Studio Mac' }, { kind: 'repo', name: 'acme/app' }] });
    vi.advanceTimersByTime(DELAY);
    const again = keeper(storage({ [KEY]: s.getItem(KEY)! }), DELAY);
    expect(again.show(box.conversation('c1'))).toBe('Look at these');
    expect(again.more().tags).toEqual([{ kind: 'task', id: 't9' }, { kind: 'place', name: 'Studio Mac' }, { kind: 'repo', name: 'acme/app' }]);
    // another box has its own, none here
    expect(again.show(box.conversation('c2'))).toBe('');
    expect(again.more()).toEqual({});
  });

  it('drafts_5_homes_project_and_folders_are_kept_with_its_words', () => {
    const s = storage();
    keep(box.home, 'Start the API', s, Date.now(), { project: 'p1', folders: ['/work/api'] });
    expect(extras(box.home, s)).toEqual({ project: 'p1', folders: ['/work/api'] });
    // typing on keeps them
    keep(box.home, 'Start the API now', s);
    expect(extras(box.home, s)).toEqual({ project: 'p1', folders: ['/work/api'] });
  });

  it('drafts_6_a_small_files_bytes_come_back_after_a_relaunch_a_big_one_only_this_run', async () => {
    vi.useRealTimers();
    const s = storage();
    const small = new File(['hello'], 'notes.txt', { type: 'text/plain' });
    const data = await dataOf(small);
    expect(data).toMatch(/^data:text\/plain;base64,/);
    keep(box.task('t1'), '', s, Date.now(), { tags: [{ kind: 'file', name: 'notes.txt', type: 'text/plain', data }, { kind: 'file', name: 'big.mov', type: 'video/quicktime' }] });
    // after a relaunch (nothing held): the small one is made again from its bytes, the big one is gone
    hold(box.task('t1'), []);
    const [one, two] = extras(box.task('t1'), storage({ [KEY]: s.getItem(KEY)! })).tags!;
    const back = fileOf(box.task('t1'), one as never)!;
    expect(back.name).toBe('notes.txt');
    expect(await back.text()).toBe('hello');
    expect(fileOf(box.task('t1'), two as never)).toBeUndefined();
    // this run, the big one is held
    const big = new File(['x'], 'big.mov');
    hold(box.task('t1'), [{ name: 'big.mov', file: big }]);
    expect(fileOf(box.task('t1'), two as never)).toBe(big);
    // past FILE_KEPT no bytes are read; past FILES_KEPT all told a file keeps its name only
    expect(await dataOf(new File([new Uint8Array(FILE_KEPT + 1)], 'huge.bin'))).toBeUndefined();
    const full = 'data:x;base64,' + 'A'.repeat(FILES_KEPT - 20);
    keep(box.task('t2'), '', s, Date.now(), { tags: [{ kind: 'file', name: 'a', data: full }, { kind: 'file', name: 'b', data: full }] });
    expect(extras(box.task('t2'), s).tags).toEqual([{ kind: 'file', name: 'a', data: full }, { kind: 'file', name: 'b' }]);
  });

  it('drafts_7_a_box_with_tags_and_no_words_is_still_kept', () => {
    const s = storage();
    const k = keeper(s, DELAY);
    k.show(box.conversation('c1'));
    k.carry({ tags: [{ kind: 'task', id: 't1' }] });
    vi.advanceTimersByTime(DELAY);
    expect(draft(box.conversation('c1'), s)).toBe('');
    expect(extras(box.conversation('c1'), s).tags).toEqual([{ kind: 'task', id: 't1' }]);
  });

  it('drafts_8_sending_or_deleting_lets_the_tags_go_with_the_words', () => {
    const s = storage();
    const k = keeper(s, DELAY);
    k.show(box.conversation('c1'));
    k.typed('With a task');
    k.carry({ tags: [{ kind: 'task', id: 't1' }] });
    vi.advanceTimersByTime(DELAY);
    // sent: the box empties, its tags go
    k.typed('');
    k.carry({});
    expect(s.getItem(KEY)).toBeNull();
    keep(box.conversation('c2'), '', s, Date.now(), { tags: [{ kind: 'task', id: 't2' }] });
    forget(box.conversation('c2'), s);
    expect(s.getItem(KEY)).toBeNull();
  });

  it('drafts_8_a_foreign_tag_is_nothing_kept', () => {
    const at = Date.now();
    const s = storage({ [KEY]: JSON.stringify({ home: { text: 'ok', at, tags: [{ kind: 'task' }, { kind: 'file', name: 3 }, 'x', { kind: 'place', name: 'Mac' }], project: 7, folders: ['/a', 1] } }) });
    expect(extras(box.home, s)).toEqual({ tags: [{ kind: 'place', name: 'Mac' }], folders: ['/a'] });
  });
});
