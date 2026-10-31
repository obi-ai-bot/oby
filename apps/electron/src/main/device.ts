import { app, ipcMain, safeStorage } from 'electron';
import { createPrivateKey, createPublicKey, generateKeyPairSync, sign, type KeyObject } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { closeSync, constants, openSync, readFileSync, renameSync, rmSync, writeSync } from 'node:fs';
import { hostname } from 'node:os';
import { join } from 'node:path';
import { HOME, obid, type Answer, type Trusted } from './obid';
import { wordsHeader } from './words';

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

function name() {
  let mac = hostname();
  try { mac = execFileSync('/usr/sbin/scutil', ['--get', 'ComputerName'], { encoding: 'utf8' }).trim() || mac; } catch { /* the host name, then */ }
  return `${mac} (Oby desktop)`;
}

const said = (a: Answer) => { try { return (JSON.parse(a.text) as { error?: string }).error ?? a.text; } catch { return a.text; } };

// Registered with obid, every time (a key it knows answers its id at once). A new key goes in only
// with the owner present: on their click, as the Swift Mac app does, home/pairing names it for the
// moment the registration takes (obid takes it within 2 minutes), and is removed whatever happened.
export async function register(): Promise<string> {
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

// The owner's words (wordsOf's routes only), signed with this Mac's key, on the preload's word
// that the owner's own keystroke or click held them (preload/index.ts). A limit-stopped job's
// continuation needs it (docs/runner-api.md, `accountLimit`); everything else just isn't marked.
export async function ownersWords(method: string, path: string, body: unknown): Promise<Answer> {
  try {
    const words = wordsHeader(deviceKey(), await register(), method, path, body);
    return obid(method, path, body, words ?? undefined);
  } catch (err) {
    return { status: 500, text: JSON.stringify({ error: err instanceof Error ? err.message : String(err) }) };
  }
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
