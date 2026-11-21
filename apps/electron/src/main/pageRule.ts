/** a page's address the window loads: the web, or a blank page */
export const pageOk = (url: unknown) => typeof url === 'string' && /^(https?:\/\/|about:blank$)/.test(url);

const LOOPBACK = new Set(['localhost', '127.0.0.1', '[::1]']);
const at = (url: string) => { try { const u = new URL(url); return { protocol: u.protocol, host: LOOPBACK.has(u.hostname) ? 'loopback' : u.hostname, port: u.port }; } catch { return null; } };
/**
 * whether `url` is the running app's own page: the packaged app's `file:` one, or in a development
 * run its dev server's origin (ELECTRON_RENDERER_URL), on any loopback name for it
 */
export function appPage(url: unknown, dev = process.env.ELECTRON_RENDERER_URL): boolean {
  if (typeof url !== 'string') return false;
  if (url.startsWith('file:')) return true;
  const a = at(url), b = dev ? at(dev) : null;
  return !!a && !!b && a.protocol === b.protocol && a.host === b.host && a.port === b.port;
}
