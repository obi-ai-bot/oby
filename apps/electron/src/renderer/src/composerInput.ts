import { useEffect, useRef, useState } from 'react';
import { conversationFiles } from './live';
import type { ConversationFile } from './obid';

// What the message box takes besides typing (docs/contracts/composer.md, COMPOSER-12 to 16), each
// as T3 Code's composer does it: files dropped on it, files of the conversation's folders after an
// @, the agent's own commands after a / at its start, and Stop's "Stopping…". Pure where it can be,
// so the tests read it without a window.

// --- files dropped (COMPOSER-13), after T3's workspaceFileDrop.ts

type Transfer = {
  types: ReadonlyArray<string>;
  files: Iterable<File>;
  items?: Iterable<{ kind: string; getAsFile(): File | null; webkitGetAsEntry?(): { isDirectory: boolean } | null }>;
};
export const isFileDrag = (t: Pick<Transfer, 'types'>) => Array.from(t.types).includes('Files');
// The files dropped, and how many folders among them (a folder can't be attached)
export function dropped(t: Transfer): { files: File[]; folders: number } {
  if (!t.items) return { files: Array.from(t.files), folders: 0 };
  const files: File[] = [];
  let folders = 0;
  for (const item of t.items) {
    if (item.kind !== 'file') continue;
    const f = item.getAsFile();
    if (!f) continue;
    if (item.webkitGetAsEntry?.()?.isDirectory) folders++;
    else files.push(f);
  }
  return { files, folders };
}
// What the box says of folders dropped with no file it could take
export const FOLDER_DROPPED = 'Folders can’t be attached. Use + › Select working folder… to work in one.';

// --- @ files (COMPOSER-14)

// The file a path is, as the words name it: from its folder, or (several folders) after its folder's name
export const fileWords = (f: ConversationFile, folders: number) => (folders > 1 ? `${f.folder.split('/').filter(Boolean).pop()}/${f.path}` : f.path);

// The files matching what's typed after an @ in a conversation, asked 120 ms after the last
// keystroke, the latest answer winning (as search.ts does); none while there's no @ or no conversation
export function useMentionFiles(conversation: string | undefined, at: string | null): ConversationFile[] {
  const [files, setFiles] = useState<ConversationFile[]>([]);
  const asked = useRef(0);
  useEffect(() => {
    const n = ++asked.current;
    if (!conversation || at === null) { setFiles([]); return; }
    const t = setTimeout(() => { void conversationFiles(conversation, at).then((f) => { if (n === asked.current) setFiles(f); }); }, 120);
    return () => clearTimeout(t);
  }, [conversation, at]);
  return files;
}

// --- / commands (COMPOSER-16)

// What's typed after a / at the very start of the box, before any space; null: no command being typed
export const slashOf = (text: string) => /^\/(\S*)$/.exec(text)?.[1] ?? null;
// What the list says while obid can list no command of the tool's: what's missing, plainly
export const noCommands = (tool: string) => `${tool}’s own commands can’t be listed here yet: Oby doesn’t read them from its sessions, and words to Oby don’t run them. Say what you want instead.`;

// --- Stop (COMPOSER-12), after T3's useStopPhase

// How long "Stopping…" waits for the turn to end before Stop can be pressed again
export const STOP_PENDING_MAX_MS = 10_000;
// "Stopping…" from a Stop pressed until the turn really ends (`answering` false), or until
// STOP_PENDING_MAX_MS pass with no end, when Stop shows again
export function useStopping(answering: boolean): [boolean, () => void] {
  const [asked, setAsked] = useState(false);
  useEffect(() => { if (!answering) setAsked(false); }, [answering]);
  useEffect(() => {
    if (!asked) return;
    const t = setTimeout(() => setAsked(false), STOP_PENDING_MAX_MS);
    return () => clearTimeout(t);
  }, [asked]);
  return [asked && answering, () => setAsked(true)];
}
