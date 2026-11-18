import { mkdirSync, mkdtempSync, realpathSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// Open in Terminal, Open in <editor> (docs/contracts/task-changes.md, CHANGES-6 and 7): main opens
// obid's worktree of a job it has, a real folder inside obid's worktrees, in the Mac's Terminal or
// the first installed editor, else Finder; never a path or a program the page names.
const the = vi.hoisted(() => ({ home: '', jobs: new Set<string>(), asked: [] as string[] }));
vi.mock('electron', () => ({ ipcMain: { handle() {}, on() {} } }));
vi.mock('../../../main/obid', () => ({
  get HOME() { return the.home; },
  obid: async (method: string, path: string) => {
    the.asked.push(`${method} ${path}`);
    const id = decodeURIComponent(path.split('/').at(-1)!);
    return the.jobs.has(id) ? { status: 200, text: '{}' } : { status: 404, text: '{"error":"There’s no such job."}' };
  },
}));

import { editorOf, openCommand, openWorktree, worktreeOf } from '../../../main/worktree';

const platform = Object.getOwnPropertyDescriptor(process, 'platform')!;
beforeEach(() => {
  the.home = realpathSync(mkdtempSync(join(tmpdir(), 'oby-worktree-')));
  mkdirSync(join(the.home, 'worktrees', 'j1'), { recursive: true });
  the.jobs = new Set(['j1']);
  the.asked = [];
  Object.defineProperty(process, 'platform', { ...platform, value: 'darwin' });
  return () => Object.defineProperty(process, 'platform', platform);
});

// a stand-in for execFile: what it was asked to run
const runner = () => {
  const ran: [string, string[]][] = [];
  const run = ((program: string, args: string[], done: (err: Error | null) => void) => { ran.push([program, args]); done(null); }) as unknown as typeof import('node:child_process').execFile;
  return { ran, run };
};

describe('CHANGES-6: the worktree opens in Terminal or the owner\'s editor', () => {
  it('changes_6_terminal_opens_obids_worktree_of_the_job', async () => {
    const { ran, run } = runner();
    expect(await openWorktree('j1', 'terminal', run)).toBe(true);
    expect(ran).toEqual([['/usr/bin/open', ['-a', 'Terminal', join(the.home, 'worktrees', 'j1')]]]);
    expect(the.asked).toEqual(['GET /v1/jobs/j1']);
  });
  it('changes_6_the_editor_is_the_first_installed_of_cursor_vs_code_and_zed_else_finder', () => {
    const only = (...apps: string[]) => (p: string) => apps.includes(p);
    expect(editorOf(only('/Applications/Cursor.app', '/Applications/Visual Studio Code.app'), '/Users/o')).toEqual({ label: 'Cursor', app: '/Applications/Cursor.app' });
    expect(editorOf(only('/Applications/Visual Studio Code.app', '/Applications/Zed.app'), '/Users/o')).toEqual({ label: 'VS Code', app: '/Applications/Visual Studio Code.app' });
    expect(editorOf(only('/Users/o/Applications/Zed.app'), '/Users/o')).toEqual({ label: 'Zed', app: '/Users/o/Applications/Zed.app' });
    expect(editorOf(only(), '/Users/o')).toEqual({ label: 'Finder' });
    expect(openCommand('editor', '/w/j1', { label: 'Zed', app: '/Applications/Zed.app' })).toEqual(['/usr/bin/open', ['-a', '/Applications/Zed.app', '/w/j1']]);
    expect(openCommand('editor', '/w/j1', { label: 'Finder' })).toEqual(['/usr/bin/open', ['/w/j1']]);
  });
});

describe('CHANGES-7: main opens nothing it didn\'t make the path of', () => {
  it('changes_7_an_id_that_isnt_one_a_job_obid_lacks_a_file_or_a_link_out_open_nothing', async () => {
    const worktrees = join(the.home, 'worktrees');
    writeFileSync(join(worktrees, 'file'), '');
    const outside = mkdtempSync(join(tmpdir(), 'oby-outside-'));
    symlinkSync(outside, join(worktrees, 'link'));
    for (const id of ['../x', '/etc', 'a/b', '', '.hidden']) expect(worktreeOf(the.home, id), id).toBeNull();
    for (const id of ['file', 'link', 'missing']) expect(worktreeOf(the.home, id), id).toBeNull();
    expect(worktreeOf(the.home, 'j1')).toBe(join(worktrees, 'j1'));
    const { ran, run } = runner();
    the.jobs.add('link');
    for (const id of ['../x', 'file', 'link', 'missing']) expect(await openWorktree(id, 'terminal', run), id).toBe(false);
    // a folder on disk for a job obid doesn't have
    mkdirSync(join(worktrees, 'gone'));
    expect(await openWorktree('gone', 'editor', run)).toBe(false);
    expect(ran).toEqual([]);
  });
});
