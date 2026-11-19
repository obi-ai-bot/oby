import { execFile } from 'node:child_process';
import { existsSync, lstatSync, realpathSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { isAbsolute, join, resolve, sep } from 'node:path';
import { HOME, obid, type Trusted } from './obid';
import { obidState } from './live';
import { takeInput } from './words';
import { ipc } from './ipc';
import { oneOf, string, type Decoder } from '../shared/decode';

// Open in Terminal, Open in <editor> (docs/contracts/task-changes.md, CHANGES-6 and 7): a task's
// worktree, in a program of the owner's own. The page sends only the job's id: main makes the path
// itself, obid's `<home>/worktrees/<id>`, and opens it only when obid has that job and the folder is
// a real one inside obid's worktrees. The program is the Mac's Terminal, or the first editor of
// T3 Code's "Open in" list that is installed, else Finder: never one a job, an agent or the page names.

// the editors looked for, in order, by their app in /Applications or the owner's own Applications
export const EDITORS = [
  { label: 'Cursor', app: 'Cursor.app' },
  { label: 'VS Code', app: 'Visual Studio Code.app' },
  { label: 'Zed', app: 'Zed.app' },
] as const;

export type Editor = { label: string; app?: string };

// the owner's editor: the first installed, else Finder
export function editorOf(exists: (path: string) => boolean = existsSync, home = homedir()): Editor {
  for (const e of EDITORS) {
    for (const dir of ['/Applications', join(home, 'Applications')]) {
      const app = join(dir, e.app);
      if (exists(app)) return { label: e.label, app };
    }
  }
  return { label: 'Finder' };
}

// a job's id as obid makes them: no path in it
export const JOB_ID = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/;

// job `id`'s worktree under obid's home `home`, its real path: null for an id that isn't one, or a
// folder that isn't a real one inside `home/worktrees` (missing, a file, a link, or one out of it)
export function worktreeOf(home: string, id: string): string | null {
  if (!JOB_ID.test(id)) return null;
  const root = join(home, 'worktrees'), dir = join(root, id);
  try {
    const st = lstatSync(dir);
    if (st.isSymbolicLink() || !st.isDirectory()) return null;
    const real = realpathSync(dir);
    return real.startsWith(realpathSync(root) + sep) ? real : null;
  } catch {
    return null;
  }
}

// the program and its arguments that open `dir`: `open -a Terminal`, `open -a <editor>`, or Finder
export function openCommand(where: 'terminal' | 'editor', dir: string, editor: Editor = editorOf()): [string, string[]] {
  if (where === 'terminal') return ['/usr/bin/open', ['-a', 'Terminal', dir]];
  return editor.app ? ['/usr/bin/open', ['-a', editor.app, dir]] : ['/usr/bin/open', [dir]];
}

// Opens job `id`'s worktree in Terminal or the editor: true once `open` started it, false when main
// opens nothing (not a click main saw, not a Mac, a job obid doesn't have, no such folder)
export async function openWorktree(id: string, where: 'terminal' | 'editor', run = execFile): Promise<boolean> {
  if (process.platform !== 'darwin') return false;
  const dir = worktreeOf(HOME, id);
  if (!dir) return false;
  const job = await obid('GET', `/v1/jobs/${encodeURIComponent(id)}`);
  if (job.status !== 200) return false;
  const [program, args] = openCommand(where, dir);
  return new Promise((resolve) => { run(program, args, (err) => resolve(!err)); });
}

// A path in a reply (docs/contracts/chat-first.md, CHATFIRST-39 and 40): `src/x.ts`, `src/x.ts:42`
// or `src/x.ts:42:7`, written by an agent or Oby, so anyone's words. The page sends only the words
// and where the reply is (a task, by its id; a conversation, by its id): main makes the folders
// itself, the task's worktree or the conversation's folders as obid has them, and a path is a file
// only when it is a real file inside one of them, after every link in it is followed. Anything
// else (outside them, `..` out, a folder, a link out, a URL, a scheme) is no file, and stays text.
export type Place = 'task' | 'conversation';
export type FileAt = { file: string; line?: number; column?: number };

// the folders a reply's paths are read from, each real: a task's worktree; a conversation's folders
// (its project's while it's in one), only those obid lists for it, each a real folder
export function foldersOf(place: Place, id: string, home = HOME): string[] {
  if (place === 'task') { const dir = worktreeOf(home, id); return dir ? [dir] : []; }
  const folders = obidState().conversations.find((c) => c.id === id)?.folders ?? [];
  return folders.flatMap((f) => {
    if (typeof f !== 'string' || !isAbsolute(f)) return [];
    try { const real = realpathSync(f); return statSync(real).isDirectory() ? [real] : []; } catch { return []; }
  });
}

// `path[:line[:column]]` as a reply writes it, at most 1,024 characters and no control character
const REF = /^([^\0-\x1f\x7f]+?)(?::(\d{1,7})(?::(\d{1,7}))?)?$/;
// the file a reply's path names in `folders`, or null when it names none of theirs
export function fileIn(folders: string[], ref: string, home = homedir()): FileAt | null {
  if (ref.length > 1024) return null;
  const m = REF.exec(ref);
  if (!m || /^[a-z][a-z\d+.-]*:/i.test(m[1])) return null;
  const path = m[1].startsWith('~/') ? join(home, m[1].slice(2)) : m[1];
  for (const root of folders) {
    const target = isAbsolute(path) ? path : resolve(root, path);
    try {
      const real = realpathSync(target);
      if (!real.startsWith(root + sep) || !statSync(real).isFile()) continue;
      const line = m[2] ? Number(m[2]) : undefined, column = m[3] ? Number(m[3]) : undefined;
      return { file: real, ...(line ? { line } : {}), ...(line && column ? { column } : {}) };
    } catch { /* not there: the next folder */ }
  }
  return null;
}

// the program and arguments that open `at` in the owner's editor, at its line: the editor's own
// command-line tool inside its app (Cursor's and VS Code's `--goto`, Zed's `cli`), else the app
// opened on the file (at its first line), else Finder showing it (never `open` on the file itself,
// which would run a script or an app)
export function openFileCommand(at: FileAt, editor: Editor = editorOf(), exists: (path: string) => boolean = existsSync): [string, string[]] {
  const spot = at.line && !at.file.includes(':') ? `${at.file}:${at.line}${at.column ? `:${at.column}` : ''}` : at.file;
  if (editor.app) {
    const cli = editor.label === 'Zed' ? join(editor.app, 'Contents/MacOS/cli')
      : join(editor.app, 'Contents/Resources/app/bin', editor.label === 'Cursor' ? 'cursor' : 'code');
    if (exists(cli)) return editor.label === 'Zed' ? [cli, [spot]] : [cli, spot === at.file ? [at.file] : ['--goto', spot]];
    return ['/usr/bin/open', ['-a', editor.app, at.file]];
  }
  return ['/usr/bin/open', ['-R', at.file]];
}

// which of a reply's paths are files it may open (the page links those, the rest stay text)
export const filesOf = (place: Place, id: string, refs: string[]) => {
  const folders = foldersOf(place, id);
  return refs.map((r) => !!folders.length && !!fileIn(folders, r));
};

// Opens a reply's path in the owner's editor: true once started, false when it names no file of the
// reply's task or conversation (or not a Mac, or a task obid doesn't have)
export async function openFile(place: Place, id: string, ref: string, run = execFile): Promise<boolean> {
  if (process.platform !== 'darwin') return false;
  const at = fileIn(foldersOf(place, id), ref);
  if (!at) return false;
  if (place === 'task' && (await obid('GET', `/v1/jobs/${encodeURIComponent(id)}`)).status !== 200) return false;
  const [program, args] = openFileCommand(at);
  return new Promise((done) => { run(program, args, (err) => done(!err)); });
}

// a reply's paths, as the page sends them: at most 200, each a string
const refs: Decoder<string[]> = (v) => (Array.isArray(v) && v.length <= 200 && v.every((r) => typeof r === 'string') ? { value: v as string[] } : undefined);

export function worktrees(trusted: Trusted) {
  const channels = ipc(trusted);
  // the editor's name, for the menu's "Open in <editor>"
  channels.handle('worktree-editor', [], () => editorOf().label, null);
  channels.handle('open-worktree', [string, oneOf('terminal', 'editor')], (_e, id, where) => (takeInput() ? openWorktree(id, where) : false), false);
  // a reply's paths: which are files to link (CHATFIRST-39), and one opened, right after a click main saw (CHATFIRST-40)
  channels.handle('reply-files', [oneOf('task', 'conversation'), string, refs], (_e, place, id, list) => filesOf(place, id, list), [] as boolean[]);
  channels.handle('open-reply-file', [oneOf('task', 'conversation'), string, string], (_e, place, id, ref) => (takeInput() ? openFile(place, id, ref) : false), false);
}
