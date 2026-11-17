import { useEffect, useRef, useState } from 'react';
import { agentCommands, conversationFiles } from './live';
import { onObid } from './lean';
import { COMMANDS, FILES } from './mock';
import type { AgentCommand, ConversationFile } from './obid';

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
    // (in a browser, the mock's: obid lists the real ones)
    const ask = () => (onObid ? conversationFiles(conversation, at) : Promise.resolve(FILES.filter((f) => f.path.toLowerCase().includes(at.toLowerCase())).slice(0, 20)));
    const t = setTimeout(() => { void ask().then((f) => { if (n === asked.current) setFiles(f); }); }, 120);
    return () => clearTimeout(t);
  }, [conversation, at]);
  return files;
}

// --- / commands (COMPOSER-16, 17), after T3's composerSlashCommandSearch.ts

// What's typed after a / at the very start of the box, before any space; null: no command being typed
export const slashOf = (text: string) => /^\/(\S*)$/.exec(text)?.[1] ?? null;
// The most commands the list shows
export const COMMANDS_MOST = 50;
// How well a name matches what's typed: it, starts with it, has it after a - _ or :, holds it (lower
// is better), as T3's scoreQueryMatch ranks a command's name; null: not at all
function score(name: string, typed: string): number | null {
  const n = name.toLowerCase();
  if (n === typed) return 0;
  if (n.startsWith(typed)) return 1;
  if (['-', '_', ':'].some((m) => n.includes(m + typed))) return 2;
  if (n.includes(typed)) return 3;
  return null;
}
// The commands matching what's typed after the /, best first and by name within each; every one, in
// Claude Code's order, while nothing is typed
export function searchCommands(commands: AgentCommand[], typed: string): AgentCommand[] {
  const q = typed.replace(/^\/+/, '').trim().toLowerCase();
  if (!q) return commands.slice(0, COMMANDS_MOST);
  return commands
    .map((c) => ({ c, s: score(c.name, q) }))
    .filter((r): r is { c: AgentCommand; s: number } => r.s !== null)
    .sort((a, b) => a.s - b.s || a.c.name.localeCompare(b.c.name))
    .slice(0, COMMANDS_MOST)
    .map((r) => r.c);
}
// What a picked command puts in the box, in place of what was typed: the owner adds what it takes
export const commandWords = (c: AgentCommand) => `/${c.name} `;
// What the list says when it has none to show: a tool whose commands obid can't list, Claude Code's
// before any was seen, or none matching what's typed
export const noCommands = (tool: string) => `${tool}’s own commands can’t be listed here yet. Say what you want instead.`;
export const NO_COMMANDS_YET = 'Claude Code’s commands show here once Oby has talked through it on this Mac.';
export const noMatch = (typed: string) => `No command matches /${typed}.`;

// The owner's own Claude Code's commands, asked when the / list opens (`open`), once each time it
// does; none while it's closed (in a browser, the mock's: obid lists the real ones)
export function useAgentCommands(open: boolean): AgentCommand[] | null {
  const [commands, setCommands] = useState<AgentCommand[] | null>(null);
  useEffect(() => {
    if (!open) { setCommands(null); return; }
    let live = true;
    void (onObid ? agentCommands() : Promise.resolve(COMMANDS)).then((c) => { if (live) setCommands(c); });
    return () => { live = false; };
  }, [open]);
  return commands;
}

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
