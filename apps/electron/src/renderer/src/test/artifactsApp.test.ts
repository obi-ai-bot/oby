import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { request } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { PREVIEW_CSP, RELAY_CSP, servedHost, servedWorker, startRelay, type Relay } from '../../../main/sandpackRelay';
import { allowed, noFiles, overlay, previewAsk, shownAs } from '../../../main/previewRules';
import { showable } from '../artifacts/showable';
import { importType } from '../../../shared/artifactImport';
import { exportArtifact, nameOf, saveFile } from '../../../main/previewRules';
// @ts-expect-error a plain script
import { FORBIDDEN, forbiddenIn, localOnly } from '../../../../scripts/local-only.mjs';

// a disk that fills partway through a write when told to (disk.fails): half the bytes land, then it fails
const disk = vi.hoisted(() => ({ fails: false }));
vi.mock('node:fs/promises', async (original) => {
  const fs = await original<typeof import('node:fs/promises')>();
  const half = async (write: (b: Uint8Array) => Promise<void>, bytes: Uint8Array) => {
    await write(bytes.subarray(0, bytes.length >> 1));
    throw Object.assign(new Error('no space left on device'), { code: 'ENOSPC' });
  };
  const writeFile = ((path: string, bytes: Uint8Array, ...rest: []) => disk.fails ? half((b) => fs.writeFile(path, b), bytes) : fs.writeFile(path, bytes, ...rest)) as typeof fs.writeFile;
  const open = (async (...a: Parameters<typeof fs.open>) => {
    const h = await fs.open(...a);
    return disk.fails ? new Proxy(h, { get: (t, k) => k === 'writeFile' ? (b: Uint8Array) => half((x) => t.writeFile(x), b) : typeof t[k as keyof typeof t] === 'function' ? (t[k as keyof typeof t] as (...x: unknown[]) => unknown).bind(t) : t[k as keyof typeof t] }) : h;
  }) as typeof fs.open;
  return { ...fs, default: { ...fs, writeFile, open }, writeFile, open };
});

// The app's artifacts, run local (docs/contracts/artifacts-app.md): Sandpack's relay as main serves
// it, what the preview's session lets go, the build's guard, and what a version shows. The window
// itself (the view, its partition, a real artifact drawn) is scripts/test-artifacts.mjs, on the Mac.

const app = resolve(__dirname, '../../../..');
const relayDir = join(app, 'resources/sandpack-relay');
const packageRelay = join(app, 'node_modules/static-browser-server/out/preview');
const filesIn = (dir: string, at = ''): string[] => readdirSync(join(dir, at)).flatMap((n) => statSync(join(dir, at, n)).isDirectory() ? filesIn(dir, join(at, n)) : [join(at, n)]);

let relay: Relay;
beforeAll(async () => { relay = await startRelay(); });
afterAll(async () => { await relay.close(); });

// one GET to the relay, with the Host header a browser would send
function get(host: string, path: string): Promise<{ status: number; headers: Record<string, string | string[] | undefined>; body: string }> {
  return new Promise((done, failed) => {
    const r = request({ host: '127.0.0.1', port: relay.port, path, headers: { Host: host } }, (res) => {
      let body = '';
      res.setEncoding('utf8');
      res.on('data', (c) => { body += c; });
      res.on('end', () => done({ status: res.statusCode ?? 0, headers: res.headers, body }));
    });
    r.on('error', failed);
    r.end();
  });
}

describe('ARTIFACTSAPP-1', () => {
  it('artifactsapp_1_the_relay_serves_sandpacks_own_files_on_loopback_for_a_preview_host', async () => {
    expect(relay.origin).toBe(`http://preview.localhost:${relay.port}`);
    const relayPage = await get(`k3j9-preview.localhost:${relay.port}`, '/__csb_relay/');
    expect(relayPage.status).toBe(200);
    expect(relayPage.body).toContain('/__csb_relay/__csb_relay.30yfeq026i8arwjbp19vlgcep8e6tgf.js');
    expect(relayPage.headers['content-type']).toBe('text/html; charset=utf-8');
    const worker = await get(`k3j9-preview.localhost:${relay.port}`, '/__csb_sw.rpfsli6sirrzs1630o3lgzuxwlr2uqc.js');
    expect(worker.status).toBe(200);
    expect(worker.headers['content-type']).toBe('text/javascript; charset=utf-8');
    expect(worker.body).toBe(servedWorker());
  });

  it('artifactsapp_1_the_vendored_relay_is_the_packages_byte_for_byte', () => {
    const ours = filesIn(relayDir).filter((f) => !['LICENSE', 'README.md'].includes(f)).sort();
    expect(ours).toEqual(filesIn(packageRelay).sort());
    for (const f of ours) expect(readFileSync(join(relayDir, f)).equals(readFileSync(join(packageRelay, f))), f).toBe(true);
    expect(readFileSync(join(relayDir, 'LICENSE'), 'utf8')).toContain('Apache License\n                           Version 2.0, January 2004');
    expect(JSON.parse(readFileSync(join(app, 'node_modules/static-browser-server/package.json'), 'utf8')).license).toBe('Apache-2.0');
  });

  it('artifactsapp_1_a_page_shows_its_own_files_a_document_and_a_picture_get_an_index', () => {
    const page = showable({ kind: 'html', title: 'page', files: { 'index.html': { text: '<link rel="stylesheet" href="style.css"><h1>Hello</h1>' }, 'style.css': { text: 'h1 { color: teal }' } } });
    expect(Object.keys(page).sort()).toEqual(['/index.html', '/style.css']);
    expect(page['/style.css'].code).toBe('h1 { color: teal }');
    const doc = showable({ kind: 'markdown', title: 'notes', files: { 'notes.md': { text: '# Notes\n\nSome *words*.' } } });
    expect(Object.keys(doc)).toEqual(['/index.html']);
    expect(doc['/index.html'].code).toContain('<h1>Notes</h1>');
    expect(doc['/index.html'].code).toContain('<em>words</em>');
    const pic = showable({ kind: 'image', title: 'chart', files: { 'chart.png': { base64: Buffer.from([0x89, 0x50, 0x4e, 0x47, 0xff]).toString('base64') } } });
    expect(pic['/index.html'].code).toContain('<img src="./chart.png" alt="chart">');
    expect([...(pic['/chart.png'].code as Uint8Array)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0xff]);
  });
});

describe('ARTIFACTSAPP-2', () => {
  it('artifactsapp_2_the_guard_names_each_hosted_address_and_nodebox', () => {
    for (const [, what] of FORBIDDEN) expect(typeof what).toBe('string');
    const said = forbiddenIn('a.js', 'x("https://2-19-8-sandpack.codesandbox.io/");y("https://cdn.tailwindcss.com/3.4.17");z("https://cdn.jsdelivr.net/npm/monaco-editor")');
    expect(said).toHaveLength(3);
    expect(said[0]).toContain('a.js holds a CodeSandbox host');
    expect(forbiddenIn('b.js', 'https://sandpack-cdn-staging.blazingly.io/dep_tree/x')).toHaveLength(1);
    expect(forbiddenIn('c.js', 'Sustainable Use License\nVersion 1.0')).toHaveLength(1);
    // and the app's own pieces are clean: the relay, served, and Sandpack's static client once taken in
    for (const f of filesIn(relayDir).filter((n) => n !== 'LICENSE')) expect(forbiddenIn(f, readFileSync(join(relayDir, f), 'utf8'))).toEqual([]);
    expect(forbiddenIn('worker', servedWorker())).toEqual([]);
  });

  it('artifactsapp_2_sandpacks_static_default_is_taken_out_and_its_absence_fails_the_build', () => {
    const plugin = localOnly();
    const id = join(app, 'node_modules/@codesandbox/sandpack-client/dist/index-599aeaf7.mjs');
    const source = readFileSync(id, 'utf8');
    expect(forbiddenIn('static', source)).toHaveLength(1);
    const errors: string[] = [];
    const ctx = { error: (m: string) => { errors.push(m); throw new Error(m); } };
    const out = plugin.transform.call(ctx, source, id) as { code: string };
    expect(forbiddenIn('static', out.code)).toEqual([]);
    expect(() => plugin.transform.call(ctx, source.replace('preview.sandpack-static-server', 'elsewhere'), id)).toThrow(/no longer names/);
  });

  it('artifactsapp_2_nodebox_is_our_empty_stub_never_the_package', async () => {
    const pkg = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8'));
    expect(pkg.dependencies['@codesandbox/nodebox']).toBe('file:./stubs/nodebox');
    expect(pkg.overrides['@codesandbox/nodebox']).toBe('$@codesandbox/nodebox');
    const lock = JSON.parse(readFileSync(join(app, 'package-lock.json'), 'utf8'));
    expect(lock.packages['node_modules/@codesandbox/nodebox']).toEqual({ resolved: 'stubs/nodebox', link: true });
    expect(readFileSync(join(app, 'node_modules/@codesandbox/nodebox/package.json'), 'utf8')).toContain('0.0.0-oby-stub');
    // a resolution into the real package fails the build
    const plugin = localOnly();
    const ctx = (id: string) => ({ resolve: async () => ({ id }), error: (m: string) => { throw new Error(m); } });
    await expect(plugin.resolveId.call(ctx('/x/node_modules/@codesandbox/nodebox/build/index.mjs'), '@codesandbox/nodebox', '/y.mjs', {})).rejects.toThrow(/never part of Oby/);
    await expect(plugin.resolveId.call(ctx(join(app, 'stubs/nodebox/index.mjs')), '@codesandbox/nodebox', '/y.mjs', {})).resolves.toEqual({ id: join(app, 'stubs/nodebox/index.mjs') });
  });
});

describe('ARTIFACTSAPP-8', () => {
  it('artifactsapp_8_main_takes_the_owners_unsaved_edits_to_lay_over_the_version_and_nothing_more', () => {
    const id = '3f0c2a1e-1111-4222-8333-944455556666', bounds = { x: 0, y: 0, width: 10, height: 10 };
    expect(previewAsk({ id, version: 1, bounds, edits: { 'style.css': 'h1 { color: crimson }' } })).toEqual({ value: { id, version: 1, bounds, edits: { 'style.css': 'h1 { color: crimson }' } } });
    expect(previewAsk({ id, version: 1, bounds })).toEqual({ value: { id, version: 1, bounds } });
    for (const edits of [null, 'x', ['a'], { 'a.css': 1 }, { '': 'x' }, { ['p'.repeat(1025)]: 'x' }, Object.fromEntries(Array.from({ length: 513 }, (_, i) => [`f${i}`, 'x'])), { 'big.html': 'x'.repeat((17 << 20) + 1) }]) {
      expect(previewAsk({ id, version: 1, bounds, edits }), JSON.stringify(edits).slice(0, 40)).toBeUndefined();
    }
  });

  it('artifactsapp_8_an_edit_named_like_an_inherited_key_changes_nothing', () => {
    const files = Object.assign(noFiles(), { 'index.html': { text: '<h1>One</h1>' }, 'style.css': { text: 'h1 {}' } });
    overlay(files, JSON.parse('{"__proto__": "<h1>x</h1>", "constructor": "x", "toString": "x", "hasOwnProperty": "x", "new.html": "x", "style.css": "h1 { color: red }"}'));
    expect(Object.keys(files).sort()).toEqual(['index.html', 'style.css']);
    expect(files['style.css']).toEqual({ text: 'h1 { color: red }' });
    expect(files['index.html']).toEqual({ text: '<h1>One</h1>' });
    expect(Object.getPrototypeOf(files)).toBeNull();
    // a real file of that name, set as readVersion sets it, is one like any other: laid over
    const named = Object.assign(noFiles(), { 'index.html': { text: 'x' } });
    named['__proto__'] = { text: 'kept' };
    overlay(named, JSON.parse('{"__proto__": "edited"}'));
    expect(Object.keys(named).sort()).toEqual(['__proto__', 'index.html']);
    expect(named['__proto__']).toEqual({ text: 'edited' });
  });

  it('artifactsapp_8_a_file_named_proto_is_a_file', () => {
    const files = noFiles();
    files['index.html'] = { text: '<h1>One</h1>' };
    files['__proto__'] = { text: 'a file' };
    // the exact script main hands executeJavaScript, run as the page would
    let received: { files: Record<string, unknown> } | undefined;
    new Function('window', shownAs({ files, bundler: 'http://127.0.0.1:1' }))({ obyShow: (v: { files: Record<string, unknown> }) => { received = v; } });
    expect(Object.keys(received!.files).sort()).toEqual(['__proto__', 'index.html']);
    expect(Object.hasOwn(received!.files, '__proto__')).toBe(true);
    expect(received!.files['__proto__']).toEqual({ text: 'a file' });
    expect(Object.getPrototypeOf(received!.files)).toBe(Object.prototype);
  });
});

describe('ARTIFACTSAPP-11 and 12, main', () => {
  it('artifactsapp_11_main_saves_obids_export_byte_for_byte_where_the_owner_picked_and_nothing_when_cancelled', async () => {
    const zip = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 1, 2, 3, 255]);
    const written: [string, Uint8Array][] = [];
    const asked: string[] = [];
    const deps = (answer: string | null, r: Response | { status: number; text: string }) => ({
      fetch: async (path: string) => { asked.push(path); return r; },
      ask: async (defaultPath: string) => { asked.push(defaultPath); return answer; },
      write: async (path: string, bytes: Uint8Array) => { written.push([path, bytes]); },
      downloads: '/Users/me/Downloads',
    });
    const ok = () => new Response(zip, { headers: { 'Content-Disposition': 'attachment; filename="Landing-page-v2.zip"' } });
    expect(await exportArtifact('3f0c2a1e-1111-4222-8333-944455556666', 2, 'zip', deps('/tmp/x.zip', ok()))).toEqual({ saved: '/tmp/x.zip' });
    expect(asked).toEqual(['/v1/artifacts/3f0c2a1e-1111-4222-8333-944455556666/export/2?as=zip', '/Users/me/Downloads/Landing-page-v2.zip']);
    expect([...written[0][1]]).toEqual([...zip]);
    expect(await exportArtifact('3f0c2a1e-1111-4222-8333-944455556666', 2, 'zip', deps(null, ok()))).toEqual({ cancelled: true });
    expect(written).toHaveLength(1);
    expect(await exportArtifact('3f0c2a1e-1111-4222-8333-944455556666', 9, 'html', deps('/tmp/y', new Response('{}', { status: 404 })))).toEqual({ error: 'That version isn’t there any more.' });
    expect(written).toHaveLength(1);
    // obid's file name, or one made the same way; never a path
    for (const bad of ['attachment; filename="../../.ssh/x"', 'attachment; filename=".hidden"', null]) expect(nameOf(bad, 'html', 3)).toBe('artifact-v3.html');
  });

  it('artifactsapp_11_a_download_replaces_the_file_whole_or_not_at_all_and_never_writes_through_a_link', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'oby-save-'));
    const to = (path: string) => ({ fetch: async () => new Response(new Uint8Array([1, 2, 3, 4])), ask: async () => path, write: saveFile, downloads: dir });
    try {
      // a link where the owner picked (swapped in after the dialog closed): refused, what it points at untouched
      writeFileSync(join(dir, 'elsewhere'), 'theirs');
      symlinkSync(join(dir, 'elsewhere'), join(dir, 'a.zip'));
      expect(await exportArtifact('3f0c2a1e-1111-4222-8333-944455556666', 1, 'zip', to(join(dir, 'a.zip')))).toEqual({ error: 'Oby couldn’t save it: a.zip is a link.' });
      expect(readFileSync(join(dir, 'elsewhere'), 'utf8')).toBe('theirs');
      // a write that fails partway (the disk fills): the file that was there stays whole, and nothing is left beside it
      writeFileSync(join(dir, 'b.zip'), 'before');
      disk.fails = true;
      try {
        expect(await exportArtifact('3f0c2a1e-1111-4222-8333-944455556666', 1, 'zip', to(join(dir, 'b.zip')))).toEqual({ error: 'Oby couldn’t save it: no space left on device' });
      } finally {
        disk.fails = false;
      }
      expect(readFileSync(join(dir, 'b.zip'), 'utf8')).toBe('before');
      expect(readdirSync(dir).sort()).toEqual(['a.zip', 'b.zip', 'elsewhere']);
      // one that goes: the bytes whole, over the file that was there and as a new one
      for (const name of ['b.zip', 'c.zip']) {
        expect(await exportArtifact('3f0c2a1e-1111-4222-8333-944455556666', 1, 'zip', to(join(dir, name)))).toEqual({ saved: join(dir, name) });
        expect([...readFileSync(join(dir, name))]).toEqual([1, 2, 3, 4]);
      }
      expect(readdirSync(dir).sort()).toEqual(['a.zip', 'b.zip', 'c.zip', 'elsewhere']);
    } finally {
      rmSync(dir, { recursive: true });
    }
  });

  it('artifactsapp_12_an_import_is_sent_as_what_its_bytes_are', () => {
    const enc = (s: string) => new TextEncoder().encode(s);
    expect(importType(new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0]))).toBe('application/zip');
    expect(importType(enc('<!doctype html><h1>x</h1>'))).toBe('text/html');
    expect(importType(enc('\ufeff\n  <html>'))).toBe('text/html');
    expect(importType(enc('# Notes\n<b>bold</b>'))).toBe('text/markdown');
    expect(importType(enc('PK but text'))).toBe('text/markdown');
  });
});

describe('ARTIFACTSAPP-3', () => {
  it('artifactsapp_3_the_relay_answers_only_preview_hosts_on_its_own_port', async () => {
    for (const host of [`127.0.0.1:${relay.port}`, `localhost:${relay.port}`, `evil.com`, `k3j9-preview.localhost:${relay.port + 1}`, `a.b-preview.localhost:${relay.port}`, `preview.localhost.evil.com:${relay.port}`]) {
      expect((await get(host, '/__csb_relay/')).status, host).toBe(404);
    }
    expect(servedHost(`preview.localhost:${relay.port}`, relay.port)).toBe(true);
    expect((await get(`k3j9-preview.localhost:${relay.port}`, '/etc/passwd')).status).toBe(404);
    expect((await get(`k3j9-preview.localhost:${relay.port}`, '/../package.json')).status).toBe(404);
    // every answer carries the relay's CSP, which allows no network but its own origin
    expect((await get(`k3j9-preview.localhost:${relay.port}`, '/__csb_relay/')).headers['content-security-policy']).toBe(RELAY_CSP);
    expect(RELAY_CSP).toContain("connect-src 'self'");
    expect(RELAY_CSP).toContain("default-src 'self'");
  });

  it('artifactsapp_3_the_worker_sends_other_origins_to_the_network_and_puts_the_csp_on_every_answer', () => {
    const worker = servedWorker();
    expect(worker).toContain('if (parsedUrl.origin !== self.location.origin) {\n      return;\n    }');
    expect(worker).toContain(`"Content-Security-Policy": ${JSON.stringify(PREVIEW_CSP)}`);
    // the preview's CSP: its own origin only for anything it fetches, loads or sends a form to
    for (const d of ["default-src 'self'", "connect-src 'self'", "form-action 'self'", "object-src 'none'", "base-uri 'self'"]) expect(PREVIEW_CSP).toContain(d);
    expect(PREVIEW_CSP).not.toMatch(/https?:|\*/);
    // a worker that changed is refused, never served unchanged
    expect(() => servedWorker('self.addEventListener("fetch", () => {});')).toThrow(/isn't there once/);
  });

  it('artifactsapp_3_the_preview_session_lets_go_only_its_host_page_and_the_relay', () => {
    const page = 'file:///Applications/Oby.app/Contents/Resources/app.asar/out/renderer/';
    const yes = [`${page}preview.html`, `${page}assets/preview-B6GBTJO3.js`, `http://preview.localhost:4123/__csb_relay/`, `http://k3j9-preview.localhost:4123/style.css`, 'data:image/png;base64,AA', 'blob:http://k3j9-preview.localhost:4123/1'];
    const no = ['https://example.com/', 'http://127.0.0.1:4123/', 'http://localhost:4123/', 'http://k3j9-preview.localhost:4124/', 'https://k3j9-preview.localhost:4123/', 'ws://k3j9-preview.localhost:4123/', 'file:///etc/hosts', `file:///Applications/Oby.app/Contents/Resources/app.asar/out/main/index.js`, 'http://127.0.0.1:7313/v1/status', 'https://preview.sandpack-static-server.codesandbox.io/', 'chrome://settings', 'not a url'];
    for (const u of yes) expect(allowed(u, 4123, page), u).toBe(true);
    for (const u of no) expect(allowed(u, 4123, page), u).toBe(false);
    // in development only, the dev server too
    expect(allowed('http://localhost:5175/src/artifacts/host.ts', 4123, page, 'http://localhost:5175')).toBe(true);
    expect(allowed('http://localhost:5175/src/artifacts/host.ts', 4123, page)).toBe(false);
  });

  it('artifactsapp_3_the_page_asks_for_an_artifact_a_version_and_a_box_nothing_else', () => {
    const id = '3f0c2a1e-1111-4222-8333-944455556666';
    expect(previewAsk(null)).toEqual({ value: null });
    expect(previewAsk({ id, version: 2, bounds: { x: 10, y: 20, width: 300, height: 400 }, extra: 1 })).toEqual({ value: { id, version: 2, bounds: { x: 10, y: 20, width: 300, height: 400 } } });
    for (const bad of [undefined, 'x', {}, { id: '../../etc', version: 1, bounds: { x: 0, y: 0, width: 1, height: 1 } }, { id, version: 0, bounds: { x: 0, y: 0, width: 1, height: 1 } }, { id, version: 1.5, bounds: { x: 0, y: 0, width: 1, height: 1 } }, { id, version: 1 }, { id, version: 1, bounds: { x: -1, y: 0, width: 1, height: 1 } }, { id: id.toUpperCase(), version: 1, bounds: { x: 0, y: 0, width: 1, height: 1 } }]) {
      expect(previewAsk(bad), JSON.stringify(bad)).toBeUndefined();
    }
  });
});

describe('ARTIFACTSAPP-21', () => {
  const VENDOR = '/__csb_vendor/tailwindcss-browser/index.global.js';
  const BEFORE = '<script>window.tailwind ??= {};</script>';
  const html = (shown: ReturnType<typeof showable>, p = '/index.html') => shown[p].code as string;

  it('artifactsapp_21_a_pages_tailwind_cdn_tag_gets_the_bundled_tailwind', () => {
    const cdns = ['https://cdn.tailwindcss.com', 'https://cdn.tailwindcss.com/3.4.17?plugins=forms', 'https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4', 'https://unpkg.com/@tailwindcss/browser@4.1.0'];
    for (const cdn of cdns) {
      for (const [written, src] of [[`<script src="${cdn}"></script>`, `"${VENDOR}"`], [`<script defer src='${cdn}'></script>`, `'${VENDOR}'`], [`<script src=${cdn}></script>`, VENDOR], [`<SCRIPT Defer SRC="${cdn.toUpperCase()}"></SCRIPT>`, `"${VENDOR}"`]]) {
        const tag = written.replace(/(src=)(["']?)[^"'\s>]+\2/i, `$1${src}`);
        const shown = showable({ kind: 'html', title: 'page', files: { 'index.html': { text: `<head>${written}</head><main class="p-4">x</main>` }, 'about.HTML': { text: written } } });
        expect(html(shown), written).toBe(`<head>${BEFORE}${tag}</head><main class="p-4">x</main>`);
        expect(html(shown, '/about.HTML'), written).toBe(`${BEFORE}${tag}`);
      }
    }
    // left as written: a stylesheet, a link, another CDN, an escaped tag in a <pre>, a comment
    const tag = '<script src="https://cdn.tailwindcss.com"></script>';
    const kept = [
      '<link rel="stylesheet" href="https://cdn.tailwindcss.com/base.css">',
      '<script src="https://cdn.jsdelivr.net/npm/chart.js"></script>',
      '<script src="https://cdn.tailwindcss.com.example.com/x.js"></script>',
      '<pre>&lt;script src="https://cdn.tailwindcss.com"&gt;&lt;/script&gt;</pre>',
      `<!-- ${tag} -->`,
    ];
    const page = showable({ kind: 'html', title: 'page', files: { 'index.html': { text: kept.join('\n') }, 'style.css': { text: tag }, 'notes.txt': { text: tag } } });
    expect(html(page)).toBe(kept.join('\n'));
    expect(html(page, '/style.css')).toBe(tag);
    expect(html(page, '/notes.txt')).toBe(tag);
    // not a page: a React artifact, a document, a canvas and a design are as they were
    const react = showable({ kind: 'html', title: 'r', files: { 'App.tsx': { text: 'export default function App() { return <main />; }' }, 'extra.html': { text: tag } } });
    expect(html(react, '/extra.html')).toBe(tag);
    expect(html(react)).not.toContain(BEFORE);
    const doc = html(showable({ kind: 'markdown', title: 'd', files: { 'notes.md': { text: `# Notes\n\n${tag}\n` } } }));
    expect(doc).toContain(tag);
    expect(doc).not.toContain(BEFORE);
    for (const kind of ['canvas', 'design']) expect(html(showable({ kind, title: 'c', files: { 'index.html': { text: tag } } })), kind).toBe(tag);
  });

  it('artifactsapp_21_a_page_without_a_cdn_tag_is_unchanged', () => {
    const files = {
      'index.html': { text: '<!doctype html><html><head><script src="./app.js"></script><link rel="stylesheet" href="style.css"></head><body><h1>Hi</h1></body></html>' },
      'app.js': { text: 'document.title = "x";' },
      'style.css': { text: 'h1 { color: teal }' },
      'logo.png': { base64: Buffer.from([0x89, 0x50, 0x4e, 0x47, 0xff]).toString('base64') },
    };
    const shown = showable({ kind: 'html', title: 'page', files });
    expect(Object.keys(shown).sort()).toEqual(['/app.js', '/index.html', '/logo.png', '/style.css']);
    for (const p of ['index.html', 'app.js', 'style.css'] as const) expect(shown[`/${p}`].code, p).toBe(files[p].text);
    expect([...(shown['/logo.png'].code as Uint8Array)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0xff]);
  });
});
