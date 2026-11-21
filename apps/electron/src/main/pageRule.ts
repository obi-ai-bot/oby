/** a page's address the window loads: the web, or a blank page */
export const pageOk = (url: unknown) => typeof url === 'string' && /^(https?:\/\/|about:blank$)/.test(url);

/**
 * whether a URL's parsed host (WHATWG: lower-cased, IPv4 in dotted quads, IPv6 compressed) reaches
 * this Mac: `localhost` and every `*.localhost` name (Chromium sends them to loopback), with or
 * without the root's dot, 127.0.0.0/8, 0.0.0.0, `[::1]`, `[::]` and their IPv4-mapped forms
 */
export function loopback(host: string): boolean {
  const h = host.replace(/\.$/, '');
  if (h === 'localhost' || h.endsWith('.localhost')) return true;
  if (/^127\.\d+\.\d+\.\d+$/.test(h) || h === '0.0.0.0') return true;
  return h === '[::1]' || h === '[::]' || /^\[::ffff:(7f[0-9a-f]{2}:[0-9a-f]{1,4}|0:0)\]$/.test(h);
}
const parse = (url: string) => { try { return new URL(url); } catch { return null; } };
/**
 * whether `url` is the running app's own page: the packaged app's `file:` one, or in a development
 * run anything on its dev server (ELECTRON_RENDERER_URL): its port on any loopback host
 */
export function appPage(url: unknown, dev = process.env.ELECTRON_RENDERER_URL): boolean {
  if (typeof url !== 'string') return false;
  if (url.startsWith('file:')) return true;
  const a = parse(url), b = dev ? parse(dev) : null;
  if (!a || !b) return false;
  const port = (u: URL) => u.port || (u.protocol === 'https:' ? '443' : '80');
  return port(a) === port(b) && (loopback(b.hostname) ? loopback(a.hostname) : a.hostname === b.hostname);
}
