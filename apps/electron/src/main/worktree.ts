import { execFile } from 'node:child_process';
import { existsSync, lstatSync, realpathSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, sep } from 'node:path';
import { HOME, obid, type Trusted } from './obid';
import { takeInput } from './words';
import { ipc } from './ipc';
import { oneOf, string } from '../shared/decode';

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
const JOB_ID = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/;

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

export function worktrees(trusted: Trusted) {
  const channels = ipc(trusted);
  // the editor's name, for the menu's "Open in <editor>"
  channels.handle('worktree-editor', [], () => editorOf().label, null);
  channels.handle('open-worktree', [string, oneOf('terminal', 'editor')], (_e, id, where) => (takeInput() ? openWorktree(id, where) : false), false);
}
