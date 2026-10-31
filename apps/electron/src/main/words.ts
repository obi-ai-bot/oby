import { createHash, sign, type KeyObject } from 'node:crypto';
import { wordsOf } from '../shared/words';

// X-Oby-Words for the owner's words (docs/runner-api.md, "Words the owner typed"): `<device>
// <signedAt> <signature>`, ECDSA P-256 with SHA-256 over obid's sentence for exactly this request,
// its body as send() sends it (JSON), signed to the millisecond. Null for a request that carries
// no words obid reads a signature on: main signs nothing else.
export function wordsHeader(key: KeyObject, device: string, method: string, path: string, body: unknown, signedAt = new Date().toISOString()): string | null {
  if (!wordsOf(method, path, body)) return null;
  const digest = createHash('sha256').update(JSON.stringify(body)).digest('hex');
  const signature = sign('sha256', Buffer.from(`oby-words\n${method}\n${path.split('?')[0]}\n${digest}\n${signedAt}`, 'utf8'), key).toString('base64');
  return `${device} ${signedAt} ${signature}`;
}
