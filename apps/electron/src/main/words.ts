import { createHash, sign, type KeyObject } from 'node:crypto';
import { WITHIN_MS, wordsOf } from '../shared/words';

// X-Oby-Words for the owner's words (docs/runner-api.md, "Words the owner typed"): `<device>
// <signedAt> <signature>`, ECDSA P-256 with SHA-256 over obid's sentence for exactly this request,
// its body as send() sends it (JSON), signed to the millisecond. Null for a request that isn't one
// of the app's words requests in exactly its shape (wordsOf): main signs nothing else.
export function wordsHeader(key: KeyObject, device: string, method: string, path: string, body: unknown, signedAt = new Date().toISOString()): string | null {
  if (!wordsOf(method, path, body) || !ownFolders(body)) return null;
  const digest = createHash('sha256').update(JSON.stringify(body)).digest('hex');
  const signature = sign('sha256', Buffer.from(`oby-words\n${method}\n${path.split('?')[0]}\n${digest}\n${signedAt}`, 'utf8'), key).toString('base64');
  return `${device} ${signedAt} ${signature}`;
}

// The folders the owner chose in main's own dialog this run (index.ts, pick-folders): words that
// carry any other folder (a recent one from an earlier run, or one the page made up) go unsigned,
// so the page can't put a vetted conversation in a folder the owner never picked.
const chosen = new Set<string>();
export const picked = (paths: string[]) => { for (const p of paths) chosen.add(p); };
const ownFolders = (body: unknown) => {
  const folders = (body as { folders?: unknown }).folders;
  return folders === undefined || (Array.isArray(folders) && folders.every((f) => chosen.has(f)));
};

// The owner's real keystrokes (Enter) and clicks, as main itself saw them reach the window
// (index.ts: before-input-event, input-event), never as the page says: each allows one signature
// within WITHIN_MS of it. The page's own process can't make one.
let inputs: { at: number; select: boolean }[] = [];
export function input(at = Date.now(), select = false) {
  inputs = [...inputs.filter((t) => at - t.at < WITHIN_MS), { at, select }];
}
export function takeInput(at = Date.now(), select = false): boolean {
  inputs = inputs.filter((t) => at - t.at < WITHIN_MS);
  const index = inputs.findIndex((t) => !t.select || select);
  return index >= 0 && inputs.splice(index, 1).length === 1;
}

// Native selects can change with navigation keys without Enter or a click. Main observes these
// itself; they authorize only Default mode, never ordinary words or a Confirm. Enter keeps its
// existing single token, so a key never grants both kinds of signature.
export function keyInput(i: { type: string; key: string; code: string; isAutoRepeat: boolean }) {
  if (i.type !== 'keyDown' || i.isAutoRepeat) return;
  if (i.key === 'Enter') input();
  else if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(i.key)) input(Date.now(), true);
  if (i.code === 'Space') space();
}

export const takeWordsInput = (method: string, path: string, body: unknown) =>
  takeInput(Date.now(), method === 'PUT' && path.split('?')[0] === '/v1/settings/permissions' && wordsOf(method, path, body) !== null);

// Space on a focused button, which clicks it, is the owner's too, but only for the Confirm below:
// a Space while typing signs no words.
let spaces: number[] = [];
export function space(at = Date.now()) {
  spaces = [...spaces.filter((t) => at - t < WITHIN_MS), at];
}
const takeSpace = (at: number) => {
  spaces = spaces.filter((t) => at - t < WITHIN_MS);
  return spaces.shift() !== undefined;
};

// A Confirm that tells other people something takes a press of that button: letting this Oby tell them
// by itself (PEER-17, ROOMS-17), or answering another Oby's question as the owner (PEER-33). The preload
// saw a button labelled Confirm held by a real event (`clicked`), and main saw a real click or key
// itself. Other Confirms sign on the page's say, as they did. It stops a script acting on an input it
// didn't arrange, no more (docs/contracts/signed-words.md).
const PRESSED = ['share_status', 'share_room', 'answer_question', 'decline_remember'];
export const mayConfirm = (item: { confirm?: { action?: string } }, clicked: boolean, at = Date.now()) =>
  !PRESSED.includes(item.confirm?.action ?? '') || (clicked && (takeInput(at) || takeSpace(at)));
