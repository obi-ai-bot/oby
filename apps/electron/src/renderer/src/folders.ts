import { useSyncExternalStore } from 'react';

// The working folders a conversation's work is in (the message box's Select working folder…,
// the Mac's own picker: main's pick-folders), by conversation. Once obid takes them (its status
// says folders: true, docs/contracts/folders.md) they're obid's: shown from its conversations
// (live.ts: takeFolders) and changed there (PATCH, sendFoldersBy). An older obid takes none, and
// the app keeps them itself, remembered across launches.
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
// obid's (live.ts): a conversation's new folders sent to obid, when it takes folders and knows the
// conversation (else nothing: undefined), settled once obid answered; its answer comes back
// through takeFolders. A change it refuses is said (report).
let send: (conversation: string, folders: string[]) => Promise<unknown> | undefined = () => undefined;
let report: (e: unknown) => void = () => {};
export const sendFoldersBy = (f: typeof send, r: typeof report) => { send = f; report = r; };
let obidKeeps = false;
export function saveFolders() {
  if (obidKeeps) return;
  const kept: Record<string, string[]> = {};
  for (const [c, f] of Object.entries(all)) {
    const id = c.startsWith('new:') ? realOf(c) : c;
    if (!id.startsWith('new:')) kept[id] = f;
  }
  try { localStorage.setItem(KEY, JSON.stringify(kept)); } catch { /* this launch only, that's all */ }
}
export function setFolders(conversation: string, folders: string[]) {
  const sent = send(conversation, folders);
  if (sent) return void sent.catch(report);
  all = { ...all, [conversation]: folders };
  if (!folders.length) delete all[conversation];
  saveFolders();
  listeners.forEach((l) => l());
}
// Only the conversations still there keep theirs: given obid's whole list (live.ts, on each sync),
// the folders of ones deleted since are dropped (a new one not yet named keeps its own)
export function keepFoldersOf(conversations: Set<string>) {
  const gone = Object.keys(all).filter((c) => !c.startsWith('new:') && !conversations.has(c));
  if (!gone.length) return;
  all = Object.fromEntries(Object.entries(all).filter(([c]) => !gone.includes(c)));
  saveFolders();
  listeners.forEach((l) => l());
}
// The folders of `conversation` obid refused of `folders`: all of them sent at once, else (one
// refused fails the whole change) one at a time, so every folder obid takes moves
async function moved(conversation: string, folders: string[]): Promise<string[]> {
  const tries = async (f: string[]) => {
    const sent = send(conversation, f);
    if (!sent) return false;
    try { await sent; return true; } catch { return false; }
  };
  if (await tries(folders)) return [];
  const took: string[] = [], refused: string[] = [];
  for (const f of folders) (await tries([...took, f]) ? took : refused).push(f);
  return refused;
}
// obid's folders for every conversation it lists, by the app's id, whenever they may have changed:
// they replace the app's own. The first time, folders the app kept for a conversation obid has none
// for are sent to it; the app's copy goes once they're all there, and keeps only what obid refused
// (tried again at the next launch). The recent list stays the app's. Settles once they're sent.
let taken = '';
export function takeFolders(byConversation: Record<string, string[]>, storage: Storage = localStorage): Promise<void> {
  let moving = Promise.resolve();
  if (!obidKeeps) {
    obidKeeps = true;
    const kept = Object.entries(all).filter(([c, f]) => !c.startsWith('new:') && f.length && !byConversation[c]?.length);
    moving = Promise.all(kept.map(async ([c, f]) => [c, await moved(c, f)] as const)).then((results) => {
      const refused = results.filter(([, f]) => f.length);
      try {
        if (refused.length) storage.setItem(KEY, JSON.stringify(Object.fromEntries(refused)));
        else storage.removeItem(KEY);
      } catch { /* kept as it was */ }
    });
  }
  const seen = JSON.stringify(byConversation);
  if (seen === taken) return moving;
  taken = seen;
  // (one started here and not yet named keeps the folders it was given)
  const unnamed = Object.entries(all).filter(([c]) => c.startsWith('new:') && !(c in byConversation));
  all = Object.fromEntries([...unnamed, ...Object.entries(byConversation).filter(([, f]) => f.length)]);
  listeners.forEach((l) => l());
  return moving;
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
