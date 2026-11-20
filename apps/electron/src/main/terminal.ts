import { app, clipboard, type WebContents } from 'electron';
import { chmodSync, existsSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { userInfo } from 'node:os';
import { basename, dirname, isAbsolute, join } from 'node:path';
import { HOME, obid, type Trusted } from './obid';
import { obidState, onLive } from './live';
import { takeInput } from './words';
import { mayWrite } from './terminalInput';
import { foldersOf, worktreeOf } from './worktree';
import { ipc } from './ipc';
import { oneOf, safeInteger, string, type Decoder } from '../shared/decode';

// The owner's own shell in a task's or a conversation's working copy, in Oby's window
// (docs/contracts/terminal.md), as T3 Code's terminal drawer: run by main, on the owner's own click
// (TERMINAL-1), as the owner, with no agent and no box, as their Terminal would. The page sends only
// where (a task or a conversation, by its id): main finds the folder itself (TERMINAL-2), starts the
// owner's login shell there with a clean environment that carries nothing of Oby's (TERMINAL-3), and
// kills it when its tab or panel closes, the page goes, its task or conversation is removed, or Oby
// quits (TERMINAL-4). What the page types reaches the shell only as main saw it typed (TERMINAL-5,
// terminalInput.ts).

export type Place = 'task' | 'conversation';
type Disposable = { dispose(): void };
// node-pty's pty, as far as main uses it
export type Pty = {
  pid: number;
  onData(f: (data: string) => void): Disposable;
  onExit(f: (e: { exitCode: number; signal?: number }) => void): Disposable;
  write(data: string): void;
  resize(cols: number, rows: number): void;
  kill(signal?: string): void;
};
export type Spawn = (file: string, args: string[], options: { name: string; cols: number; rows: number; cwd: string; env: Record<string, string>; encoding: 'utf8' }) => Pty;

// at most this many shells at once, all places together
export const MAX_TERMINALS = 8;
// how long a closed shell has to end after SIGHUP and SIGTERM before SIGKILL
export const KILL_GRACE_MS = 1000;
// output that arrives within this long goes to the page in one message
const BATCH_MS = 4;
// a size xterm.js can have: columns and rows
const size = (cols: number, rows: number) => Number.isInteger(cols) && Number.isInteger(rows) && cols >= 2 && cols <= 1000 && rows >= 1 && rows <= 500;

// The owner's login shell, as macOS's directory has it for their user (never $SHELL, which the
// app's environment may carry from anywhere): an absolute path to a file, else zsh (bash off a Mac)
export function shellOf(info: { shell: string | null } = userInfo(), exists: (p: string) => boolean = existsSync, platform = process.platform): string {
  const s = info.shell;
  if (s && isAbsolute(s) && exists(s)) return s;
  return platform === 'darwin' ? '/bin/zsh' : '/bin/bash';
}
// a login shell (-l), as Terminal starts one: it reads the owner's profile, which sets their PATH
export const shellArgs = (_shell: string): string[] => ['-l'];

// The variables main gives the shell (TERMINAL-3): who the owner is, where their home and temporary
// folder are, the system's PATH (the login shell's profile adds theirs), a UTF-8 locale, what kind of
// terminal this is, and the owner's own ssh-agent and macOS text encoding when the app has them.
// Nothing else of the app's environment: no token of obid's or anyone's, no vault secret, no
// OBID_*, ELECTRON_* or NODE_* variable, nothing an agent or the app's launch set.
const PASSED = ['TMPDIR', 'SSH_AUTH_SOCK', '__CF_USER_TEXT_ENCODING'] as const;
export function cleanEnv(base: NodeJS.ProcessEnv, who: { home: string; user: string; shell: string }, version: string): Record<string, string> {
  const env: Record<string, string> = {
    HOME: who.home, USER: who.user, LOGNAME: who.user, SHELL: who.shell,
    PATH: '/usr/bin:/bin:/usr/sbin:/sbin',
    // UTF-8, whatever the app was started with: a terminal that isn't would garble anything past ASCII
    LANG: base.LANG && /utf-?8$/i.test(base.LANG) ? base.LANG : 'en_US.UTF-8',
    TERM: 'xterm-256color', COLORTERM: 'truecolor', TERM_PROGRAM: 'Oby', TERM_PROGRAM_VERSION: version,
  };
  for (const k of PASSED) { const v = base[k]; if (typeof v === 'string' && v && !/[\0\n]/.test(v)) env[k] = v; }
  return env;
}

// The folder a place's shell starts in (TERMINAL-2): a task's worktree, as obid keeps it under its
// home (worktree.ts) and only while obid has the job; a conversation's first folder, as obid lists
// it (its project's while it's in one). Null: none on this Mac.
export async function cwdOf(place: Place, id: string, ask = obid): Promise<string | null> {
  if (place === 'conversation') return foldersOf('conversation', id)[0] ?? null;
  const dir = worktreeOf(HOME, id);
  if (!dir) return null;
  return (await ask('GET', `/v1/jobs/${encodeURIComponent(id)}`)).status === 200 ? dir : null;
}

type Term = { id: string; place: Place; of: string; pty: Pty; page: WebContents | null; out: string; flush: ReturnType<typeof setTimeout> | null; cols: number; rows: number; exited: boolean; closing: boolean };
export type Opened = { term: string; cwd: string; name: string } | { error: string };

// The shells main holds, each for one page, one place. send: to the page that opened it
export class Terminals {
  private terms = new Map<string, Term>();
  private next = 1;
  // shells closed and not yet gone: SIGKILLed when Oby quits (TERMINAL-4)
  private dying = new Set<Pty>();
  constructor(private spawn: () => Spawn, private send: (page: WebContents | null, channel: string, ...args: unknown[]) => void,
    private env: () => { env: Record<string, string>; shell: string }, private graceMs = KILL_GRACE_MS) {}

  get open() { return [...this.terms.values()].filter((t) => !t.exited).length; }
  // the places that have a shell, for removal (TERMINAL-4)
  places() { return [...new Set([...this.terms.values()].map((t) => `${t.place}:${t.of}`))].map((k) => { const i = k.indexOf(':'); return { place: k.slice(0, i) as Place, of: k.slice(i + 1) }; }); }

  start(page: WebContents | null, place: Place, of: string, cwd: string, cols: number, rows: number): Opened {
    if (this.open >= MAX_TERMINALS) return { error: `Oby keeps ${MAX_TERMINALS} terminals open at most: close one first.` };
    try { if (!statSync(cwd).isDirectory()) return { error: 'Its folder isn’t on this Mac any more.' }; } catch { return { error: 'Its folder isn’t on this Mac any more.' }; }
    const { env, shell } = this.env();
    let pty: Pty;
    try {
      pty = this.spawn()(shell, shellArgs(shell), { name: 'xterm-256color', cols, rows, cwd, env, encoding: 'utf8' });
    } catch (e) {
      return { error: `The shell didn’t start: ${e instanceof Error ? e.message : String(e)}` };
    }
    const id = `t${this.next++}`;
    const t: Term = { id, place, of, pty, page, out: '', flush: null, cols, rows, exited: false, closing: false };
    this.terms.set(id, t);
    // output in batches (a busy program prints thousands of chunks a second), as it came: node-pty
    // decodes UTF-8 across chunks itself (encoding), so a character split between two reads stays whole
    pty.onData((data) => {
      t.out += data;
      t.flush ??= setTimeout(() => this.flush(t), BATCH_MS);
    });
    pty.onExit(({ exitCode, signal }) => {
      t.exited = true;
      this.dying.delete(pty);
      this.flush(t);
      this.send(t.page, 'terminal-exit', id, exitCode, signal ?? null);
      this.terms.delete(id);
    });
    return { term: id, cwd, name: basename(cwd) };
  }

  private flush(t: Term) {
    if (t.flush) clearTimeout(t.flush);
    t.flush = null;
    if (!t.out) return;
    const out = t.out;
    t.out = '';
    if (!t.closing) this.send(t.page, 'terminal-data', t.id, out);
  }

  // the page's bytes, once mayWrite said the owner typed them
  write(id: string, data: string) {
    const t = this.terms.get(id);
    if (!t || t.exited || t.closing) return false;
    t.pty.write(data);
    return true;
  }

  // A new size: only a real one, changed, to a shell still there (a resize that races its exit is
  // dropped, never thrown). False: nothing done
  resize(id: string, cols: number, rows: number) {
    const t = this.terms.get(id);
    if (!t || t.exited || t.closing || !size(cols, rows) || (t.cols === cols && t.rows === rows)) return false;
    try { t.pty.resize(cols, rows); } catch { return false; }
    t.cols = cols; t.rows = rows;
    return true;
  }

  // Ends a shell and what it runs: SIGHUP, which an interactive shell passes to its jobs (it ignores
  // SIGTERM), then SIGTERM, then SIGKILL if it's still there after the grace
  close(id: string) {
    const t = this.terms.get(id);
    if (!t) return false;
    this.terms.delete(id);
    if (t.flush) clearTimeout(t.flush);
    t.closing = true;
    if (t.exited) return true;
    this.dying.add(t.pty);
    for (const sig of ['SIGHUP', 'SIGTERM']) { if (t.exited) break; try { t.pty.kill(sig); } catch { /* gone already */ } }
    const timer = setTimeout(() => { if (this.dying.delete(t.pty)) { try { t.pty.kill('SIGKILL'); } catch { /* gone */ } } }, this.graceMs);
    timer.unref?.();
    return true;
  }
  closeWhere(f: (t: { place: Place; of: string; page: WebContents | null }) => boolean) {
    for (const t of [...this.terms.values()]) if (f(t)) this.close(t.id);
  }
  // Oby quits: every shell closed, then those still there killed outright, before the app ends
  closeAll() { for (const id of [...this.terms.keys()]) this.close(id); }
  killLeft() {
    for (const pty of this.dying) { try { pty.kill('SIGKILL'); } catch { /* gone */ } }
    this.dying.clear();
  }
}

// node-pty, loaded only when a shell first starts (its native part isn't needed before). Its macOS
// helper must be executable: an install that lost its mode gets it back (T3 Code's fix too)
let loaded: Spawn | null = null;
export function nodePty(): Spawn {
  if (loaded) return loaded;
  const require = createRequire(import.meta.url);
  const pty = require('node-pty') as { spawn: Spawn };
  if (process.platform === 'darwin') {
    const helper = join(dirname(require.resolve('node-pty/package.json')), 'prebuilds', `darwin-${process.arch}`, 'spawn-helper')
      .replace('app.asar', 'app.asar.unpacked');
    try { if (!(statSync(helper).mode & 0o111)) chmodSync(helper, 0o755); } catch { /* none there, or the app's own folder isn't writable: package.sh made it executable */ }
  }
  loaded = pty.spawn;
  return loaded;
}

const placeOf = oneOf('task', 'conversation');
const id: Decoder<string> = (v) => (typeof v === 'string' && /^[A-Za-z0-9:_-]{1,128}$/.test(v) ? { value: v } : undefined);
// a write: at most 1 MB at once (a big paste)
const bytes: Decoder<string> = (v) => (typeof v === 'string' && v.length > 0 && v.length <= 1 << 20 ? { value: v } : undefined);

// the owner's login shell and the environment it gets, read for each shell
export function ownersShell(version = app.getVersion()) {
  const who = userInfo(), shell = shellOf(who);
  return { shell, env: cleanEnv(process.env, { home: who.homedir, user: who.username, shell }, version) };
}

export function terminals(trusted: Trusted, terms = new Terminals(nodePty, (page, channel, ...args) => { if (page && !page.isDestroyed()) page.send(channel, ...args); }, ownersShell)) {
  const channels = ipc(trusted);
  // a task or a conversation removed: its shells end (TERMINAL-4), as obid's events say, or its
  // worktree gone; followed from the first shell on
  let following = false;
  const follow = () => {
    if (following) return;
    following = true;
    onLive(() => {
      const s = obidState();
      for (const { place, of } of terms.places()) {
        const removed = place === 'task' ? s.gone.has(of) || !worktreeOf(HOME, of) : s.goneConversations.has(of);
        if (removed) terms.closeWhere((t) => t.place === place && t.of === of);
      }
    });
  };
  // the pages whose shells end when they go (TERMINAL-4): reloaded, crashed or closed
  const watched = new WeakSet<WebContents>();
  const watch = (page: WebContents) => {
    if (watched.has(page)) return;
    watched.add(page);
    const gone = () => terms.closeWhere((t) => t.page === page);
    page.on('destroyed', gone);
    page.on('render-process-gone', gone);
    page.on('did-start-navigation', (d) => { if (d.isMainFrame && !d.isSameDocument) gone(); });
  };
  // a shell, on the owner's own click or Enter main saw (TERMINAL-1): where, and the size the page measured
  channels.handle('terminal-open', [placeOf, id, safeInteger, safeInteger], async (e, place, of, cols, rows): Promise<Opened> => {
    if (!takeInput()) return { error: 'Opening a terminal takes your own click.' };
    if (!size(cols, rows)) return { error: 'The terminal has no room to open in.' };
    const cwd = await cwdOf(place, of);
    if (!cwd) return { error: place === 'task' ? 'This task’s working copy isn’t on this Mac.' : 'This conversation has no folder on this Mac.' };
    watch(e.sender);
    follow();
    return terms.start(e.sender, place, of, cwd, cols, rows);
  }, { error: 'Not this app’s page.' });
  // what the owner typed (TERMINAL-5): only bytes a real input of theirs made reach the shell
  // (in order: a paste waits for the clipboard's text, and what's typed after it waits for the paste)
  const queued = new Map<string, Promise<void>>();
  channels.on('terminal-write', [string, bytes], (_e, term, data) => {
    const may = mayWrite(data, () => clipboard.readText());
    const before = queued.get(term);
    if (typeof may === 'boolean' && !before) { if (may) terms.write(term, data); return; }
    const next = (before ?? Promise.resolve()).then(async () => { if (await may) terms.write(term, data); });
    queued.set(term, next);
    void next.finally(() => { if (queued.get(term) === next) queued.delete(term); });
  });
  channels.on('terminal-resize', [string, safeInteger, safeInteger], (_e, term, cols, rows) => { terms.resize(term, cols, rows); });
  channels.on('terminal-close', [string], (_e, term) => { terms.close(term); });
  // Oby quits: every shell gets its signals, and any still there when the app ends is killed
  app.on('before-quit', () => terms.closeAll());
  app.on('will-quit', () => terms.killLeft());
  return terms;
}
