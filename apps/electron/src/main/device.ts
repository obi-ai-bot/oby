import { app, BrowserWindow, dialog, ipcMain, safeStorage } from 'electron';
import { createPrivateKey, createPublicKey, generateKeyPairSync, sign, type KeyObject } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { closeSync, constants, openSync, readFileSync, renameSync, rmSync, writeSync } from 'node:fs';
import { hostname } from 'node:os';
import { join } from 'node:path';
import { HOME, obid, type Answer, type Trusted } from './obid';
import { takeInput, wordsHeader } from './words';

// This Mac's device key for obid (docs/runner-api.md, "POST /v1/devices"), which makes a Confirm
// the owner's (#776). A P-256 key made here in main, kept encrypted by safeStorage (macOS: a key
// in the login Keychain that only this app reads without asking) in the app's own folder; it
// never leaves main. Nothing here signs what the page asks to: a Confirm is one open item by its
// id, on the owner's click, and main reads that item's title from obid itself and signs obid's own
// sentence for it; words (ownersWords) are signed only when the preload saw the owner's own
// keystroke or click hold them, and only on the routes that carry words.

const O = constants;

// A file written fresh, 0600, never through a link
export function writeOwn(path: string, data: string | Buffer, flags: number) {
  const fd = openSync(path, O.O_WRONLY | O.O_CREAT | O.O_NOFOLLOW | flags, 0o600);
  try { writeSync(fd, typeof data === 'string' ? Buffer.from(data) : data); } finally { closeSync(fd); }
}

// A key made here in main, kept encrypted by safeStorage in the app's own folder as `file`: this
// Mac's device key, and the owner key (main/sphere.ts)
export function keptKey(file: string, make: () => KeyObject): KeyObject {
  const path = join(app.getPath('userData'), file);
  if (!safeStorage.isEncryptionAvailable()) throw new Error('This Mac’s Keychain isn’t available to Oby, so it can’t keep a key to confirm with.');
  let sealed: Buffer | null = null;
  try {
    const fd = openSync(path, O.O_RDONLY | O.O_NOFOLLOW);
    try { sealed = readFileSync(fd); } finally { closeSync(fd); }
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code !== 'ENOENT') throw e;
  }
  if (sealed) {
    try { return createPrivateKey(safeStorage.decryptString(sealed)); } catch {
      // sealed by a key this app can't open anymore (another signature, a Keychain reset): set
      // aside, and a new one, which pairs on the owner's click as the first did
      renameSync(path, `${path}.unreadable-${Date.now()}`);
    }
  }
  const made = make();
  // O_EXCL: never over a key that's there
  writeOwn(path, safeStorage.encryptString(made.export({ type: 'pkcs8', format: 'pem' }) as string), O.O_EXCL);
  return made;
}

let key: KeyObject | null = null;
export const deviceKey = (): KeyObject => (key ??= keptKey('device-key', () => generateKeyPairSync('ec', { namedCurve: 'prime256v1' }).privateKey));

// X9.63 (65 bytes, uncompressed), base64, as obid takes it
export function publicKey() {
  const { x, y } = createPublicKey(deviceKey()).export({ format: 'jwk' });
  return Buffer.concat([Buffer.from([4]), Buffer.from(x!, 'base64url'), Buffer.from(y!, 'base64url')]).toString('base64');
}

// ECDSA P-256 with SHA-256, DER, base64: obid's sentences only (callers below)
export const signed = (sentence: string) => sign('sha256', Buffer.from(sentence, 'utf8'), deviceKey()).toString('base64');
export const now = () => new Date().toISOString().replace(/\.\d+Z$/, 'Z');

function macName() {
  try { return execFileSync('/usr/sbin/scutil', ['--get', 'ComputerName'], { encoding: 'utf8' }).trim() || hostname(); } catch { return hostname(); }
}
const name = () => `${macName()} (Oby desktop)`;

const said = (a: Answer) => { try { return (JSON.parse(a.text) as { error?: string }).error ?? a.text; } catch { return a.text; } };

// Registered with obid, every time (a key it knows answers its id at once). A new key goes in only
// with the owner present: on their click, as the Swift Mac app does, home/pairing names it for the
// moment the registration takes (obid takes it within 2 minutes), and is removed whatever happened.
// One at a time: two at once would both write and remove home/pairing.
let registering: Promise<string> | null = null;
export function register(): Promise<string> {
  return (registering ??= registerNow().finally(() => { registering = null; }));
}
async function registerNow(): Promise<string> {
  const body = { name: name(), publicKey: publicKey() };
  let r = await obid('POST', '/v1/devices', body);
  if (r.status === 403) {
    const pairing = join(HOME, 'pairing');
    try {
      writeOwn(pairing, body.publicKey, O.O_TRUNC);
      r = await obid('POST', '/v1/devices', body);
    } finally {
      rmSync(pairing, { force: true });
    }
  }
  if (r.status < 200 || r.status >= 300) throw new Error(r.status ? said(r) : r.text);
  return (JSON.parse(r.text) as { id: string }).id;
}

type Item = { id: string; kind: string; title: string; state: string };

// The owner's words, signed with this Mac's key: the preload saw the owner's own keystroke or
// click hold them, main saw that keystroke or click itself (main/words.ts, takeInput), and the
// request is one of the app's words requests in its exact shape (wordsHeader). Anything short of
// that, or a key or registration that fails, goes unsigned, as words always could: obid marks
// them. A limit-stopped job's continuation needs the signature (docs/runner-api.md, `accountLimit`).
// The device registers once a run (a Confirm still registers each time).
let device: string | null = null;
export async function ownersWords(method: string, path: string, body: unknown): Promise<Answer> {
  let words: string | null = null;
  try {
    if (takeInput()) words = wordsHeader(deviceKey(), (device ??= await register()), method, path, body);
  } catch (err) {
    console.error(`Oby couldn’t sign the owner’s words, so they go unsigned: ${err instanceof Error ? err.message : String(err)}`);
  }
  return obid(method, path, body, words ?? undefined);
}

// Fetch and verify the current options before signing; the revision and destination are covered.
export async function answerLimit(id: string, revision: number, choice: string, account?: string): Promise<Answer> {
  try {
    const current = await obid('GET', '/v1/limit-decisions');
    if (current.status !== 200) return current;
    const decisions = JSON.parse(current.text) as { id: string; revision: number; options: { kind: string; account?: string }[] }[];
    const decision = decisions.find((d) => d.id === id && d.revision === revision);
    if (!decision || !decision.options.some((o) => o.kind === choice && (o.account ?? '') === (account ?? ''))) {
      return { status: 409, text: JSON.stringify({ error: 'This choice changed. Review the current options and try again.' }) };
    }
    const device = await register(), signedAt = now();
    const signature = signed(`oby-limit-decision\n${id}\n${revision}\n${choice}\n${account ?? ''}\n${signedAt}`);
    return obid('POST', `/v1/limit-decisions/${encodeURIComponent(id)}/answer`, {
      revision, choice, ...(account ? { account } : {}), approval: { device, action: choice, signedAt, signature },
    });
  } catch (err) {
    return { status: 500, text: JSON.stringify({ error: err instanceof Error ? err.message : String(err) }) };
  }
}

// Confirm, on the owner's click in the window: the open `confirm` item, by its id, with this
// device's signature over obid's sentence for it (POST /v1/inbox/{id}/answer, "Confirm takes the
// owner's tap")
export function confirming(trusted: Trusted) {
  ipcMain.handle('obid-confirm', async (e, id: unknown): Promise<Answer> => {
    if (!trusted(e) || typeof id !== 'string') return { status: 403, text: '' };
    try {
      const inbox = await obid('GET', '/v1/inbox');
      if (inbox.status !== 200) return inbox;
      const item = (JSON.parse(inbox.text) as Item[]).find((i) => i.id === id && i.kind === 'confirm' && i.state !== 'done');
      if (!item) return { status: 409, text: JSON.stringify({ error: 'That isn’t waiting for a Confirm anymore.' }) };
      const device = await register(), signedAt = now();
      const signature = signed(`oby-confirm\n${item.id}\n${item.title}\n${signedAt}`);
      return obid('POST', `/v1/inbox/${encodeURIComponent(item.id)}/answer`, { answer: 'Confirm', approval: { device, action: item.title, signedAt, signature } });
    } catch (err) {
      return { status: 500, text: JSON.stringify({ error: err instanceof Error ? err.message : String(err) }) };
    }
  });

  // Pair a device, from Settings › Runner: an invitation (POST /v1/pairing/invite) with this Mac's
  // signature over obid's sentence for it, so the phone it pairs may add its own key and approve
  // (docs/runner-api.md, "obid only: the gateway"). Only once the owner clicked Pair in main's own
  // dialog, which the page can't click: a phone that can approve is for good (PAIRING-2). The name
  // is this Mac's.
  ipcMain.handle('obid-pair-invite', async (e): Promise<Answer> => {
    if (!trusted(e)) return { status: 403, text: '' };
    const w = BrowserWindow.fromWebContents(e.sender);
    const asked = { type: 'question' as const, message: 'Pair a new device with this Mac?', detail: 'The phone that scans its code can see and answer your tasks, and approve for you, until you revoke it.', buttons: ['Pair', 'Cancel'], defaultId: 1, cancelId: 1 };
    const { response } = await (w ? dialog.showMessageBox(w, asked) : dialog.showMessageBox(asked));
    if (response !== 0) return { status: 409, text: JSON.stringify({ error: 'Not paired: you cancelled.' }) };
    try {
      const device = await register(), signedAt = now();
      return obid('POST', '/v1/pairing/invite', { name: macName(), approval: { device, action: '', signedAt, signature: signed(`oby-pair-invite\n${signedAt}`) } });
    } catch (err) {
      return { status: 500, text: JSON.stringify({ error: err instanceof Error ? err.message : String(err) }) };
    }
  });

  // A usage-limit answer is bound to the exact decision revision, option and destination.
  // Read the options from obid in main; never sign an arbitrary renderer-provided sentence.
  ipcMain.handle('obid-limit-answer', async (e, id: unknown, revision: unknown, choice: unknown, account: unknown): Promise<Answer> => {
    if (!trusted(e) || typeof id !== 'string' || !Number.isSafeInteger(revision) || typeof choice !== 'string' ||
      !['move', 'resume', 'stop'].includes(choice) || (account !== undefined && typeof account !== 'string')) {
      return { status: 403, text: '' };
    }
    return answerLimit(id, revision as number, choice, account as string | undefined);
  });
}
