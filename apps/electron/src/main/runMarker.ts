import { app } from 'electron';
import { closeSync, constants, openSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { now, writeOwn } from './device';

// The app's own run records (docs/contracts/engine.md, ENGINE-17 to 19), three private files in its
// own folder: how its last run ended (run-marker), its starts per version that never loaded the
// window (start-attempts.json), and its crash lines (crashes.jsonl). Nothing here ever stops a
// start, a reload or a quit: a file that can't be read reads as empty, one that can't be written
// is left as it was.

export type Marker = 'running' | 'quitting' | 'clean' | 'quit_timeout';
export type Crash = { kind: string; reason: string; exitCode?: number; previous?: string; time: string };

// the crash lines kept, the newest; and how long a quit may take
export const KEPT_CRASHES = 100;
export const QUIT_DEADLINE = 30_000;

const MARKERS: readonly string[] = ['running', 'quitting', 'clean', 'quit_timeout'];
const file = (name: string) => join(app.getPath('userData'), name);

// a file of the app's own, never through a link: '' when it isn't there or can't be read
function read(name: string): string {
  try {
    const fd = openSync(file(name), constants.O_RDONLY | constants.O_NOFOLLOW);
    try { return readFileSync(fd, 'utf8'); } finally { closeSync(fd); }
  } catch { return ''; }
}
function keep(name: string, data: string) {
  try { writeOwn(file(name), data, constants.O_TRUNC); } catch (e) { console.error(`Oby: couldn’t keep ${name}: ${e}`); }
}
const parse = (text: string): unknown => { try { return JSON.parse(text); } catch { return undefined; } };
const plain = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);

function marker(): Marker | null {
  const word = read('run-marker').trim();
  return MARKERS.includes(word) ? (word as Marker) : null;
}
const mark = (word: Marker) => keep('run-marker', word);

// every version's count, only the ones that are counts
function attempts(): Record<string, number> {
  const kept = parse(read('start-attempts.json'));
  if (!plain(kept)) return {};
  return Object.fromEntries(Object.entries(kept).filter((e): e is [string, number] => Number.isSafeInteger(e[1]) && (e[1] as number) >= 0));
}
const count = (n: number) => keep('start-attempts.json', JSON.stringify({ ...attempts(), [app.getVersion()]: n }));

// the lines, oldest first, a malformed one skipped
function crashes(): Crash[] {
  return read('crashes.jsonl').split('\n').map(parse).filter((c): c is Crash =>
    plain(c) && typeof c.kind === 'string' && typeof c.reason === 'string' && typeof c.time === 'string');
}
function crashed(line: Crash) {
  keep('crashes.jsonl', [...crashes(), line].slice(-KEPT_CRASHES).map((c) => JSON.stringify(c) + '\n').join(''));
}

// What's kept, for the diagnostics export and the recovery page: how the last run ended, this
// version's starts that never loaded the window, and the crash lines
export function runRecords(): { marker: Marker | null; attempts: number; crashes: Crash[] } {
  return { marker: marker(), attempts: attempts()[app.getVersion()] ?? 0, crashes: crashes() };
}

// The app starts (in whenReady, before obid): a last run that never ended cleanly is a crash line,
// and this start counts until its window loads. A `--background` start opens no window: it doesn't count.
export function started(background: boolean) {
  const previous = marker();
  if (previous === 'running' || previous === 'quitting') crashed({ kind: 'app', reason: 'unclean_exit', previous, time: now() });
  mark('running');
  if (!background) count((attempts()[app.getVersion()] ?? 0) + 1);
}

// The window's page loaded: this version starts fine, and a renderer gone from now on may be
// reloaded again
let reloading = false;
export function windowLoaded() {
  reloading = false;
  count(0);
}

// A process gone: its kind, Electron's reason and exit code, and the time, nothing else
type Gone = { reason: string; exitCode: number };
const line = (kind: string, { reason, exitCode }: Gone): Crash => ({ kind, reason, exitCode, time: now() });
export function childGone(details: Gone & { type: string }) {
  crashed(line(details.type, details));
}
// The window's renderer gone (page: its webContents, null for any other): reloaded once, unless it
// ended cleanly or it's gone again before that reload loaded
export function rendererGone(details: Gone, page: { reload(): void; isDestroyed(): boolean } | null) {
  crashed(line('renderer', details));
  if (details.reason === 'clean-exit' || !page || page.isDestroyed() || reloading) return;
  reloading = true;
  try { page.reload(); } catch (e) { console.error(`Oby: couldn’t reload the window: ${e}`); }
}

// The quit: asked for (before-quit), it has 30 s to end by itself, then the app ends here.
// The timer never keeps the app alive by itself.
let deadline: ReturnType<typeof setTimeout> | null = null;
export function quitting() {
  if (deadline) return;
  mark('quitting');
  deadline = setTimeout(() => {
    mark('quit_timeout');
    crashed({ kind: 'app', reason: 'quit_timeout', time: now() });
    app.exit(1);
  }, QUIT_DEADLINE);
  deadline.unref?.();
}
// it ended (quit): cleanly
export function quit() {
  if (deadline) clearTimeout(deadline);
  mark('clean');
}
