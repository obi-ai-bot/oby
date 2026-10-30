// What was typed in a message box and not sent yet, by box (Home's, a
// conversation's, a task's, an Oby's page's): put back when the box shows
// again, the cursor at its end, kept across a relaunch, and gone once sent (the
// box emptied), once its conversation is deleted, or a week after it was last
// typed in (so what was pasted, a secret among it, doesn't stay for good). The
// user's own words, so the window's localStorage holds them, under one key, as
// the Mac and iPhone apps keep theirs in UserDefaults (apps/shared/ChatThread.swift,
// Drafts); on this Mac only, never sent to obid or anywhere else.

export const KEY = 'oby.drafts';
// how long an untouched draft is kept
export const KEPT = 7 * 86_400_000;
// how long after the last keystroke it's written (a write per keystroke would be too many)
export const DELAY = 300;

// The boxes, by what they're for
export const box = {
  home: 'home',
  // a conversation with Oby: undefined for a new one, until its first words start it
  conversation: (id?: string) => `conversation:${id ?? 'new'}`,
  // a task's box: its page's, its session's and its conversation's are one
  task: (id: string) => `task:${id}`,
  // a conversation with other people's Obys
  peer: (id: string) => `peer:${id}`,
  // an Oby's page: the box that starts a conversation with it
  oby: (name: string) => `oby:${name}`,
};

type Kept = Record<string, { text: string; at: number }>;

// the drafts still kept at `now` (a value that isn't one is nothing kept)
function live(store: Storage, now: number): Kept {
  try {
    const all: unknown = JSON.parse(store.getItem(KEY) ?? '{}');
    if (!all || typeof all !== 'object' || Array.isArray(all)) return {};
    return Object.fromEntries(Object.entries(all as Record<string, unknown>).filter(([, d]) => {
      const draft = d as { text?: unknown; at?: unknown } | null;
      return !!draft && typeof draft.text === 'string' && typeof draft.at === 'number' && now - draft.at < KEPT;
    })) as Kept;
  } catch {
    return {};
  }
}

// The box's draft, or '' for none
export function draft(box: string, store: Storage = localStorage, now = Date.now()): string {
  return live(store, now)[box]?.text ?? '';
}

// Keeps `text` as the box's draft, and lets go of every draft past KEPT;
// nothing but white space lets the box's go
export function keep(box: string, text: string, store: Storage = localStorage, now = Date.now()) {
  const all = live(store, now);
  if (text.trim()) all[box] = { text, at: now };
  else delete all[box];
  try {
    if (Object.keys(all).length) store.setItem(KEY, JSON.stringify(all));
    else store.removeItem(KEY);
  } catch { /* not kept, that's all */ }
}

// The box's draft is gone (its conversation deleted, say)
export const forget = (box: string, store: Storage = localStorage) => keep(box, '', store);

// One message box's keeper: show(box) hands over the box's draft to put back
// (writing the box before's pending words first); typed(text) keeps the words
// DELAY after the last keystroke, or at once when the box is emptied (sent,
// or cleared by hand); flush() writes what's pending now (the window closing)
export function keeper(store: Storage = localStorage, delay = DELAY) {
  // kept: what the store holds for the shown box, so the editor reporting the
  // words put back doesn't count as typing them (and push their week back)
  let shown: string | null = null, kept = '', pending: string | null = null, timer: ReturnType<typeof setTimeout> | undefined;
  const flush = () => {
    clearTimeout(timer);
    timer = undefined;
    if (shown !== null && pending !== null) { keep(shown, pending, store); kept = pending; }
    pending = null;
  };
  return {
    show(next: string | null): string {
      flush();
      shown = next;
      return (kept = next === null ? '' : draft(next, store));
    },
    typed(text: string) {
      if (shown === null) return;
      if (text === kept) { clearTimeout(timer); pending = null; return; }
      pending = text;
      if (!text.trim()) return flush();
      clearTimeout(timer);
      timer = setTimeout(flush, delay);
    },
    flush,
  };
}
