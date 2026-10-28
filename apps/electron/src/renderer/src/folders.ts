import { useSyncExternalStore } from 'react';

// The working folders a conversation's work is in (the message box's Select working folder…,
// the Mac's own picker: main's pick-folders), by conversation, remembered across launches. obid
// takes none yet (Wellington decides how: asked 2026-10-28), so for now they're the app's only.
const KEY = 'oby.folders';
// (only lists of paths: anything else that's there is dropped rather than trusted)
const paths = (x: unknown): string[] => (Array.isArray(x) ? x.filter((p): p is string => typeof p === 'string') : []);
const read = (): Record<string, string[]> => {
  try {
    const all = JSON.parse(localStorage.getItem(KEY) ?? '{}');
    return all && typeof all === 'object' ? Object.fromEntries(Object.entries(all).map(([c, f]) => [c, paths(f)])) : {};
  } catch { return {}; }
};
let all = read();
const listeners = new Set<() => void>();
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
const NONE: string[] = [];

export const foldersOf = (conversation: string) => all[conversation] ?? NONE;
// A conversation started here goes by 'new:…' (numbered from 1 again each launch) till obid names
// it: saved, its folders go under obid's id (live.ts says which: realOf, and saves again once it
// knows), so the next launch finds them there; still unnamed, they're kept this launch only
let realOf = (id: string) => id;
export const nameFoldersBy = (f: (id: string) => string) => { realOf = f; };
export function saveFolders() {
  const kept: Record<string, string[]> = {};
  for (const [c, f] of Object.entries(all)) {
    const id = c.startsWith('new:') ? realOf(c) : c;
    if (!id.startsWith('new:')) kept[id] = f;
  }
  try { localStorage.setItem(KEY, JSON.stringify(kept)); } catch { /* this launch only, that's all */ }
}
export function setFolders(conversation: string, folders: string[]) {
  all = { ...all, [conversation]: folders };
  if (!folders.length) delete all[conversation];
  saveFolders();
  listeners.forEach((l) => l());
}
export const useFolders = (conversation?: string) => useSyncExternalStore(subscribe, () => (conversation ? foldersOf(conversation) : NONE));

// a folder as its tag shows it: its own name (the path in its tooltip)
export const folderName = (path: string) => path.replace(/\/+$/, '').split('/').pop() || path;
// The folders picked lately, newest first (8 at most), across conversations and launches: what
// Select working folder… offers before the Mac's picker
const RECENT = 'oby.recentFolders';
let recent: string[] = (() => { try { return paths(JSON.parse(localStorage.getItem(RECENT) ?? '[]')); } catch { return []; } })();
export const useRecentFolders = () => useSyncExternalStore(subscribe, () => recent);
export function usedFolders(paths: string[]) {
  recent = [...paths, ...recent.filter((p) => !paths.includes(p))].slice(0, 8);
  try { localStorage.setItem(RECENT, JSON.stringify(recent)); } catch { /* this launch only */ }
  listeners.forEach((l) => l());
}
// where a folder is, for its line in that list: its parent, home as ~
const parentOf = (path: string) => path.replace(/\/+$/, '').split('/').slice(0, -1).join('/');
export const folderPlace = (path: string) => parentOf(path).replace(/^\/Users\/[^/]+/, '~') || '/';

// the Mac's picker, opening where the last folder was picked (its parent): the folders picked,
// none when cancelled or where there's no picker (a browser)
export const pickFolders = async () => {
  const from = (recent[0] && parentOf(recent[0])) || undefined;
  const picked = (await window.app?.pickFolders?.(from)) ?? [];
  if (picked.length) usedFolders(picked);
  return picked;
};
