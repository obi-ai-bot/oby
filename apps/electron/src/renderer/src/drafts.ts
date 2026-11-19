// What was typed in a message box and not sent yet, by box (Home's, a
// conversation's, a task's, an Oby's page's): put back when the box shows
// again, the cursor at its end, kept across a relaunch, and gone once sent (the
// box emptied), once its conversation is deleted, or a week after it was last
// typed in (so what was pasted, a secret among it, doesn't stay for good). The
// user's own words, so the window's localStorage holds them, under one key, as
// the Mac and iPhone apps keep theirs in UserDefaults (apps/shared/ChatThread.swift,
// Drafts); on this Mac only, never sent to obid or anywhere else.
//
// With the words go the box's tags and choices (docs/contracts/drafts.md, DRAFTS-4 to 8): the
// tasks, Mac and repository tagged, Home's project and working folders, and its files. A file's
// bytes are kept with it up to FILE_KEPT, so it comes back after a relaunch too (T3 Code keeps a
// draft's images so); a bigger one only while the app runs (`held`), as a File can't be stored.

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
  // the Sphere's room
  room: (sphere: string) => `room:${sphere}`,
  // a member's thread: a Sphere's own, as owner ids are
  member: (sphere: string, owner: string) => `member:${sphere}:${owner}`,
};

// What a box keeps besides its words. A file: its name and type, and its bytes as a data URL when
// they're FILE_KEPT or less (else only `held` has them, for this run)
export type KeptTag = { kind: 'task'; id: string } | { kind: 'place'; name: string } | { kind: 'repo'; name: string }
  | { kind: 'file'; name: string; type?: string; data?: string };
export type Extras = { tags?: KeptTag[]; project?: string | null; folders?: string[] };
// the most of a file's bytes kept across a relaunch, and of all of a box's files together
export const FILE_KEPT = 1_000_000;
export const FILES_KEPT = 3_000_000;
type Entry = { text: string; at: number } & Extras;
type Kept = Record<string, Entry>;

// nothing to keep but the words: no tag, no project, no folder
export const bare = (e?: Extras) => !e || (!e.tags?.length && !e.project && !e.folders?.length);
// only what reads as extras: a value that isn't one is nothing kept
function extrasOf(d: Record<string, unknown>): Extras {
  const tags = Array.isArray(d.tags) ? d.tags.filter((t): t is KeptTag => {
    const k = t as Record<string, unknown> | null;
    if (!k || typeof k !== 'object') return false;
    if (k.kind === 'task') return typeof k.id === 'string';
    return (k.kind === 'place' || k.kind === 'repo' || k.kind === 'file') && typeof k.name === 'string'
      && (k.data === undefined || typeof k.data === 'string') && (k.type === undefined || typeof k.type === 'string');
  }) : [];
  const folders = Array.isArray(d.folders) ? d.folders.filter((f): f is string => typeof f === 'string') : [];
  return { ...(tags.length ? { tags } : {}), ...(typeof d.project === 'string' ? { project: d.project } : {}), ...(folders.length ? { folders } : {}) };
}

// the drafts still kept at `now` (a value that isn't one is nothing kept)
function live(store: Storage, now: number): Kept {
  try {
    const all: unknown = JSON.parse(store.getItem(KEY) ?? '{}');
    if (!all || typeof all !== 'object' || Array.isArray(all)) return {};
    return Object.fromEntries(Object.entries(all as Record<string, unknown>).flatMap(([k, d]) => {
      const draft = d as { text?: unknown; at?: unknown } | null;
      return !!draft && typeof draft.text === 'string' && typeof draft.at === 'number' && now - draft.at < KEPT
        ? [[k, { text: draft.text, at: draft.at, ...extrasOf(draft as Record<string, unknown>) }]] : [];
    })) as Kept;
  } catch {
    return {};
  }
}

// The box's draft, or '' for none
export function draft(box: string, store: Storage = localStorage, now = Date.now()): string {
  return live(store, now)[box]?.text ?? '';
}

// The box's tags and choices kept with its words, none for nothing kept
export function extras(box: string, store: Storage = localStorage, now = Date.now()): Extras {
  const d = live(store, now)[box];
  return d ? extrasOf(d) : {};
}

// Keeps `text` as the box's draft, with `more` (its tags and choices; left out, those it has),
// and lets go of every draft past KEPT; nothing but white space, and no tag or choice, lets the
// box's go
export function keep(box: string, text: string, store: Storage = localStorage, now = Date.now(), more?: Extras) {
  const all = live(store, now);
  const e = more ?? (all[box] ? extrasOf(all[box]) : {});
  if (text.trim() || !bare(e)) all[box] = { text, at: now, ...trimmed(e) };
  else delete all[box];
  try {
    if (Object.keys(all).length) store.setItem(KEY, JSON.stringify(all));
    else store.removeItem(KEY);
  } catch { /* not kept, that's all */ }
}

// What stays in the box once words that could be refused are answered: all of it when they didn't
// go; when they did, what was typed after them while they went
export const leftInBox = (sent: string, now: string, went: boolean) =>
  !went ? now : now.startsWith(sent) ? now.slice(sent.length).trimStart() : now;

// what's stored of extras: only what has something in it, and files' bytes up to FILES_KEPT all
// told (past it, a file keeps its name only: this run's `held` still has it)
function trimmed(e: Extras): Extras {
  let room = FILES_KEPT;
  const tags = e.tags?.map((t) => {
    if (t.kind !== 'file' || !t.data) return t;
    if (t.data.length > room) { const { data: _, ...rest } = t; return rest; }
    room -= t.data.length;
    return t;
  });
  return { ...(tags?.length ? { tags } : {}), ...(e.project ? { project: e.project } : {}), ...(e.folders?.length ? { folders: e.folders } : {}) };
}

// The files a box holds this run, by box and name: what a draft's file tag stands for when its
// bytes weren't kept (or before they're read). Gone with the window, as a File can't be stored
const held = new Map<string, File>();
const heldKey = (box: string, name: string) => `${box}\n${name}`;
export const hold = (box: string, files: { name: string; file?: File }[]) => {
  for (const k of [...held.keys()]) if (k.startsWith(`${box}\n`)) held.delete(k);
  for (const f of files) if (f.file) held.set(heldKey(box, f.name), f.file);
};
// a kept file tag's File: this run's own, else one made from its kept bytes, else none (a big file
// after a relaunch: its tag doesn't come back)
export function fileOf(box: string, t: Extract<KeptTag, { kind: 'file' }>): File | undefined {
  const own = held.get(heldKey(box, t.name));
  if (own) return own;
  if (!t.data) return undefined;
  const m = /^data:([^;,]*)(;base64)?,(.*)$/s.exec(t.data);
  if (!m) return undefined;
  const bytes = m[2] ? Uint8Array.from(atob(m[3]), (c) => c.charCodeAt(0)) : new TextEncoder().encode(decodeURIComponent(m[3]));
  return new File([bytes], t.name, { type: t.type ?? m[1] ?? '' });
}
// a file's bytes as a data URL when they're FILE_KEPT or less, else none
export async function dataOf(f: File): Promise<string | undefined> {
  if (f.size > FILE_KEPT) return undefined;
  try {
    const bytes = new Uint8Array(await f.arrayBuffer());
    let raw = '';
    for (let i = 0; i < bytes.length; i += 0x8000) raw += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return `data:${f.type || 'application/octet-stream'};base64,${btoa(raw)}`;
  } catch {
    return undefined;
  }
}

// The box's draft is gone (its conversation deleted, say)
export const forget = (box: string, store: Storage = localStorage) => { keep(box, '', store, Date.now(), {}); hold(box, []); };

// One message box's keeper: show(box) hands over the box's draft to put back
// (writing the box before's pending words first); typed(text) keeps the words
// DELAY after the last keystroke, or at once when the box is emptied (sent,
// or cleared by hand); carry(extras) keeps its tags and choices the same way;
// flush() writes what's pending now (the window closing)
export function keeper(store: Storage = localStorage, delay = DELAY) {
  // kept: what the store holds for the shown box, so the editor reporting the
  // words put back doesn't count as typing them (and push their week back)
  let shown: string | null = null, kept = '', keptMore = '{}', pending: string | null = null, more: Extras | null = null, timer: ReturnType<typeof setTimeout> | undefined;
  const flush = () => {
    clearTimeout(timer);
    timer = undefined;
    if (shown !== null && (pending !== null || more !== null)) {
      const text = pending ?? kept;
      keep(shown, text, store, Date.now(), more ?? undefined);
      kept = text;
      if (more) keptMore = JSON.stringify(trimmed(more));
    }
    pending = null;
    more = null;
  };
  const later = (now: boolean) => {
    if (now) return flush();
    clearTimeout(timer);
    timer = setTimeout(flush, delay);
  };
  return {
    show(next: string | null): string {
      flush();
      shown = next;
      keptMore = JSON.stringify(next === null ? {} : trimmed(extras(next, store)));
      return (kept = next === null ? '' : draft(next, store));
    },
    // what the box shows besides its words, kept with them
    more: (): Extras => (shown === null ? {} : extras(shown, store)),
    typed(text: string) {
      if (shown === null) return;
      if (text === kept && more === null) { clearTimeout(timer); pending = null; return; }
      pending = text === kept ? null : text;
      later(!text.trim());
    },
    carry(next: Extras) {
      if (shown === null) return;
      if (JSON.stringify(trimmed(next)) === keptMore) { if (pending === null) { clearTimeout(timer); more = null; } return; }
      more = next;
      later(bare(next));
    },
    flush,
  };
}
