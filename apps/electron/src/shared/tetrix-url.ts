// The address of the owner's Tetrix (its MCP server), as the person types it. Oby has no
// built-in Tetrix and no default: the owner enters theirs, in the first conversation or in
// Settings › Connectors, and until then Oby runs without Tetrix. What's taken is what obid takes
// (docs/runner-api.md, "obid only: Tetrix", a connection's rule): https, with a host, and no
// login, query or fragment in it. obid checks it again.

// Why `text` isn't an address Oby sends to obid, in a sentence for the field, or null when it is.
export function tetrixUrlProblem(text: string): string | null {
  const typed = text.trim();
  if (!typed) return 'Enter your Tetrix’s address, starting with https://.';
  let url: URL;
  try {
    url = new URL(typed);
  } catch {
    return 'An https:// address';
  }
  if (url.protocol !== 'https:') return 'An https:// address';
  if (!url.hostname) return 'The address needs a host, like tetrix.example.com.';
  if (url.username || url.password) return 'Leave the login out of the address: you sign in on Tetrix’s own page.';
  if (url.search || url.hash || typed.includes('?') || typed.includes('#')) return 'Leave the ? and # parts out of the address.';
  return null;
}

// The address as it goes to obid: as typed, trimmed. Only for text tetrixUrlProblem passed.
export const tetrixUrl = (text: string) => text.trim();
