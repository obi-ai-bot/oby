import { app, clipboard, dialog, shell } from 'electron';
import { execFile } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { accessSync, closeSync, constants, fstatSync, mkdirSync, mkdtempSync, openSync, readFileSync, readSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { homedir, release, tmpdir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { oneOf } from '../shared/decode';
import { COMMIT } from '../shared/build';
import { obid } from './obid';
import { ipc, type Trusted } from './ipc';

// The Help menu (docs/contracts/help.md, HELP-1 to 6): obid's log shown in Finder, and the facts a
// bug needs copied for the owner to paste into it the same day. What's copied goes through the
// secret scan first, as everything that leaves does (AGENTS.md, "Non-negotiable"): obid's log is
// meant to hold no secret (LOGS-1), and the scan is what makes sure. A finding, or a scan that
// can't run, copies nothing. T3 Code keeps the same facts on a Settings page
// (apps/web/src/components/settings/DiagnosticsSettings.tsx); Oby has the two menu items only.

// core/crates/obid/src/app.rs, log_path
export const OBID_LOG = join(homedir(), 'Library/Logs/obid.log');
export const LOG_LINES = 200;
// at most this much of the log's end is read for its last lines
const TAIL_BYTES = 256 * 1024;

// The last `lines` lines of a file, read from its end (never a link followed); null when there's no file
export function tail(path: string, lines = LOG_LINES): string[] | null {
  let fd: number;
  try { fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW); } catch { return null; }
  try {
    const size = fstatSync(fd).size, buffer = Buffer.alloc(Math.min(size, TAIL_BYTES));
    readSync(fd, buffer, 0, buffer.length, size - buffer.length);
    const all = buffer.toString('utf8').split('\n');
    // read from the middle of the file: its first line is only the end of one
    if (buffer.length < size) all.shift();
    while (all.length && all.at(-1) === '') all.pop();
    return all.slice(-lines);
  } finally {
    closeSync(fd);
  }
}

export type Facts = {
  app: { version: string; commit: string };
  // obid's own version and commit (GET /v1/status's runner), or why it didn't say
  obid: { version: string; commit: string | null } | { why: string };
  macos: string;
  arch: string;
  log: string[] | null;
};

// What Copy Diagnostics puts on the clipboard (HELP-2)
export function diagnosticsText(f: Facts): string {
  const obidLine = 'why' in f.obid ? `obid: ${f.obid.why}` : `obid: ${f.obid.version} (commit ${f.obid.commit || 'unknown'})`;
  return [
    `Oby: ${f.app.version} (commit ${f.app.commit || 'unknown'})`,
    obidLine,
    `macOS: ${f.macos}`,
    `Architecture: ${f.arch}`,
    '',
    f.log ? `obid's log, its last ${f.log.length} lines (~/Library/Logs/obid.log):` : 'obid\'s log: none at ~/Library/Logs/obid.log',
    ...(f.log ?? []),
  ].join('\n');
}

// obid's version and commit, as GET /v1/status's runner says them, waiting at most 3 seconds
async function obidFacts(): Promise<Facts['obid']> {
  const late = new Promise<Facts['obid']>((r) => setTimeout(() => r({ why: 'didn’t answer within 3 seconds' }), 3000).unref?.());
  const asked = obid('GET', '/v1/status', undefined, undefined, true).then((r): Facts['obid'] => {
    if (r.status !== 200) return { why: r.status ? `answered ${r.status}` : 'isn’t running' };
    try {
      const runner = (JSON.parse(r.text) as { runner?: { version?: unknown; commit?: unknown } }).runner;
      if (typeof runner?.version === 'string') return { version: runner.version, commit: typeof runner.commit === 'string' ? runner.commit : null };
    } catch { /* below */ }
    return { why: 'answered without its version' };
  }, (): Facts['obid'] => ({ why: 'isn’t running' }));
  return Promise.race([asked, late]);
}

// The macOS version ("26.1"), from the system's own version file, else Darwin's
function macos(): string {
  try {
    const plist = readFileSync('/System/Library/CoreServices/SystemVersion.plist', 'utf8');
    const v = /<key>ProductVersion<\/key>\s*<string>([^<]+)<\/string>/.exec(plist)?.[1];
    if (v) return v;
  } catch { /* below */ }
  return `Darwin ${release()}`;
}

export const facts = async (log = OBID_LOG): Promise<Facts> => ({
  app: { version: app.getVersion(), commit: COMMIT },
  obid: await obidFacts(),
  macos: macos(),
  arch: process.arch,
  log: tail(log),
});

// The secret scan's verdict on a text (HELP-3, HELP-4)
export type Scan = { kind: 'clean' } | { kind: 'found'; rules: string[] } | { kind: 'couldNotRun'; why: string };
// gitleaks run on one file: its exit code (0 nothing found, 99 something), or a throw when it can't start
export type Gitleaks = (input: string, report: string, home: string) => Promise<number>;

// gitleaks, as obid runs it (core/crates/obid/src/scan.rs, run_gitleaks): the one beside the app
// (Contents/MacOS/gitleaks, where its packaging puts it), else one on the PATH
export function gitleaksBin(): string | null {
  const beside = join(dirname(app.getPath('exe')), 'gitleaks');
  const dirs = [...(process.env.PATH ?? '').split(delimiter), '/opt/homebrew/bin', '/usr/local/bin'].filter(Boolean);
  for (const bin of [beside, ...dirs.map((d) => join(d, 'gitleaks'))]) {
    try { if (statSync(bin).isFile()) { accessSync(bin, constants.X_OK); return bin; } } catch { /* the next */ }
  }
  return null;
}

export const runGitleaks = (bin: string): Gitleaks => (input, report, home) => new Promise((resolve, reject) => {
  execFile(bin, ['dir', input, '--no-banner', '--redact', '--ignore-gitleaks-allow', '--exit-code=99', '--report-format=json', `--report-path=${report}`],
    { env: { PATH: '/usr/bin:/bin', HOME: home, TMPDIR: home }, cwd: dirname(input), timeout: 30_000 },
    (error) => {
      if (!error) return resolve(0);
      const code = (error as NodeJS.ErrnoException & { code?: unknown }).code;
      if (typeof code === 'number' && !(error as { killed?: boolean }).killed) return resolve(code);
      reject(error);
    });
});

// A made-up GitHub token on the text's last line: gitleaks must find it, or it didn't read the text
// (as obid's canary, scan.rs: a report without it is a scan that couldn't run)
function canary() {
  const letters = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  return `ghp_${[...randomBytes(36)].map((b) => letters[b % letters.length]).join('')}`;
}

const appGitleaks = (): Gitleaks | null => { const bin = gitleaksBin(); return bin ? runGitleaks(bin) : null; };

export async function scanText(text: string, gitleaks: Gitleaks | null = appGitleaks()): Promise<Scan> {
  if (!gitleaks) return { kind: 'couldNotRun', why: 'gitleaks isn’t installed' };
  const folder = mkdtempSync(join(tmpdir(), 'oby-diagnostics-'));
  try {
    const lines = text.split('\n');
    // a neutral line between the text and the canary, so a last line that only names a secret isn't read on into it
    const canaryLine = lines.length + 2;
    const input = join(folder, 'diagnostics.txt'), report = join(folder, 'report.json'), home = join(folder, 'home');
    mkdirSync(home, { mode: 0o700 });
    writeFileSync(input, [...lines, '-', canary()].join('\n'), { mode: 0o600 });
    let code: number;
    try { code = await gitleaks(input, report, home); } catch (e) { return { kind: 'couldNotRun', why: `gitleaks didn’t run: ${e instanceof Error ? e.message : String(e)}` }; }
    if (code !== 0 && code !== 99) return { kind: 'couldNotRun', why: `gitleaks stopped with code ${code}` };
    let found: { StartLine?: unknown; RuleID?: unknown }[];
    try { found = JSON.parse(readFileSync(report, 'utf8')); } catch { return { kind: 'couldNotRun', why: 'gitleaks wrote no report' }; }
    if (!Array.isArray(found) || !found.some((f) => f.StartLine === canaryLine)) return { kind: 'couldNotRun', why: 'gitleaks didn’t read the text' };
    const rules = [...new Set(found.filter((f) => f.StartLine !== canaryLine).map((f) => String(f.RuleID ?? 'secret')))];
    return rules.length ? { kind: 'found', rules } : { kind: 'clean' };
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}

// The same scan on bytes that leave (the Obysphere's picture, SPHEREAPP-16): every printable ASCII
// run of them as it is, everything else a space, so what a secret is written in reads as text and
// gitleaks still finds its canary. A finding, or a scan that can't run, sends nothing.
// eslint-disable-next-line no-control-regex
export const printable = (bytes: Uint8Array) => Buffer.from(bytes).toString('latin1').replace(/[^\x20-\x7e\n]/g, ' ');
export const scanBytes = (bytes: Uint8Array, gitleaks?: Gitleaks | null): Promise<Scan> =>
  scanText(printable(bytes), gitleaks === undefined ? appGitleaks() : gitleaks);

// Copy Diagnostics: copied when the scan is clean, else nothing and why (HELP-2 to 4). null: copied
export async function copyDiagnostics(the: { facts?: () => Promise<Facts>; scan?: (text: string) => Promise<Scan>; write?: (text: string) => void } = {}): Promise<string | null> {
  const text = diagnosticsText(await (the.facts ?? facts)());
  const scan = await (the.scan ?? scanText)(text);
  if (scan.kind === 'found') return `The secret scan found what looks like a secret (${scan.rules.join(', ')}), so nothing was copied.`;
  if (scan.kind === 'couldNotRun') return `The secret scan couldn’t run (${scan.why}), so nothing was copied.`;
  (the.write ?? ((t: string) => clipboard.writeText(t)))(text);
  return null;
}

// Reveal obid's Log: the file shown in Finder (HELP-1). null: shown
export function revealLog(log = OBID_LOG, show = (path: string) => shell.showItemInFolder(path)): string | null {
  try { if (!statSync(log).isFile()) throw new Error('not a file'); } catch { return 'obid hasn’t written a log yet (~/Library/Logs/obid.log).'; }
  show(log);
  return null;
}

export type HelpAsk = 'reveal-log' | 'copy-diagnostics';
const run = (what: HelpAsk) => (what === 'reveal-log' ? Promise.resolve(revealLog()) : copyDiagnostics());

// From the menu bar: what went wrong, in a dialog; nothing when it went right
const told = (what: HelpAsk) => () => void run(what).then((why) => { if (why) void dialog.showMessageBox({ type: 'warning', message: why }); });
export const helpActions = { revealLog: told('reveal-log'), copyDiagnostics: told('copy-diagnostics') };

// From the page (⌘K's Actions, SHORTCUTS-11): the same, what went wrong answered for it to show
export function helping(trusted: Trusted) {
  ipc(trusted).handle('help', [oneOf('reveal-log', 'copy-diagnostics')], (_e, what) => run(what), 'Not this app’s page.');
}
