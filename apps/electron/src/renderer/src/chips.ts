import { useSyncExternalStore } from 'react';
import { KEPT } from './drafts';

// The message box's chips (docs/contracts/composer.md, COMPOSER-1): the mode the owner set for the
// tasks their words start, by box (drafts.ts's boxes), as T3 Code keeps its per-draft mode. Kept
// with the box's draft: across a relaunch, and a week after it was last set. It goes with the words
// (COMPOSER-3) and is never written to Settings: this window's localStorage holds it, on this Mac
// only. A tool, model or effort an earlier build kept here is read as nothing (COMPOSER-17): the
// model and effort are the account's, on the talk pill, and nothing on screen would show them.

export type Chips = { mode?: string };
export const KEY = 'oby.chips';

type Kept = Record<string, Chips & { at: number }>;
const listeners = new Set<() => void>();
const text = (v: unknown) => typeof v === 'string' && v !== '';

// the chips still kept at `now` (anything that isn't one is nothing kept)
function live(store: Storage, now: number): Kept {
  try {
    const all: unknown = JSON.parse(store.getItem(KEY) ?? '{}');
    if (!all || typeof all !== 'object' || Array.isArray(all)) return {};
    return Object.fromEntries(Object.entries(all as Record<string, Record<string, unknown>>).flatMap(([box, c]) => {
      if (!c || typeof c !== 'object' || typeof c.at !== 'number' || now - c.at >= KEPT) return [];
      const kept = Object.fromEntries((['mode'] as const).filter((k) => text(c[k])).map((k) => [k, c[k] as string]));
      return [[box, { ...kept, at: c.at }]];
    }));
  } catch {
    return {};
  }
}

// The box's chips: none set, {}
export function chipsOf(box: string | undefined, store: Storage = localStorage, now = Date.now()): Chips {
  if (!box) return {};
  const { at: _, ...chips } = live(store, now)[box] ?? { at: 0 };
  return chips;
}

// Sets the box's chips: a key undefined is the owner's usual again; nothing left lets the box's go
export function setChips(box: string, chips: Chips, store: Storage = localStorage, now = Date.now()) {
  const all = live(store, now);
  const kept = Object.fromEntries(Object.entries(chips).filter(([, v]) => text(v)));
  if (Object.keys(kept).length) all[box] = { ...kept, at: now };
  else delete all[box];
  try {
    if (Object.keys(all).length) store.setItem(KEY, JSON.stringify(all));
    else store.removeItem(KEY);
  } catch { /* not kept, that's all */ }
  listeners.forEach((l) => l());
}

// What goes with the words (COMPOSER-3): the mode, when one is set
export function defaultsOf(chips: Chips): Chips | undefined {
  return chips.mode ? { mode: chips.mode } : undefined;
}

let version = 0;
listeners.add(() => { version++; });
// the box's chips as they are now, again on any change
export function useChips(box: string | undefined): [Chips, (c: Chips) => void] {
  useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => version);
  return [chipsOf(box), (c) => { if (box) setChips(box, c); }];
}
