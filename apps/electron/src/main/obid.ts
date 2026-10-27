import { ipcMain, type IpcMainEvent, type IpcMainInvokeEvent } from 'electron';
import { closeSync, constants, fstatSync, openSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

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

// Why obid isn't answering: its own reason (home/last-start-error, written by a start that
// failed and removed once it listens, #1493), else what the connection said.
function reason(error: unknown): string {
  const own = readOwn('last-start-error', false);
  if (own) return own;
  const cause = (error as { cause?: { code?: string; message?: string } })?.cause;
  if (cause?.code === 'ECONNREFUSED') return 'Nothing answers on 127.0.0.1:7777: obid isn’t running.';
  return cause?.message ?? (error instanceof Error ? error.message : String(error));
}

// What a call or the stream came back with: obid's status and body, or (status 0) why it didn't answer
export type Answer = { status: number; text: string };

async function send(method: string, path: string, body: unknown, signal?: AbortSignal): Promise<Response | Answer> {
  // only obid's API: never another host, never another path
  if (!METHODS.has(method) || !/^\/v1\/[^\s#]*$/.test(path) || !new URL(BASE + path).pathname.startsWith('/v1/')) return { status: 400, text: JSON.stringify({ error: 'Not an obid call.' }) };
  const t = token();
  if (!t) return { status: 0, text: readOwn('last-start-error', false) ?? `No token of obid’s in ${HOME}/api-token: is obid set up on this Mac?` };
  try {
    return await fetch(BASE + path, {
      method, signal, cache: 'no-store',
      headers: {
        Authorization: `Bearer ${t}`,
        Accept: path === '/v1/events' ? 'text/event-stream' : 'application/json',
        ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (e) {
    return { status: 0, text: reason(e) };
  }
}

// Starting obid from the app: a stub. The Swift Mac app's login item (com.deskree.oby.obid)
// starts it today.
// TODO: bundle obid in the app and run `obid serve` here when nothing answers on 7777, with its
// failure reasons named (last-start-error).
export function startObid() {}

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
      end({ status: 0, text: 'obid closed the event stream.' });
    } catch (err) {
      if (!control.signal.aborted) end({ status: 0, text: reason(err) });
    }
  });
}
