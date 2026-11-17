import { describe, expect, it } from 'vitest';
import { KEY, chipsOf, defaultsOf, setChips } from '../chips';
import { KEPT } from '../drafts';

// The message box's chips, kept per box (docs/contracts/composer.md, COMPOSER-1 to 3)
function storage(): Storage {
  const map = new Map<string, string>();
  return { get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null, key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); } };
}

describe('chips', () => {
  it('composer_3_chips_are_kept_per_box', () => {
    const s = storage();
    setChips('conversation:c1', { mode: 'plan' }, s, 1000);
    setChips('home', { agent: 'codex', model: 'gpt-6.1-sol', effort: 'high' }, s, 1000);
    expect(chipsOf('conversation:c1', s, 2000)).toEqual({ mode: 'plan' });
    expect(chipsOf('home', s, 2000)).toEqual({ agent: 'codex', model: 'gpt-6.1-sol', effort: 'high' });
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

  it('composer_3_only_what_is_set_goes_and_a_model_only_with_its_tool', () => {
    expect(defaultsOf({})).toBeUndefined();
    expect(defaultsOf({ mode: 'ask' })).toEqual({ mode: 'ask' });
    expect(defaultsOf({ model: 'claude-opus-5-5', effort: 'high' })).toBeUndefined();
    expect(defaultsOf({ agent: 'claude', model: 'claude-opus-5-5' })).toEqual({ agent: 'claude', model: 'claude-opus-5-5' });
  });
});
