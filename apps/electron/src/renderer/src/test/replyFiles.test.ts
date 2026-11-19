import { mkdirSync, mkdtempSync, realpathSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// A path in a reply, as main reads it (docs/contracts/chat-first.md, CHATFIRST-39 and 40): a file
// only inside the reply's task's worktree or its conversation's folders, after every link is
// followed, and opened in the owner's editor at its line; anything else names no file.
const the = vi.hoisted(() => ({ home: '', jobs: new Set<string>(), conversations: [] as { id: string; folders?: string[] }[] }));
vi.mock('electron', () => ({ ipcMain: { handle() {}, on() {} } }));
vi.mock('../../../main/obid', () => ({
  get HOME() { return the.home; },
  obid: async (_method: string, path: string) => (the.jobs.has(decodeURIComponent(path.split('/').at(-1)!)) ? { status: 200, text: '{}' } : { status: 404, text: '{}' }),
}));
vi.mock('../../../main/live', () => ({ obidState: () => ({ conversations: the.conversations }) }));

import { fileIn, filesOf, foldersOf, openFile, openFileCommand } from '../../../main/worktree';

const platform = Object.getOwnPropertyDescriptor(process, 'platform')!;
let tree = '', folder = '', outside = '';
beforeEach(() => {
  the.home = realpathSync(mkdtempSync(join(tmpdir(), 'oby-reply-')));
  tree = join(the.home, 'worktrees', 'j1');
  mkdirSync(join(tree, 'src'), { recursive: true });
  writeFileSync(join(tree, 'src', 'x.ts'), 'export {};\n');
  folder = join(the.home, 'project');
  mkdirSync(join(folder, 'docs'), { recursive: true });
  writeFileSync(join(folder, 'docs', 'plan.md'), '# Plan\n');
  outside = join(the.home, 'secret.txt');
  writeFileSync(outside, 'no\n');
  symlinkSync(outside, join(tree, 'src', 'out.txt'));
  the.jobs = new Set(['j1']);
  the.conversations = [{ id: 'c1', folders: [folder] }];
  Object.defineProperty(process, 'platform', { ...platform, value: 'darwin' });
  return () => Object.defineProperty(process, 'platform', platform);
});
const runner = () => {
  const ran: [string, string[]][] = [];
  const run = ((program: string, args: string[], done: (err: Error | null) => void) => { ran.push([program, args]); done(null); }) as unknown as typeof import('node:child_process').execFile;
  return { ran, run };
};

describe('CHATFIRST-39: a path in a reply is a file of its task or its conversation', () => {
  it('chatfirst_39_a_tasks_path_is_read_in_its_worktree_with_its_line_and_column', () => {
    expect(foldersOf('task', 'j1')).toEqual([tree]);
    expect(fileIn([tree], 'src/x.ts')).toEqual({ file: join(tree, 'src', 'x.ts') });
    expect(fileIn([tree], 'src/x.ts:42')).toEqual({ file: join(tree, 'src', 'x.ts'), line: 42 });
    expect(fileIn([tree], './src/x.ts:42:7')).toEqual({ file: join(tree, 'src', 'x.ts'), line: 42, column: 7 });
    expect(fileIn([tree], join(tree, 'src', 'x.ts'))).toEqual({ file: join(tree, 'src', 'x.ts') });
  });

  it('chatfirst_39_a_conversations_path_is_read_in_its_folders_as_obid_lists_them', () => {
    expect(foldersOf('conversation', 'c1')).toEqual([folder]);
    expect(filesOf('conversation', 'c1', ['docs/plan.md', 'src/x.ts', 'docs/nope.md'])).toEqual([true, false, false]);
    // one obid doesn't list, or whose folders are gone, has none
    expect(filesOf('conversation', 'c2', ['docs/plan.md'])).toEqual([false]);
    the.conversations = [{ id: 'c1', folders: [join(the.home, 'gone'), 'relative/dir'] }];
    expect(foldersOf('conversation', 'c1')).toEqual([]);
  });

  it('chatfirst_39_a_path_that_isnt_a_file_there_names_none', () => {
    for (const ref of ['src/nope.ts', 'src', '../secret.txt', 'src/../../../secret.txt', outside, 'src/out.txt', '~/secret.txt',
      'https://example.com/src/x.ts', 'file:///etc/hosts', 'vscode://file/etc/hosts', `src/x.ts\u0000`, `${'a/'.repeat(600)}x.ts`]) {
      expect(fileIn([tree], ref), ref).toBeNull();
    }
    // a task obid has no worktree for: no folders, so no file
    expect(filesOf('task', 'j2', ['src/x.ts'])).toEqual([false]);
    expect(filesOf('task', '../j1', ['src/x.ts'])).toEqual([false]);
  });
});

describe('CHATFIRST-40: a file opens in the owner\'s editor at its line, and nothing else opens', () => {
  it('chatfirst_40_the_editors_own_tool_opens_it_at_its_line_else_the_app_else_finder_shows_it', () => {
    const at = { file: '/w/j1/src/x.ts', line: 42, column: 7 };
    const has = (...p: string[]) => (x: string) => p.includes(x);
    const cursor = { label: 'Cursor', app: '/Applications/Cursor.app' };
    expect(openFileCommand(at, cursor, has('/Applications/Cursor.app/Contents/Resources/app/bin/cursor')))
      .toEqual(['/Applications/Cursor.app/Contents/Resources/app/bin/cursor', ['--goto', '/w/j1/src/x.ts:42:7']]);
    const code = { label: 'VS Code', app: '/Applications/Visual Studio Code.app' };
    expect(openFileCommand({ file: '/w/j1/src/x.ts', line: 3 }, code, has('/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code')))
      .toEqual(['/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code', ['--goto', '/w/j1/src/x.ts:3']]);
    const zed = { label: 'Zed', app: '/Applications/Zed.app' };
    expect(openFileCommand(at, zed, has('/Applications/Zed.app/Contents/MacOS/cli'))).toEqual(['/Applications/Zed.app/Contents/MacOS/cli', ['/w/j1/src/x.ts:42:7']]);
    // no tool inside the app: the app on the file; no editor: Finder shows it, never runs it
    expect(openFileCommand(at, cursor, has())).toEqual(['/usr/bin/open', ['-a', '/Applications/Cursor.app', '/w/j1/src/x.ts']]);
    expect(openFileCommand(at, { label: 'Finder' }, has())).toEqual(['/usr/bin/open', ['-R', '/w/j1/src/x.ts']]);
    // no line: just the file
    expect(openFileCommand({ file: '/w/j1/src/x.ts' }, cursor, () => true)[1]).toEqual(['/w/j1/src/x.ts']);
  });

  it('chatfirst_40_only_a_file_of_the_reply_opens', async () => {
    const { ran, run } = runner();
    expect(await openFile('task', 'j1', 'src/x.ts:42', run)).toBe(true);
    expect(ran).toHaveLength(1);
    expect(ran[0][1].join(' ')).toContain(join(tree, 'src', 'x.ts'));
    expect(await openFile('conversation', 'c1', 'docs/plan.md', run)).toBe(true);
    for (const [place, id, ref] of [['task', 'j1', '../secret.txt'], ['task', 'j1', 'src/out.txt'], ['task', 'j1', outside],
      ['task', 'j1', 'https://example.com'], ['conversation', 'c1', 'src/x.ts'], ['conversation', 'c9', 'docs/plan.md']] as const) {
      expect(await openFile(place, id, ref, run), `${place} ${id} ${ref}`).toBe(false);
    }
    // a worktree left on disk for a task obid no longer has opens nothing
    the.jobs = new Set();
    expect(await openFile('task', 'j1', 'src/x.ts', run)).toBe(false);
    expect(ran).toHaveLength(2);
  });
});
