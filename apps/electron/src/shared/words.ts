// Words the owner typed or clicked, which main signs with this Mac's device key (X-Oby-Words,
// docs/runner-api.md, "Words the owner typed"; docs/contracts/signed-words.md). The preload asks
// main to sign a request only when its words are text the owner's own keystroke or click held
// (owners); main signs only these requests, in exactly the shapes the app sends them (wordsOf),
// and only right after a real keystroke or click it saw itself (main/words.ts).

type Body = Record<string, unknown>;
const isBody = (v: unknown): v is Body => typeof v === 'object' && v !== null && !Array.isArray(v);
const text = (v: unknown): v is string => typeof v === 'string';
const strings = (v: unknown) => Array.isArray(v) && v.every(text);
// only these keys, each of the kind it must be: anything else and nothing is signed
const shaped = (body: Body, keys: Record<string, (v: unknown) => boolean>) =>
  Object.entries(body).every(([k, v]) => keys[k]?.(v) ?? false);
// the label of the button that answers a permission (live.ts, DECISIONS)
export const DECISIONS: Record<string, string> = { allow: 'Allow', always: 'Always allow', deny: 'Deny' };

// The words a request carries, for the requests the app sends obid with the owner's words in
// them; null for any other request, or any other shape (main never signs it). A key the app
// doesn't send (a job's check, mode or repository, say) is never signed along with the words.
export function wordsOf(method: string, path: string, body: unknown): string[] | null {
  if (!isBody(body)) return null;
  const p = path.split('?')[0];
  const id = text, flag = (v: unknown) => v === true;
  if (method === 'POST' && p === '/v1/plans') {
    const answer = body.answer;
    const ok = text(body.said) && shaped(body, {
      said: text, answer: (a) => isBody(a) && text(a.question) && text(a.option) && shaped(a, { question: text, option: text }),
      conversation: id, new: flag, folders: strings, id, attachments: strings, jobs: strings,
    });
    // an answer to Oby's question: the words it answers, and the owner's answer
    return ok ? [body.said as string, ...(isBody(answer) ? [answer.option as string] : [])] : null;
  }
  if (method === 'POST' && /^\/v1\/jobs\/[^/]+\/messages$/.test(p)) {
    return text(body.text) && shaped(body, { text, attachments: strings, conversation: flag }) ? [body.text as string] : null;
  }
  if (method === 'PATCH' && /^\/v1\/jobs\/[^/]+$/.test(p)) return text(body.title) && shaped(body, { title: text }) ? [body.title] : null;
  if (method === 'POST' && /^\/v1\/inbox\/[^/]+\/answer$/.test(p)) return text(body.answer) && shaped(body, { answer: text }) ? [body.answer] : null;
  if (method === 'POST' && /^\/v1\/jobs\/[^/]+\/permission$/.test(p)) {
    const label = DECISIONS[body.decision as string];
    return label && shaped(body, { id, decision: text, message: text }) ? [label, ...(text(body.message) ? [body.message] : [])] : null;
  }
  return null;
}

const norm = (s: string) => s.replace(/\s+/g, ' ').trim();

// Which of `held` (a box's text or a button's label at the owner's keystroke or click) the
// request's words are, each held text used once: their indexes, or null when they aren't all
// there. An answer to Oby's question sends again the words it answers: those may be words to Oby
// signed before (`earlier`, POST /v1/plans's `said`), never anything else. Spaces don't count: the
// box's lines come back as the page joins them.
export function owners(method: string, path: string, body: unknown, held: string[], earlier: string[]): number[] | null {
  const words = wordsOf(method, path, body);
  if (!words) return null;
  const answers = isBody(body) && body.answer !== undefined && path.split('?')[0] === '/v1/plans';
  const used: number[] = [];
  for (const [i, w] of words.entries()) {
    const n = norm(w);
    if (!n) return null;
    if (answers && i === 0 && earlier.some((e) => norm(e) === n)) continue;
    const at = held.findIndex((h, j) => !used.includes(j) && norm(h) === n);
    if (at < 0) return null;
    used.push(at);
  }
  return used;
}

// How long a keystroke or click holds its text (preload) and its one signature (main): the words
// go at once. A send obid didn't answer, sent again later, or words that wait for their files to
// upload, go unsigned (files mark them anyway).
export const WITHIN_MS = 2_000;
