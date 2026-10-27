import { app, dialog, ipcMain, shell, type IpcMainEvent, type IpcMainInvokeEvent } from 'electron';
import { execFile } from 'node:child_process';
import { closeSync, constants, fstatSync, openSync, readFileSync, readSync, statSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, join } from 'node:path';
import { promisify } from 'node:util';

// obid, Oby's engine (core/, docs/runner-api.md), as this app reaches it: the obid already
// answering on this Mac, adopted. Its token stays here in main: the renderer asks main for each
// call ('obid') and for the event stream ('obid-follow'), and main adds the bearer. Nothing but
// obid's /v1 API is reachable this way, and only from this app's own page (trusted).

// fixed: obid never moves to another port, and neither does the app
const BASE = 'http://127.0.0.1:7777';
// ponytail: macOS only, as the Swift app's runner home; follows obid's `home` when the app starts it
export const HOME = join(homedir(), 'Library/Application Support/Oby/Runner');
const METHODS = new Set(['GET', 'POST', 'PATCH', 'DELETE']);

// A file in obid's home, read only when it's a regular file of this user's, never through a link;
// secret: nobody else may read it either (the token is 0600). null when it isn't that.
function readOwn(name: string, secret: boolean): string | null {
  let fd: number;
  try { fd = openSync(join(HOME, name), constants.O_RDONLY | constants.O_NOFOLLOW); } catch { return null; }
  try {
    const st = fstatSync(fd);
    if (!st.isFile() || st.uid !== process.getuid?.() || (secret && st.mode & 0o077)) return null;
    return readFileSync(fd, 'utf8').trim() || null;
  } finally {
    closeSync(fd);
  }
}

// Read again on every call: obid writes a new one when it's reset.
const token = () => readOwn('api-token', true);

// Why obid isn't answering: why this app's own couldn't start (startObid), else obid's own reason
// (home/last-start-error, written by a start that failed and removed once it listens, #1493),
// else what the connection said.
function reason(error?: unknown): string {
  // whatever listens now gets checked again before the next call
  mineUntil = 0;
  if (problem) return problem;
  const own = readOwn('last-start-error', false);
  if (own) return own;
  const cause = (error as { cause?: { code?: string; message?: string } })?.cause;
  if (!error || cause?.code === 'ECONNREFUSED') return 'Nothing answers on 127.0.0.1:7777: obid isn’t running.';
  return cause?.message ?? (error instanceof Error ? error.message : String(error));
}

// Who listens on 127.0.0.1:7777: launchd's obid of this user's ('mine': this app's agent or its
// session job, the Swift Oby's or install.sh's), something else ('foreign': every user on the Mac
// shares the port, so another user's obid, or any other program, an agent's from its sandbox
// included, whatever it's named), or nothing. lsof lists only this user's processes. Only 'mine'
// gets the token, and this app stands down only for it. In development an obid run by hand counts
// too. ponytail: a 'mine' holds 10 s, or until a call fails; checking at every call would run lsof
// for each of the page's calls.
const TAKEN = 'Something other than your Oby engine answers on 127.0.0.1:7777 (another user’s Oby on this Mac, or another program), so Oby sends it nothing. Quit it, or log that user out, and open Oby again.';
let mineUntil = 0;
async function listener(): Promise<'mine' | 'foreign' | 'none'> {
  if (Date.now() < mineUntil) return 'mine';
  let pids: string[] = [];
  try {
    pids = (await run('/usr/sbin/lsof', ['-nP', '-a', '-iTCP@127.0.0.1:7777', '-sTCP:LISTEN', '-t'])).stdout.split('\n').filter(Boolean);
  } catch { /* exit 1: no process of this user's listens there */ }
  if (!pids.length) return await answers() ? 'foreign' : 'none';
  const launchds = await Promise.all([LABEL, SESSION, ...Object.keys(OTHERS)].map(async (label) => field(await launchd(label), 'pid')));
  for (const pid of pids) {
    const byHand = !app.isPackaged && await run('/bin/ps', ['-o', 'comm=', '-p', pid]).then((r) => basename(r.stdout.trim()) === 'obid', () => false);
    if (launchds.includes(pid) || byHand) { mineUntil = Date.now() + 10_000; return 'mine'; }
  }
  return 'foreign';
}

// What a call or the stream came back with: obid's status and body, or (status 0) why it didn't answer
export type Answer = { status: number; text: string };

async function send(method: string, path: string, body: unknown, signal?: AbortSignal): Promise<Response | Answer> {
  // only obid's API: never another host, never another path
  if (!METHODS.has(method) || !/^\/v1\/[^\s#]*$/.test(path) || !new URL(BASE + path).pathname.startsWith('/v1/')) return { status: 400, text: JSON.stringify({ error: 'Not an obid call.' }) };
  const t = token();
  if (!t) return { status: 0, text: problem ?? readOwn('last-start-error', false) ?? `No token of obid’s in ${HOME}/api-token: is obid set up on this Mac?` };
  const who = await listener();
  if (who !== 'mine') return { status: 0, text: who === 'foreign' ? TAKEN : reason() };
  try {
    const r = await fetch(BASE + path, {
      method, signal, cache: 'no-store',
      headers: {
        Authorization: `Bearer ${t}`,
        Accept: path === '/v1/events' ? 'text/event-stream' : 'application/json',
        ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    // it answers: whatever kept it away is over
    problem = null;
    return r;
  } catch (e) {
    return { status: 0, text: reason(e) };
  }
}

// This app's own obid (Contents/Helpers/obid, scripts/helpers.sh) is a launch agent from the app's
// plist (build/com.deskree.oby.electron.obid.plist), registered with SMAppService through
// Electron: launchd starts it now and at every login, and keeps it running after Oby quits, with
// its jobs and the iPhone's relay. An obid of this user's already answering on 7777 is adopted as
// it is. While launchd has the Swift Oby's login item or install.sh's agent (OTHERS), this app
// registers none and stands its own down, so two agents never fight for the port at login. The
// Swift app's lessons (apps/mac/app/LoginItem.swift):
// - only from Applications: a translocated copy, or one on the disk image, would register a path
//   that goes away;
// - macOS may want the user's approval first (System Settings › General › Login Items);
// - a registration macOS kept from an older signature can leave launchd refusing obid (exit 78,
//   "Launch Constraint Violation"): registered again, once, when obid neither answers nor runs;
// - an obid running from before the app was replaced keeps its old code: restarted once idle.
// And one of its own: macOS 26 doesn't start a just-registered agent until the next login (launchd:
// `copy_bundle_path … The specified path is not a bundle`, exit 78; backgroundtaskmanagementd:
// `record not found`, `fullPath is nil`; seen 2026-10-27 in clean macOS 26.6 VMs, the Swift Oby's
// too), so until then the same obid runs as a job of this login session only (SESSION).
const PLIST = 'com.deskree.oby.electron.obid.plist';
const LABEL = 'com.deskree.oby.electron.obid';
const SESSION = `${LABEL}.session`;
// the other agents that start an obid at login, and whose they are
const OTHERS = { 'com.deskree.oby.obid': 'the Swift Oby', 'com.deskree.obid': 'install.sh' };
const SERVICE = { type: 'agentService', serviceName: PLIST } as const;
const LOGIN_ITEMS = 'x-apple.systempreferences:com.apple.LoginItems-Settings.extension';
const run = promisify(execFile);
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));
const status = () => app.getLoginItemSettings(SERVICE).status;
const register = (on: boolean) => app.setLoginItemSettings({ ...SERVICE, openAtLogin: on });

// Why this app's obid isn't there, as startObid found it: said before anything else
let problem: string | null = null;

const domain = () => `gui/${process.getuid?.()}`;
const helper = () => join(process.resourcesPath, '../Helpers/obid');

// launchctl's `print` of this app's obid (or its session job), or null when launchd hasn't got it
async function launchd(label = LABEL): Promise<string | null> {
  try { return (await run('/bin/launchctl', ['print', `${domain()}/${label}`])).stdout; } catch { return null; }
}
// one `key = value` line of it
const field = (printed: string | null, key: string) =>
  printed?.split('\n').map((l) => l.trim()).find((l) => l.startsWith(`${key} = `))?.slice(key.length + 3);

// The same obid as a job of this login session only: its plist is in the app's own folder, never in
// ~/Library/LaunchAgents, so from the next login only the registered agent starts obid
async function forThisSession() {
  const plist = join(app.getPath('userData'), `${SESSION}.plist`);
  writeFileSync(plist, `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>${SESSION}</string>
<key>ProgramArguments</key><array><string>${helper().replace(/&/g, '&amp;').replace(/</g, '&lt;')}</string><string>serve</string></array>
<key>AssociatedBundleIdentifiers</key><array><string>com.deskree.oby.electron</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
</dict></plist>
`, { mode: 0o600 });
  await run('/bin/launchctl', ['bootout', `${domain()}/${SESSION}`]).catch(() => {});
  await run('/bin/launchctl', ['bootstrap', domain(), plist]);
}

// Whether an obid listens on 7777 (any answer, a 401 too), trying for up to `ms`
async function answers(ms = 0): Promise<boolean> {
  const end = Date.now() + ms;
  for (;;) {
    try { await fetch(BASE + '/v1/status', { signal: AbortSignal.timeout(1000) }); return true; } catch { /* not yet */ }
    if (Date.now() >= end) return false;
    await sleep(500);
  }
}

// The last lines of ~/Library/Logs/obid.log (core/crates/obid/src/app.rs, log_path)
function logTail(): string | null {
  let fd: number;
  try { fd = openSync(join(homedir(), 'Library/Logs/obid.log'), constants.O_RDONLY | constants.O_NOFOLLOW); } catch { return null; }
  try {
    const size = fstatSync(fd).size, buffer = Buffer.alloc(Math.min(size, 2048));
    readSync(fd, buffer, 0, buffer.length, size - buffer.length);
    return buffer.toString('utf8').trim().split('\n').slice(-3).join('\n') || null;
  } finally {
    closeSync(fd);
  }
}

// Why the obid launchd was given didn't start: obid's own reason, else launchd's and the log's
async function why(): Promise<string> {
  const own = readOwn('last-start-error', false);
  if (own) return own;
  const printed = await launchd();
  const exit = field(printed, 'last exit code');
  const tail = logTail();
  return `Oby’s engine didn’t start: ${printed ? `launchd says ${exit ? `its last exit code was ${exit}` : `it’s ${field(printed, 'state') ?? 'not running'}`}` : `launchd doesn’t have it (macOS says its login item is ${status()})`}.`
    + (tail ? `\n~/Library/Logs/obid.log ends:\n${tail}` : '');
}

// Oby opened from the disk image, Downloads or anywhere else: it asks to move into Applications,
// and opens again from there
async function moveToApplications() {
  const { response } = await dialog.showMessageBox({
    message: 'Move Oby to Applications?',
    detail: 'Oby’s engine starts only when Oby is in Applications.',
    buttons: ['Move to Applications', 'Not Now'], defaultId: 0, cancelId: 1,
  });
  try { if (response === 0 && app.moveToApplicationsFolder()) return; } catch { /* stays where it is */ }
  problem = 'Oby is running from outside Applications, so its engine can’t start. Quit Oby, drag it into Applications, and open it from there.';
}

// An obid launchd started before its executable was replaced (a newer Oby put over this one) runs
// its old code: restarted once it runs no job. ponytail: waits for its jobs at most 10 minutes,
// then restarts anyway (they carry on after a restart, docs/runner-api.md)
async function restartIfStale(label: string, printed: string) {
  try {
    // in C's words: Date.parse reads no other language's month names
    const lstart = await run('/bin/ps', ['-o', 'lstart=', '-p', field(printed, 'pid') ?? ''], { env: { ...process.env, LC_ALL: 'C' } });
    if (!(Date.parse(lstart.stdout.trim()) < statSync(helper()).ctimeMs - 1000)) return;
  } catch { return; }
  for (let i = 0; i < 20; i++) {
    const r = await send('GET', '/v1/status', undefined);
    const running = r instanceof Response && r.ok ? await r.json().then((s: { running?: number }) => s.running, () => 0) : 0;
    if (!running) break;
    await sleep(30_000);
  }
  await run('/bin/launchctl', ['kickstart', '-k', `${domain()}/${label}`]).catch(() => {});
}

// At launch: adopts the obid that answers, or starts this app's own and says why when it can't.
// Only a packaged app has the launch agent: in development it's the obid you run, or the Swift Oby's.
export async function startObid() {
  if (process.platform !== 'darwin' || !app.isPackaged) return;
  problem = 'Oby’s engine is starting…';
  try {
    const who = await listener();
    // never in the way of another user's obid or another program: nothing registered or stopped
    if (who === 'foreign') { problem = TAKEN; return; }
    for (const [label, whose] of Object.entries(OTHERS).filter(([l]) => l !== LABEL)) {
      const printed = await launchd(label);
      if (!printed) continue;
      if (status() === 'enabled' || status() === 'requires-approval') register(false);
      await run('/bin/launchctl', ['bootout', `${domain()}/${SESSION}`]).catch(() => {});
      const exit = field(printed, 'last exit code');
      problem = field(printed, 'state') === 'running' ? null : `Oby’s engine on this Mac is ${whose}’s, and it isn’t running (launchd says ${exit ? `its last exit code was ${exit}` : `it’s ${field(printed, 'state') ?? 'not running'}`}). Start it there, or remove it, and open this app again.`;
      return;
    }
    if (who === 'mine') {
      problem = null;
      for (const label of [LABEL, SESSION]) {
        const printed = await launchd(label);
        if (field(printed, 'state') === 'running') return await restartIfStale(label, printed!);
      }
      // an obid nobody registered (one run by hand): adopted, this app's agent kept for later logins
      return;
    }
    if (!app.isInApplicationsFolder()) return await moveToApplications();
    // registered but not running: one macOS kept from an older signature, registered again once
    if (status() === 'enabled') register(false);
    register(true);
    if (status() === 'requires-approval') {
      problem = 'Oby’s engine waits for your approval: turn Oby on in System Settings › General › Login Items.';
      void shell.openExternal(LOGIN_ITEMS);
      return;
    }
    // a first start can take a while (its configuration, the security probes) while it runs
    const started = async () => await answers(10_000) || (field(await launchd(), 'state') === 'running' && await answers(60_000));
    if (await started()) { problem = null; return; }
    // macOS 26's just-registered agent: this session's job until the next login
    await forThisSession();
    if (await answers(60_000)) { problem = null; return; }
    problem = await why();
  } catch (e) {
    problem = `Oby’s engine couldn’t be started: ${e instanceof Error ? e.message : String(e)}`;
  }
}

let following: AbortController | null = null;
// the window closed: nobody to send events to
export const stopFollowing = () => { following?.abort(); following = null; };

// One call, read whole
export async function obid(method: string, path: string, body?: unknown): Promise<Answer> {
  const r = await send(method, path, body);
  return r instanceof Response ? { status: r.status, text: await r.text() } : r;
}

// The page's own frame only: no other page, or frame in it, gets obid
export type Trusted = (e: IpcMainEvent | IpcMainInvokeEvent) => boolean;

export function bridge(trusted: Trusted) {
  // one call: obid's status and body, or why it didn't answer
  ipcMain.handle('obid', async (e, method: string, path: string, body?: unknown): Promise<Answer> => {
    if (!trusted(e)) return { status: 403, text: '' };
    return obid(method, path, body);
  });

  // The event stream (GET /v1/events), one at a time: each piece of its text goes to the page as
  // it comes ('obid-chunk'), then how it ended ('obid-end'). Following again drops the one before.
  ipcMain.on('obid-follow', async (e) => {
    if (!trusted(e)) return;
    following?.abort();
    const control = new AbortController();
    following = control;
    const page = e.sender;
    const end = (a: Answer) => { if (following === control && !page.isDestroyed()) page.send('obid-end', a); };
    const r = await send('GET', '/v1/events', undefined, control.signal);
    if (!(r instanceof Response)) return end(r);
    if (!r.ok || !r.body) return end({ status: r.status, text: await r.text().catch(() => '') });
    // obid answered: the page fetches what it shows now (an empty piece), with the events that
    // come meanwhile applied over it
    if (!page.isDestroyed()) page.send('obid-chunk', '');
    try {
      const reader = r.body.pipeThrough(new TextDecoderStream()).getReader();
      for (;;) {
        const { value, done } = await reader.read();
        if (done || following !== control) break;
        if (!page.isDestroyed()) page.send('obid-chunk', value);
      }
      mineUntil = 0;
      end({ status: 0, text: 'obid closed the event stream.' });
    } catch (err) {
      if (!control.signal.aborted) end({ status: 0, text: reason(err) });
    }
  });
}
