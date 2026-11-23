import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';
import bridge from '../../resources/preview-bridge/bridge.js?raw';
import relayHtml from '../../resources/sandpack-relay/__csb_relay/index.html?raw';
import relayJs from '../../resources/sandpack-relay/__csb_relay/__csb_relay.30yfeq026i8arwjbp19vlgcep8e6tgf.js?raw';
import workerJs from '../../resources/sandpack-relay/__csb_sw.rpfsli6sirrzs1630o3lgzuxwlr2uqc.js?raw';
import landingHtml from '../../resources/sandpack-relay/index.html?raw';

// Sandpack's relay, served by main (docs/contracts/artifacts-app.md, ARTIFACTSAPP-1 and 3). Sandpack's
// `static` template shows a preview at `<id>-<bundler host>`: a hidden relay frame there
// (`/__csb_relay/`) registers a service worker that answers the preview's requests from the files in
// memory. Its bundler host is this server, `preview.localhost` on a port of 127.0.0.1 main picks, so
// no preview reaches CodeSandbox's hosted copy. Chromium sends `*.localhost` to loopback by itself
// and counts it a secure context, where a service worker may register.
//
// Only a host `preview.localhost` or `<id>-preview.localhost`, with this server's port, is served;
// any other host, path or method is 404. The four files are static-browser-server 1.0.3's, as
// vendored (resources/sandpack-relay/README.md), with two lines of the service worker changed as
// it's served: a request to another origin goes to the network, where the CSP and the preview's
// partition stop it, instead of being answered from the artifact's files; and every answer the
// worker makes carries PREVIEW_CSP. It also serves what a React artifact runs on (ARTIFACTSAPP-13 and
// 26): every `.mjs` and `.js` under resources/sandpack-vendor/, at `/__csb_vendor/<package>/<file>`,
// and nothing else of that folder (its licences and README stay 404). The worker sends a `/__csb…`
// path of the preview's own origin to the network, so it reaches this server. So does the bridge
// every page gets (ARTIFACTSAPP-22 and 23), resources/preview-bridge/, at `/__csb_oby/bridge.js`.
//
// A Claude Design design is drawn by obid's own canvas host and runtime (ARTIFACTS-13 to 15), whose
// paths are root-relative, so it needs an origin of its version's own (ARTIFACTSAPP-29 and 30). This
// server gives it one: `<id>-v<n>.localhost` on its port, for the version main shows and no other,
// each path read from obid's file route (`GET /v1/artifacts/<id>/<n>/<path>`) with main's token and
// sent on with obid's status, type, policy and CORS as obid sent them. An answer of obid's that
// carries no policy is never sent on.

// import.meta.glob takes a literal: its keys start with this
const VENDOR = '../../resources/sandpack-vendor/';
/** The vendored modules, by their path under resources/sandpack-vendor/. */
const vendored = import.meta.glob<string>('../../resources/sandpack-vendor/**/*.{mjs,js}', { query: '?raw', import: 'default', eager: true });

/** What a preview may load: its own origin only, and what it holds inline (data:, blob:). */
export const PREVIEW_CSP = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:",
  "style-src 'self' 'unsafe-inline' blob:",
  "img-src 'self' data: blob:",
  "font-src 'self' data:",
  "media-src 'self' data: blob:",
  "connect-src 'self'",
  "worker-src 'self' blob:",
  "frame-src 'self' blob: data:",
  "object-src 'none'",
  "base-uri 'self'",
  "form-action 'self'",
].join('; ');

/** The relay's own pages: its scripts, its styles, nothing from anywhere else. */
export const RELAY_CSP = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'";

// The worker's two lines, each found exactly once or nothing is served
const OWN_ORIGIN_ONLY = {
  from: '    if (parsedUrl.pathname.startsWith("/__csb")) {\n',
  to: '    if (parsedUrl.origin !== self.location.origin) {\n      return;\n    }\n    if (parsedUrl.pathname.startsWith("/__csb")) {\n',
};
const WITH_CSP = {
  from: '        headers: response.headers,\n',
  to: `        headers: { ...response.headers, "Content-Security-Policy": ${JSON.stringify(PREVIEW_CSP)} },\n`,
};

function once(text: string, change: { from: string; to: string }): string {
  const at = text.indexOf(change.from);
  if (at < 0 || text.indexOf(change.from, at + 1) >= 0) throw new Error(`The relay's service worker changed: ${JSON.stringify(change.from.trim())} isn't there once.`);
  return text.slice(0, at) + change.to + text.slice(at + change.from.length);
}

/** The service worker as served: its two lines changed (throws when either isn't found once). */
export function servedWorker(source = workerJs): string {
  return once(once(source, OWN_ORIGIN_ONLY), WITH_CSP);
}

type File = { body: string; type: string };

function files(): Map<string, File> {
  const js = 'text/javascript; charset=utf-8', html = 'text/html; charset=utf-8';
  return new Map<string, File>([
    ['/', { body: landingHtml, type: html }],
    ['/index.html', { body: landingHtml, type: html }],
    ['/__csb_relay/', { body: relayHtml, type: html }],
    ['/__csb_relay/index.html', { body: relayHtml, type: html }],
    ['/__csb_relay/__csb_relay.30yfeq026i8arwjbp19vlgcep8e6tgf.js', { body: relayJs, type: js }],
    ['/__csb_sw.rpfsli6sirrzs1630o3lgzuxwlr2uqc.js', { body: servedWorker(), type: js }],
    ...Object.entries(vendored).map(([path, body]): [string, File] => [`/__csb_vendor/${path.slice(VENDOR.length)}`, { body, type: js }]),
    ['/__csb_oby/bridge.js', { body: bridge, type: js }],
  ]);
}

/** A preview host this server answers: `preview.localhost` or `<id>-preview.localhost`, on its port. */
export function servedHost(host: string | undefined, port: number): boolean {
  if (!host) return false;
  const m = /^(?:([a-z0-9]{1,64})-)?preview\.localhost:(\d{1,5})$/.exec(host.toLowerCase());
  return !!m && Number(m[2]) === port;
}

const UUID = '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}';
/** A design's version this server's host names: `<id>-v<n>.localhost` on its port, else null. */
export function designHost(host: string | undefined, port: number): { id: string; version: number } | null {
  const m = new RegExp(`^(${UUID})-v([1-9][0-9]{0,5})\\.localhost:(\\d{1,5})$`).exec(host ?? '');
  return m && Number(m[3]) === port ? { id: m[1], version: Number(m[2]) } : null;
}

/** The path of a design's file as asked, kept percent-encoded for obid, or null: one that climbs
 * (`..`, `.`, an empty segment, a backslash, a NUL, either encoded too) is never asked of obid. */
export function designPath(raw: string): string | null {
  if (!raw.startsWith('/') || raw.length > 2048) return null;
  const parts = raw.slice(1).split('/');
  for (const p of parts) {
    let d: string;
    try { d = decodeURIComponent(p); } catch { return null; }
    if (!d || d === '.' || d === '..' || /[\\/\0]/.test(d)) return null;
  }
  return raw;
}

// the headers of obid's that a design's answer keeps
const PASSED = ['content-type', 'content-security-policy', 'access-control-allow-origin', 'x-content-type-options', 'referrer-policy'];

/** How the relay reaches obid for a design: whether main shows that version, and obid's answer for
 * one of its files (a Response, or why obid didn't answer). */
export type Designs = {
  shown(id: string, version: number): boolean;
  read(id: string, version: number, path: string): Promise<Response | { status: number; text: string }>;
};

export type Relay = { port: number; origin: string; close(): Promise<void> };

const notHere = (res: ServerResponse, status = 404, text = 'Not here.') => {
  res.writeHead(status, { 'Content-Type': 'text/plain; charset=utf-8', 'Content-Security-Policy': RELAY_CSP, 'Cache-Control': 'no-store' }).end(text);
};

async function design(req: IncomingMessage, res: ServerResponse, at: { id: string; version: number }, designs: Designs) {
  const path = designPath((req.url ?? '/').split('?')[0]);
  if ((req.method !== 'GET' && req.method !== 'HEAD') || !path || !designs.shown(at.id, at.version)) return notHere(res);
  const r = await designs.read(at.id, at.version, path).catch((e: unknown) => ({ status: 0, text: String(e) }));
  if (!(r instanceof Response)) return notHere(res, 502, r.text || 'obid isn’t answering.');
  const body = new Uint8Array(await r.arrayBuffer());
  // never a file of a design without obid's policy on it
  if (r.ok && !r.headers.get('content-security-policy')) return notHere(res, 502, 'obid sent no policy.');
  const headers: Record<string, string> = { 'Cache-Control': 'no-store' };
  for (const h of PASSED) { const v = r.headers.get(h); if (v) headers[h] = v; }
  if (!r.ok) headers['content-security-policy'] ??= RELAY_CSP;
  res.writeHead(r.status, headers);
  res.end(req.method === 'HEAD' ? undefined : body);
}

/** The relay, listening on 127.0.0.1 on a port the system picks; with `designs`, a design's
 * version on its own origin too. */
export async function startRelay(designs?: Designs): Promise<Relay> {
  const served = files();
  let port = 0;
  const server: Server = createServer((req: IncomingMessage, res: ServerResponse) => {
    const at = designs ? designHost(req.headers.host, port) : null;
    if (at && designs) { void design(req, res, at, designs).catch(() => { if (!res.headersSent) notHere(res, 502, 'obid isn’t answering.'); else res.end(); }); return; }
    const path = (req.url ?? '/').split('?')[0];
    const file = served.get(path);
    if ((req.method !== 'GET' && req.method !== 'HEAD') || !servedHost(req.headers.host, port) || !file) {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8', 'Content-Security-Policy': RELAY_CSP }).end('Not here.');
      return;
    }
    res.writeHead(200, {
      'Content-Type': file.type,
      'Content-Security-Policy': RELAY_CSP,
      'Cache-Control': 'no-store',
      'X-Content-Type-Options': 'nosniff',
      // static-browser-server's own preview server says so: its pages are never proxied by a worker
      'x-csb-no-sw-proxy': '1',
    });
    res.end(req.method === 'HEAD' ? undefined : file.body);
  });
  await new Promise<void>((done, failed) => { server.once('error', failed); server.listen(0, '127.0.0.1', () => done()); });
  const address = server.address();
  port = typeof address === 'object' && address ? address.port : 0;
  return {
    port,
    origin: `http://preview.localhost:${port}`,
    close: () => new Promise<void>((done) => server.close(() => done())),
  };
}
