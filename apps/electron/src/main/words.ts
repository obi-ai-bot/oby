import { createHash, sign, type KeyObject } from 'node:crypto';
import { WITHIN_MS, wordsOf } from '../shared/words';

// X-Oby-Words for the owner's words (docs/runner-api.md, "Words the owner typed"): `<device>
// <signedAt> <signature>`, ECDSA P-256 with SHA-256 over obid's sentence for exactly this request,
// its body as send() sends it (JSON), signed to the millisecond. Null for a request that isn't one
// of the app's words requests in exactly its shape (wordsOf): main signs nothing else.
export function wordsHeader(key: KeyObject, device: string, method: string, path: string, body: unknown, signedAt = new Date().toISOString()): string | null {
  if (!wordsOf(method, path, body)) return null;
  const digest = createHash('sha256').update(JSON.stringify(body)).digest('hex');
  const signature = sign('sha256', Buffer.from(`oby-words\n${method}\n${path.split('?')[0]}\n${digest}\n${signedAt}`, 'utf8'), key).toString('base64');
  return `${device} ${signedAt} ${signature}`;
}

// The owner's real keystrokes (Enter) and clicks, as main itself saw them reach the window
// (index.ts: before-input-event, input-event), never as the page says: each allows one signature
// within WITHIN_MS of it. The page's own process can't make one.
let inputs: number[] = [];
export function input(at = Date.now()) {
  inputs = [...inputs.filter((t) => at - t < WITHIN_MS), at];
}
export function takeInput(at = Date.now()): boolean {
  inputs = inputs.filter((t) => at - t < WITHIN_MS);
  return inputs.shift() !== undefined;
}
