// Words the owner typed or clicked, which main signs with this Mac's device key (X-Oby-Words,
// docs/runner-api.md, "Words the owner typed"; docs/contracts/signed-words.md). The preload asks
// main to sign a request only when its words are text the owner's own keystroke or click held
// (owners); main signs only these routes (wordsOf), whatever it's asked.

const text = (v: unknown) => (typeof v === 'string' ? v : undefined);
const field = (body: unknown, key: string) => text((body as Record<string, unknown> | null)?.[key]);

// The words a request carries, for the routes obid reads a signature on; null for any other
// request, or one whose words aren't text (main never signs it)
export function wordsOf(method: string, path: string, body: unknown): string[] | null {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) return null;
  const p = path.split('?')[0];
  const words = (...ws: (string | undefined)[]) => (ws[0] === undefined ? null : ws.filter((w): w is string => w !== undefined));
  if (method === 'POST' && p === '/v1/plans') {
    const answer = (body as { answer?: unknown }).answer;
    // an answer to Oby's question: the words it answers, and the owner's answer
    return answer === undefined ? words(field(body, 'said')) : field(answer, 'option') === undefined ? null : words(field(body, 'said'), field(answer, 'option'));
  }
  if (method === 'POST' && p === '/v1/jobs') {
    const job = (body as { job?: unknown }).job;
    return words(field(job, 'task'), field(job, 'title'));
  }
  if (method === 'POST' && /^\/v1\/jobs\/[^/]+\/messages$/.test(p)) return words(field(body, 'text'));
  if (method === 'PATCH' && /^\/v1\/jobs\/[^/]+$/.test(p)) return words(field(body, 'title'));
  if (method === 'POST' && /^\/v1\/inbox\/[^/]+\/answer$/.test(p)) return words(field(body, 'answer'));
  return null;
}

const norm = (s: string) => s.replace(/\s+/g, ' ').trim();
// the repository the owner picked, put in front of words by the page (live.ts, inRepo)
const PICKED = /^In [\w.-]+\/[\w.-]+: /;

// Whether every one of `words` is text the owner held (`held`: a field's text or a button's label
// at their keystroke or click). Spaces don't count: the box's lines come back as the page joins them.
export function ownedBy(words: string[], held: string[]): boolean {
  const mine = new Set(held.map(norm).filter(Boolean));
  return words.length > 0 && words.every((w) => {
    const n = norm(w);
    return mine.has(n) || (PICKED.test(n) && mine.has(n.replace(PICKED, '')));
  });
}

// Whether the request's words are the owner's: held by their keystroke or click (wordsOf's routes
// only). An answer to Oby's question sends again the words it answers: those may be words to Oby
// signed before (`earlier`, POST /v1/plans's `said`), never anything else.
export function owners(method: string, path: string, body: unknown, held: string[], earlier: string[]): boolean {
  const words = wordsOf(method, path, body);
  if (!words) return false;
  const answers = path.split('?')[0] === '/v1/plans' && (body as { answer?: unknown }).answer !== undefined;
  return answers ? ownedBy(words.slice(0, 1), [...held, ...earlier]) && ownedBy(words.slice(1), held) : ownedBy(words, held);
}

// How long a keystroke or click holds its text: a send that obid didn't answer is sent again, and
// files upload before their words go
export const HELD_MS = 30_000;
