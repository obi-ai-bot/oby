// What the owner types in the address bar, made an address the way T3 Code's does it
// (pingdotgg/t3code@304cf149:packages/shared/src/preview.ts:75-222, `resolveAddressBarInput` and
// `normalizePreviewUrl`): an http(s) URL as it is; a bare host (`example.com`, `localhost:5173`,
// `127.0.0.1`) gets a scheme, http for loopback, https for the rest; anything else is a search
// (T3's default engine, DuckDuckGo). A non-web scheme (`file:`, `chrome:`, `javascript:`…) is no
// address: main refuses those too, and the app's own page (BROWSERAPP-28, main/pageRule.ts).

/** why typed words aren't an address the browser opens */
export class NotAnAddress extends Error {}

const SEARCH = 'https://duckduckgo.com/?q=%s';
const LOOPBACK_PREFIX = /^(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1?\])(?::|\/|$)/i;
const ADDRESS_HOST = /^(?:localhost|\d{1,3}(?:\.\d{1,3}){3}|\[[\da-f:.]+\])$/i;
const BARE_HOST_PORT = /^[^/?#:@]+:\d+(?:[/?#]|$)/;
const SCHEME_WITH_AUTHORITY = /^[a-z][a-z\d+.-]*:\/\//i;
const NON_WEB = new Set(['about:', 'blob:', 'chrome:', 'data:', 'devtools:', 'file:', 'ftp:', 'javascript:', 'mailto:', 'sms:', 'ssh:', 'tel:', 'view-source:', 'ws:', 'wss:']);

const protocolOf = (raw: string) => /^([A-Za-z][A-Za-z\d+.-]*):/.exec(raw)?.[1]?.toLowerCase().concat(':');
const loopbackHost = (host: string) => {
  const h = host.toLowerCase().replace(/^\[|\]$/g, '').replace(/\.+$/, '');
  return h === 'localhost' || h === '::1' || /^127\.\d+\.\d+\.\d+$/.test(h);
};

/** An address, made a URL (T3: normalizePreviewUrl): http for loopback, else https. */
export function normalizeAddress(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) throw new NotAnAddress('Type an address.');
  const explicit = protocolOf(trimmed);
  if (explicit !== undefined && NON_WEB.has(explicit)) throw new NotAnAddress('Oby opens only web pages.');
  const http = LOOPBACK_PREFIX.test(trimmed);
  const hasScheme = SCHEME_WITH_AUTHORITY.test(trimmed);
  let parsed: URL;
  try { parsed = new URL(hasScheme ? trimmed : `${http ? 'http' : 'https'}://${trimmed}`); } catch { throw new NotAnAddress('That isn’t an address.'); }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') throw new NotAnAddress('Oby opens only web pages.');
  if (!hasScheme && !http && loopbackHost(parsed.hostname)) return new URL(`http://${trimmed}`).href;
  return parsed.href;
}

/** What the address bar opens for the words typed in it (T3: resolveAddressBarInput). */
export function resolveAddress(raw: string): string {
  const trimmed = raw.trim();
  if (SCHEME_WITH_AUTHORITY.test(trimmed) || !trimmed) return normalizeAddress(trimmed);
  if (!/\s/.test(trimmed)) {
    const protocol = protocolOf(trimmed);
    if (protocol !== undefined && (NON_WEB.has(protocol) || !BARE_HOST_PORT.test(trimmed))) {
      if (protocol !== 'http:' && protocol !== 'https:') throw new NotAnAddress('Oby opens only web pages.');
      return normalizeAddress(trimmed);
    }
    const authority = (trimmed.split(/[/?#]/, 1)[0] ?? '').replace(/^[^@]*@/, '');
    const host = authority.replace(/:\d+$/, '');
    if (host.length < authority.length || ADDRESS_HOST.test(host) || /^[^.]+(?:\.[^.]+)+\.?$/.test(host)) {
      try { return normalizeAddress(trimmed); } catch { /* a search, below */ }
    }
  }
  return SEARCH.replaceAll('%s', encodeURIComponent(trimmed));
}
