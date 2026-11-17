import { useSyncExternalStore } from 'react';
import { KEPT } from './drafts';

// The message box's chips (docs/contracts/composer.md, COMPOSER-1 and 2): the mode, tool, model and
// effort the owner set for the tasks their words start, by box (drafts.ts's boxes), as T3 Code keeps
// its per-draft model and mode. Kept with the box's draft: across a relaunch, and a week after they
// were last set. They go with the words (COMPOSER-3) and are never written to Settings or an
// account: this window's localStorage holds them, on this Mac only.

export type Chips = { mode?: string; agent?: string; model?: string; effort?: string };
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
      const kept = Object.fromEntries((['mode', 'agent', 'model', 'effort'] as const).filter((k) => text(c[k])).map((k) => [k, c[k] as string]));
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

// What goes with the words (COMPOSER-3): only what's set; a model or effort only with its tool
export function defaultsOf(chips: Chips): Chips | undefined {
  const out: Chips = {
    ...(chips.mode ? { mode: chips.mode } : {}),
    ...(chips.agent ? { agent: chips.agent, ...(chips.model ? { model: chips.model } : {}), ...(chips.effort ? { effort: chips.effort } : {}) } : {}),
  };
  return Object.keys(out).length ? out : undefined;
}

let version = 0;
listeners.add(() => { version++; });
// the box's chips as they are now, again on any change
export function useChips(box: string | undefined): [Chips, (c: Chips) => void] {
  useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => version);
  return [chipsOf(box), (c) => { if (box) setChips(box, c); }];
}
