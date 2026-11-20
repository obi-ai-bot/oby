import { useEffect, useRef, useState } from 'react';
import { conversationFiles, skillsFor, type SkillsWhere } from './live';
import { onObid } from './lean';
import { FILES, SKILLS } from './mock';
import type { ConversationFile, Skill, SkillsAnswer } from './obid';

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
// keystroke, the latest answer winning (as search.ts does); none while there's no @ or no conversation,
// and none while what's typed isn't what they answer: Tab or Enter never picks from an older answer
// (T3 Code's composerMenuHighlight)
export function useMentionFiles(conversation: string | undefined, at: string | null): ConversationFile[] {
  const [answer, setAnswer] = useState<{ conversation?: string; at: string | null; files: ConversationFile[] }>({ at: null, files: [] });
  const asked = useRef(0);
  useEffect(() => {
    const n = ++asked.current;
    if (!conversation || at === null) { setAnswer({ at: null, files: [] }); return; }
    // (in a browser, the mock's: obid lists the real ones)
    const ask = () => (onObid ? conversationFiles(conversation, at) : Promise.resolve(FILES.filter((f) => f.path.toLowerCase().includes(at.toLowerCase())).slice(0, 20)));
    const t = setTimeout(() => { void ask().then((f) => { if (n === asked.current) setAnswer({ conversation, at, files: f }); }); }, 120);
    return () => clearTimeout(t);
  }, [conversation, at]);
  return answer.conversation === conversation && answer.at === at ? answer.files : [];
}

// --- / commands (COMPOSER-16)

// What's typed after a / at the very start of the box, before any space; null: no command being typed
export const slashOf = (text: string) => /^\/(\S*)$/.exec(text)?.[1] ?? null;
// What the list says while obid can list no command of the tool's: what's missing, plainly
export const noCommands = (tool: string) => `${tool}’s own commands can’t be listed here yet: Oby doesn’t read them from its sessions, and words to Oby don’t run them. Say what you want instead.`;

// --- / skills (COMPOSER-25 to 27), after T3's providerSkillSearch.ts

// The skills the owner can start themselves whose name or description holds what's typed after the /
// (case aside): names starting with it first, then names holding it, then descriptions, each in obid's order
export function skillsMatching(skills: Skill[], typed: string): Skill[] {
  const q = typed.toLowerCase(), offered = skills.filter((s) => s.userInvocable);
  const name = (s: Skill) => s.name.toLowerCase(), about = (s: Skill) => (s.description ?? '').toLowerCase();
  const first = offered.filter((s) => name(s).startsWith(q));
  const then = offered.filter((s) => !first.includes(s) && name(s).includes(q));
  return [...first, ...then, ...offered.filter((s) => !first.includes(s) && !then.includes(s) && about(s).includes(q))];
}

// The skills obid gives for the box's words (where), asked each time the / list opens and kept while
// it's open; null while it's closed or still being asked. In a browser, the mock's
export function useSkills(where: SkillsWhere | undefined, open: boolean): SkillsAnswer | null {
  const [answer, setAnswer] = useState<SkillsAnswer | null>(null);
  const key = open && where ? JSON.stringify(where) : null;
  useEffect(() => {
    setAnswer(null);
    if (key === null) return;
    let gone = false;
    void (onObid ? skillsFor(JSON.parse(key) as SkillsWhere) : Promise.resolve(SKILLS)).then((a) => { if (!gone) setAnswer(a); });
    return () => { gone = true; };
  }, [key]);
  return answer;
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
