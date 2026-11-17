// The address of the person's own Tetrix, as a field takes it (TETRIX-URL-1 to 3,
// docs/contracts/tetrix-url.md). Oby has no Tetrix of its own to offer: the person types where theirs is.
// This checks what obid's `url_problem` checks first (core/crates/obid/src/tetrix.rs), so Connect waits
// until obid would take the address; obid still decides, and its `reason` is the sentence under the
// field when it doesn't (signInWhy.ts).

export const TETRIX_URL_PLACEHOLDER = 'https://tetrix.example.com/mcp';

const LOOPBACK = new Set(['127.0.0.1', 'localhost', '[::1]']);
// Tetrix's one query: which organization a login in several answers for
const ORG = /^\?org=[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export type TetrixUrl = { url: string } | { problem: string };

// What the person typed, as obid is given it: trimmed and written the way it's read (a bare host
// reads as `https://host/`, which obid insists on). A problem is a sentence for under the field; an
// empty field isn't one (nothing to say until they type).
export function tetrixUrl(text: string): TetrixUrl | null {
  const typed = text.trim();
  if (!typed) return null;
  const bad = (problem: string): TetrixUrl => ({ problem });
  let url: URL;
  try { url = new URL(typed); } catch { return bad('That isn’t a web address. Try one like https://tetrix.example.com/mcp.'); }
  const loopback = LOOPBACK.has(url.hostname.toLowerCase());
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) return bad('Use a secure address, one that starts with https://.');
  if (!url.hostname) return bad('That address has no host. Try one like https://tetrix.example.com/mcp.');
  if (url.username || url.password) return bad('Leave the login out of the address.');
  if (url.hash) return bad('Leave the # part out of the address.');
  if (url.search && !ORG.test(url.search)) return bad('The one thing an address may carry after ? is org=<your organization’s id>.');
  return { url: url.href };
}
