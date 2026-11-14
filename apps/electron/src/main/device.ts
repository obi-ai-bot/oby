import { app, BrowserWindow, dialog, ipcMain, safeStorage } from 'electron';
import { createPrivateKey, createPublicKey, generateKeyPairSync, randomUUID, sign, type KeyObject } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { closeSync, constants, openSync, readFileSync, renameSync, rmSync, writeSync } from 'node:fs';
import { hostname } from 'node:os';
import { join } from 'node:path';
import { HOME, obid, takesKeys, type Answer, type Trusted } from './obid';
import { mayConfirm, takeInput, wordsHeader } from './words';

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

type Item = { id: string; kind: string; title: string; state: string; confirm?: { action?: string } };

// The owner's words, signed with this Mac's key: the preload saw the owner's own keystroke or
// click hold them, main saw that keystroke or click itself (main/words.ts, takeInput), and the
// request is one of the app's words requests in its exact shape (wordsHeader). Anything short of
// that, or a key or registration that fails, goes unsigned, as words always could: obid marks
// them. A limit-stopped job's continuation needs the signature (docs/runner-api.md, `accountLimit`).
// The device registers once a run (a Confirm still registers each time).
let device: string | null = null;
// Only main's native notification callbacks call this; never exposed over IPC. The callback
// holds the exact path and body its button names, independent of the window's input budget.
export async function notificationWords(method: string, path: string, body: unknown): Promise<Answer> {
  let words: string | null = null;
  try {
    words = wordsHeader(deviceKey(), (device ??= await register()), method, path, body);
  } catch (err) {
    console.error(`Oby couldn’t sign the notification answer: ${err instanceof Error ? err.message : String(err)}`);
  }
  return obid(method, path, body, words ?? undefined);
}

// A keyed request main signed on the owner's input that obid didn't answer (WORDS-16): the very
// same request sent again (its method, path and body byte for byte, its key included) is signed
// again, with a fresh time and no input, while main's own read says obid takes keys, so obid,
// which takes a key once, sees it signed as the first was. Kept until obid answers it, 10 s after
// its first signature, or 2 signatures again, whichever comes first; a few at most. Each signature
// again is counted before main waits on obid, so copies arriving together get 2 at most too.
const AGAIN_MS = 10_000, AGAIN = 2, KEPT = 8;
let unanswered: { request: string; at: number; again: number }[] = [];
const keyOf = (body: unknown) => typeof (body as { clientRequestId?: unknown } | null)?.clientRequestId === 'string';

// owners: the preload saw the owner's own keystroke or click hold its words
export async function ownersWords(method: string, path: string, body: unknown, owners = true): Promise<Answer> {
  const at = Date.now();
  unanswered = unanswered.filter((u) => at - u.at < AGAIN_MS && u.again < AGAIN);
  const request = keyOf(body) ? `${method} ${path}\n${JSON.stringify(body)}` : null;
  const kept = request === null ? undefined : unanswered.find((u) => u.request === request && u.again < AGAIN);
  if (kept) kept.again++;
  let words: string | null = null;
  try {
    if (kept && await takesKeys()) words = wordsHeader(deviceKey(), (device ??= await register()), method, path, body);
    else if (owners && takeInput()) words = wordsHeader(deviceKey(), (device ??= await register()), method, path, body);
  } catch (err) {
    console.error(`Oby couldn’t sign the owner’s words, so they go unsigned: ${err instanceof Error ? err.message : String(err)}`);
  }
  const r = await obid(method, path, body, words ?? undefined);
  if (r.status !== 0) unanswered = unanswered.filter((u) => u !== kept);
  else if (words && request !== null && !kept) unanswered = [...unanswered, { request, at, again: 0 }].slice(-KEPT);
  return r;
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

// The owner's own choices on a Publish (docs/runner-api.md, `GET /v1/jobs/{id}/publish`), nothing else.
type PublishChoices = { anyway?: boolean; force?: boolean; to?: string };
const publishChoices = (c: unknown): c is PublishChoices => {
  if (typeof c !== 'object' || c === null || Array.isArray(c)) return false;
  return Object.entries(c).every(([k, v]) => (k === 'anyway' || k === 'force' ? v === true : k === 'to' && typeof v === 'string' && v.length > 0));
};
const publishQuery = (c: PublishChoices) => {
  const q = new URLSearchParams(Object.entries(c).map(([k, v]) => [k, String(v)])).toString();
  return q ? `?${q}` : '';
};

// Confirm, on the owner's click in the window: the open `confirm` item, by its id, with this
// device's signature over obid's sentence for it (POST /v1/inbox/{id}/answer, "Confirm takes the
// owner's tap")
export function confirming(trusted: Trusted) {
  ipcMain.handle('obid-confirm', async (e, id: unknown, clicked: unknown): Promise<Answer> => {
    if (!trusted(e) || typeof id !== 'string') return { status: 403, text: '' };
    try {
      const inbox = await obid('GET', '/v1/inbox');
      if (inbox.status !== 200) return inbox;
      const item = (JSON.parse(inbox.text) as Item[]).find((i) => i.id === id && i.kind === 'confirm' && i.state !== 'done');
      if (!item) return { status: 409, text: JSON.stringify({ error: 'That isn’t waiting for a Confirm anymore.' }) };
      if (!mayConfirm(item, clicked === true)) return { status: 403, text: JSON.stringify({ error: 'That takes your own click.' }) };
      const device = await register(), signedAt = now();
      const signature = signed(`oby-confirm\n${item.id}\n${item.title}\n${signedAt}`);
      // main's own key, once obid takes keys (RECEIPTS-16): a Confirm is sent once, never again
      const key = await takesKeys() ? { clientRequestId: randomUUID() } : {};
      return obid('POST', `/v1/inbox/${encodeURIComponent(item.id)}/answer`, { answer: 'Confirm', approval: { device, action: item.title, signedAt, signature }, ...key });
    } catch (err) {
      return { status: 500, text: JSON.stringify({ error: err instanceof Error ? err.message : String(err) }) };
    }
  });

  // Pair a device, from Settings › Remote access: an invitation (POST /v1/pairing/invite) with this Mac's
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

  // Publish by hand, on the owner's click (PUBLISHING-9 to 11, 14). A push can't rest on the page,
  // which a script could have taken over: main reads obid's own sentence for those choices itself,
  // shows it in main's own dialog, which the page can't click or word (as Pair a device's), and signs
  // only on Publish there. Asked only right after an input main saw, and for the sentence the page
  // showed, so a script can't raise dialogs by itself, and one that changed isn't sprung on the owner.
  ipcMain.handle('obid-publish', async (e, id: unknown, action: unknown, choices: unknown): Promise<Answer> => {
    if (!trusted(e) || typeof id !== 'string' || typeof action !== 'string' || !publishChoices(choices)) return { status: 403, text: '' };
    if (!takeInput()) return { status: 403, text: JSON.stringify({ error: 'Publish takes your own click.' }) };
    try {
      const path = `/v1/jobs/${encodeURIComponent(id)}/publish`;
      const sentence = await obid('GET', path + publishQuery(choices));
      if (sentence.status !== 200) return sentence;
      // `head` and `over`, obid's own, are the commits that sentence names (PUBLISHING-9, 10)
      const read = JSON.parse(sentence.text) as { action?: string; head?: string; over?: string };
      if (read.action !== action) {
        return { status: 409, text: JSON.stringify({ error: 'What publishing would do changed since it showed. Look at it again.' }) };
      }
      const w = BrowserWindow.fromWebContents(e.sender);
      const asked = { type: 'question' as const, message: 'Publish this?', detail: action, buttons: ['Publish', 'Not now'], defaultId: 1, cancelId: 1 };
      const { response } = await (w ? dialog.showMessageBox(w, asked) : dialog.showMessageBox(asked));
      if (response !== 0) return { status: 409, text: JSON.stringify({ error: 'Not published: you said not now.' }) };
      const device = await register(), signedAt = now();
      return obid('POST', path, {
        device, action, signedAt, signature: signed(`oby-publish\n${id}\n${action}\n${signedAt}`), ...choices,
        ...(choices.anyway && read.head !== undefined ? { head: read.head } : {}), ...(choices.force && read.over !== undefined ? { over: read.over } : {}),
      });
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
