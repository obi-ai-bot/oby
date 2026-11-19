import { describe, expect, it } from 'vitest';
import { KEY, chipsOf, defaultsOf, setChips } from '../chips';
import { KEPT } from '../drafts';

// The message box's mode chip, kept per box (docs/contracts/composer.md, COMPOSER-1, 3 and 17)
function storage(): Storage {
  const map = new Map<string, string>();
  return { get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null, key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); } };
}

describe('chips', () => {
  it('composer_3_chips_are_kept_per_box', () => {
    const s = storage();
    setChips('conversation:c1', { mode: 'plan' }, s, 1000);
    setChips('home', { mode: 'ask' }, s, 1000);
    expect(chipsOf('conversation:c1', s, 2000)).toEqual({ mode: 'plan' });
    expect(chipsOf('home', s, 2000)).toEqual({ mode: 'ask' });
    expect(chipsOf('conversation:c2', s, 2000)).toEqual({});
    // a week after they were last set, they go, as a draft does
    expect(chipsOf('home', s, 1000 + KEPT)).toEqual({});
    // all back to the owner's usual: the box's go
    setChips('conversation:c1', { mode: undefined }, s, 3000);
    expect(JSON.parse(s.getItem(KEY)!)).not.toHaveProperty('conversation:c1');
    // a value that isn't one is nothing kept
    s.setItem(KEY, JSON.stringify({ home: { mode: 7, at: 3000 }, x: 'y' }));
    expect(chipsOf('home', s, 3000)).toEqual({});
  });

  it('composer_3_only_the_mode_goes', () => {
    expect(defaultsOf({})).toBeUndefined();
    expect(defaultsOf({ mode: 'ask' })).toEqual({ mode: 'ask' });
  });

  it('composer_17_a_model_an_earlier_build_kept_is_nothing', () => {
    const s = storage();
    // 0.2.23's model chip kept a tool, model and effort with the mode: only the mode is read
    s.setItem(KEY, JSON.stringify({ home: { mode: 'plan', agent: 'claude', model: 'claude-opus-5-5', effort: 'high', at: 1000 }, 'conversation:c1': { agent: 'codex', at: 1000 } }));
    expect(chipsOf('home', s, 2000)).toEqual({ mode: 'plan' });
    expect(defaultsOf(chipsOf('home', s, 2000))).toEqual({ mode: 'plan' });
    expect(chipsOf('conversation:c1', s, 2000)).toEqual({});
  });
});
