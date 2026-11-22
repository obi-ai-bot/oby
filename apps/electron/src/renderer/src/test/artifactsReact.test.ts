import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { request } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterAll, beforeAll, expect, it } from 'vitest';
import { startRelay, type Relay } from '../../../main/sandpackRelay';
import { showable } from '../artifacts/showable';
// @ts-expect-error a plain script
import { forbiddenIn } from '../../../../scripts/local-only.mjs';

// A React artifact in the preview (docs/contracts/artifacts-app.md, ARTIFACTSAPP-13 and 14): what
// the host page compiles and writes, and the vendored set main's relay serves it from. Drawn in the
// real window, from loopback only, in scripts/test-artifacts.mjs.

const app = resolve(__dirname, '../../../..');
const vendored = join(app, 'resources/sandpack-vendor');
const code = (s: { code: string | Uint8Array }) => s.code as string;
/** Every file under `dir`, by its path there, sorted. */
const walk = (dir: string) => readdirSync(dir, { recursive: true, withFileTypes: true }).filter((e) => e.isFile()).map((e) => join(e.parentPath, e.name).slice(dir.length + 1)).sort();
/** A package's own licence file, as the vendor script picks it (`LICENSE`, `license`, `LICENSE.md`, …). */
const licenceOf = (name: string) => { const dir = join(app, 'node_modules', name); return join(dir, readdirSync(dir).filter((f) => /^licen[cs]e/i.test(f)).sort()[0]); };

let relay: Relay;
beforeAll(async () => { relay = await startRelay(); });
afterAll(async () => { await relay.close(); });

function get(host: string, path: string): Promise<{ status: number; type: string | undefined; body: Buffer }> {
  return new Promise((done, failed) => {
    const r = request({ host: '127.0.0.1', port: relay.port, path, headers: { Host: host } }, (res) => {
      const chunks: Buffer[] = [];
      res.on('data', (c: Buffer) => chunks.push(c));
      res.on('end', () => done({ status: res.statusCode ?? 0, type: res.headers['content-type'], body: Buffer.concat(chunks) }));
    });
    r.on('error', failed);
    r.end();
  });
}

const APP = `import { useState } from 'react';
import { Check } from 'lucide-react';
import { Label } from './Label';

export default function App(): JSX.Element {
  const [n] = useState<number>(3);
  return <main className="p-4"><h1>React {n}</h1><Check /><Label text="ok" /></main>;
}
`;
const LABEL = `export function Label({ text }: { text: string }) {
  return <span>{text}</span>;
}
`;

it('artifactsapp_13_a_react_folder_gets_an_index_with_the_vendored_import_map', () => {
  const shown = showable({ kind: 'html', title: 'Counter', files: { 'App.tsx': { text: APP }, 'Label.tsx': { text: LABEL } } });
  expect(Object.keys(shown).sort()).toEqual(['/App.js', '/Label.js', '/index.html']);
  for (const js of ['/App.js', '/Label.js']) {
    expect(code(shown[js]), js).not.toMatch(/<\w|: (string|number|JSX)|<number>/);
  }
  expect(code(shown['/App.js'])).toContain('from "react/jsx-runtime"');
  const index = code(shown['/index.html']);
  const map = JSON.parse(/<script type="importmap">(.*?)<\/script>/s.exec(index)![1]).imports;
  expect(map).toMatchObject({
    react: '/__csb_vendor/react/react.mjs',
    'react/jsx-runtime': '/__csb_vendor/react/jsx-runtime.mjs',
    'react-dom': '/__csb_vendor/react-dom/react-dom.mjs',
    'react-dom/client': '/__csb_vendor/react-dom/client.mjs',
    'lucide-react': '/__csb_vendor/lucide-react/lucide-react.mjs',
    '/Label': '/Label.js',
    '/Label.tsx': '/Label.js',
  });
  expect(index).toContain('<script src="/__csb_vendor/tailwindcss-browser/index.global.js"></script>');
  expect(index).toContain('<div id="root"></div>');
  expect(index).toContain("import('/App.js')");
  // every address on the frame's own origin: a path, never a scheme or a host
  const addresses = [...index.matchAll(/(?:src|href)="([^"]*)"/g)].map((m) => m[1]).concat(Object.values(map) as string[], Object.keys(map).filter((k) => k.startsWith('/')));
  expect(addresses.length).toBeGreaterThan(8);
  for (const a of addresses) expect(a, a).toMatch(/^\/(?!\/)/);
  expect(index).not.toMatch(/https?:|\/\/[a-z]/i);
});

it('artifactsapp_13_a_page_with_an_index_is_shown_as_it_was', () => {
  const files = { 'index.html': { text: '<h1>Mine</h1>' }, 'App.tsx': { text: APP } };
  expect(showable({ kind: 'html', title: 'Mine', files })).toEqual({ '/App.tsx': { code: APP }, '/index.html': { code: '<h1>Mine</h1>' } });
});

it('artifactsapp_13_the_relay_serves_the_vendored_files_on_a_preview_host', async () => {
  const host = `k3j9-preview.localhost:${relay.port}`;
  const react = await get(host, '/__csb_vendor/react/react.mjs');
  expect(react.status).toBe(200);
  expect(react.type).toBe('text/javascript; charset=utf-8');
  expect(react.body.equals(readFileSync(join(vendored, 'react/react.mjs')))).toBe(true);
  for (const f of ['react/jsx-runtime.mjs', 'react-dom/react-dom.mjs', 'react-dom/client.mjs', 'lucide-react/lucide-react.mjs', 'tailwindcss-browser/index.global.js']) {
    expect((await get(host, `/__csb_vendor/${f}`)).body.equals(readFileSync(join(vendored, f))), f).toBe(true);
  }
  for (const path of ['/__csb_vendor/react/LICENSE', '/__csb_vendor/README.md', '/__csb_vendor/three/three.mjs', '/__csb_vendor/react/../react/react.mjs']) {
    expect((await get(host, path)).status, path).toBe(404);
  }
  expect((await get(`localhost:${relay.port}`, '/__csb_vendor/react/react.mjs')).status).toBe(404);
});

// ARTIFACTSAPP-26: every module of the set, found by walking it, and nothing else of it
it('artifactsapp_26_the_relay_serves_every_vendored_module_and_nothing_else', async () => {
  const host = `k3j9-preview.localhost:${relay.port}`;
  const modules = walk(vendored).filter((f) => /\.m?js$/.test(f));
  expect(modules).toEqual(expect.arrayContaining(['recharts/recharts.mjs', 'clsx/clsx.mjs', 'tailwind-merge/tailwind-merge.mjs', 'class-variance-authority/class-variance-authority.mjs']));
  for (const f of modules) {
    const served = await get(host, `/__csb_vendor/${f}`);
    expect(served.status, f).toBe(200);
    expect(served.type, f).toBe('text/javascript; charset=utf-8');
    expect(served.body.equals(readFileSync(join(vendored, f))), f).toBe(true);
  }
  for (const path of ['/__csb_vendor/recharts/LICENSE', '/__csb_vendor/recharts/THIRD_PARTY_LICENSES', '/__csb_vendor/shadcn/LICENSE', '/__csb_vendor/shadcn/THIRD_PARTY_LICENSES', '/__csb_vendor/README.md', '/__csb_vendor/three/three.mjs', '/__csb_vendor/react/../react/react.mjs']) {
    expect((await get(host, path)).status, path).toBe(404);
  }
  for (const f of modules) expect((await get(`localhost:${relay.port}`, `/__csb_vendor/${f}`)).status, f).toBe(404);
});

// ARTIFACTSAPP-26: recharts and the trio in the map, each on the frame's own origin
it('artifactsapp_26_a_react_artifacts_map_has_recharts_and_the_trio', () => {
  const index = code(showable({ kind: 'html', title: 'Chart', files: { 'App.tsx': { text: APP }, 'Label.tsx': { text: LABEL } } })['/index.html']);
  const map = JSON.parse(/<script type="importmap">(.*?)<\/script>/s.exec(index)![1]).imports;
  expect(map).toMatchObject({
    recharts: '/__csb_vendor/recharts/recharts.mjs',
    clsx: '/__csb_vendor/clsx/clsx.mjs',
    'tailwind-merge': '/__csb_vendor/tailwind-merge/tailwind-merge.mjs',
    'class-variance-authority': '/__csb_vendor/class-variance-authority/class-variance-authority.mjs',
  });
  for (const a of Object.values(map) as string[]) expect(a, a).toMatch(/^\/(?!\/)/);
});

// ARTIFACTSAPP-27: shadcn's components, as LibreChat ships them, built into sandpack-vendor/shadcn/
const SHADCN = ['accordion', 'alert', 'alert-dialog', 'avatar', 'badge', 'breadcrumb', 'button', 'card', 'checkbox', 'collapsible', 'dialog', 'dropdown-menu', 'input', 'label', 'menubar', 'navigation-menu', 'pagination', 'popover', 'progress', 'radio-group', 'select', 'separator', 'skeleton', 'slider', 'switch', 'table', 'tabs', 'textarea', 'toast', 'toaster', 'toggle', 'toggle-group', 'tooltip', 'use-toast'];
const EXTERNAL = ['react', 'react-dom', 'react/jsx-runtime', 'lucide-react', 'clsx', 'tailwind-merge', 'class-variance-authority'];
const shadcnSrc = join(app, 'resources/shadcn-src');
const shadcnOut = join(vendored, 'shadcn');
/** Every specifier a module imports or re-exports, statically or by a literal `import()`. */
const importsOf = (code: string) => [...code.matchAll(/(?<![\w$.])(?:import|export)\s*(?:[^"';]*?\bfrom\s*)?["']([^"']+)["']|(?<![\w$.])import\(\s*["']([^"']+)["']\s*\)/g)].map((m) => m[1] ?? m[2]);

it('artifactsapp_27_a_react_artifacts_map_has_shadcn', () => {
  const mapOf = (files: Record<string, { text: string }>) => JSON.parse(/<script type="importmap">(.*?)<\/script>/s.exec(code(showable({ kind: 'html', title: 'Board', files })['/index.html']))![1]).imports;
  const map = mapOf({ 'App.tsx': { text: APP }, 'Label.tsx': { text: LABEL } });
  for (const name of SHADCN) {
    expect(map[`/components/ui/${name}`], name).toBe(`/__csb_vendor/shadcn/ui/${name}.mjs`);
    expect(map[`@/components/ui/${name}`], name).toBe(`/__csb_vendor/shadcn/ui/${name}.mjs`);
  }
  expect(map['/lib/utils']).toBe('/__csb_vendor/shadcn/lib/utils.mjs');
  expect(map['@/lib/utils']).toBe('/__csb_vendor/shadcn/lib/utils.mjs');
  for (const a of Object.values(map) as string[]) expect(a, a).toMatch(/^\/(?!\/)/);
  // the names the map has are the files the build wrote
  const named = Object.keys(map).filter((k) => k.startsWith('/components/ui/')).map((k) => k.slice('/components/ui/'.length)).sort();
  expect(named).toEqual([...SHADCN].sort());
  expect(named).toEqual(readdirSync(join(shadcnOut, 'ui')).filter((f) => f.endsWith('.mjs')).map((f) => f.slice(0, -4)).sort());
  // the artifact's own file wins, under both spellings
  const own = mapOf({ 'App.tsx': { text: APP }, 'Label.tsx': { text: LABEL }, 'components/ui/button.tsx': { text: 'export function Button() {\n  return <button>mine</button>;\n}\n' } });
  expect(own['/components/ui/button']).toBe('/components/ui/button.js');
  expect(own['@/components/ui/button']).toBe('/components/ui/button.js');
  expect(own['@/components/ui/card']).toBe('/__csb_vendor/shadcn/ui/card.mjs');
  // `@/components/ui/<name>` is in the map, so the import check lets it through
  const board = code(showable({ kind: 'html', title: 'Board', files: { 'App.tsx': { text: "import { Card } from '@/components/ui/card';\n\nexport default function App() {\n  return <Card>ok</Card>;\n}\n" } } })['/index.html']);
  expect(board).toContain("import('/App.js')");
  expect(board).not.toContain('<pre data-oby-error>');
});

it('artifactsapp_27_shadcns_sources_name_only_what_resolves', () => {
  expect(walk(shadcnSrc)).toEqual(['LICENSE', ...SHADCN.map((n) => `components/ui/${n}.tsx`).sort(), 'lib/utils.ts'].sort());
  const radix = Object.keys(JSON.parse(readFileSync(join(app, 'package.json'), 'utf8')).devDependencies).filter((p) => p.startsWith('@radix-ui/react-'));
  const lucide = readFileSync(join(vendored, 'lucide-react/lucide-react.mjs'), 'utf8');
  const icons = new Set([...lucide.slice(lucide.lastIndexOf('export{')).matchAll(/ as ([\w$]+)/g)].map((m) => m[1]));
  const licence = readFileSync(join(shadcnSrc, 'LICENSE'), 'utf8');
  const mit = licence.slice(licence.indexOf('Permission is hereby granted'), licence.indexOf('SOFTWARE.\n') + 'SOFTWARE.'.length);
  expect(mit).toMatch(/^Permission is hereby granted[^]*THE SOFTWARE IS PROVIDED "AS IS"[^]*DEALINGS IN THE\nSOFTWARE\.$/);
  for (const f of walk(shadcnSrc).filter((f) => f !== 'LICENSE')) {
    const text = readFileSync(join(shadcnSrc, f), 'utf8');
    const top = /^\/\*([^]*?)\*\//.exec(text)?.[1] ?? '';
    for (const line of ['From LibreChat (`danny-avila/LibreChat` at `4700c44`, `packages/data-provider/src/artifacts.ts`, `', 'Copyright (c) 2026 LibreChat', 'Copyright (c) 2023 shadcn', mit]) expect(top, f).toContain(line);
    for (const s of importsOf(text)) {
      if (s.startsWith('.')) {
        const at = resolve(join(shadcnSrc, f, '..'), s);
        expect(at.startsWith(shadcnSrc + '/') && ['.ts', '.tsx'].some((x) => existsSync(at + x)), `${f}: ${s}`).toBe(true);
      } else expect([...EXTERNAL, ...radix], `${f}: ${s}`).toContain(s);
    }
    for (const m of text.matchAll(/import\s*\{([^}]*)\}\s*from\s*["']lucide-react["']/g)) {
      for (const name of m[1].split(',').map((n) => n.trim().split(/\s+as\s+/)[0]).filter(Boolean)) expect(icons.has(name), `${f}: lucide's ${name}`).toBe(true);
    }
  }
});

// ARTIFACTSAPP-28: a bare import outside the set is named before anything runs, with what is there
const HAS = 'It has: react, react-dom, lucide-react, recharts, clsx, tailwind-merge, class-variance-authority and shadcn/ui at /components/ui/&#60;name&#62;.';
const refusal = (files: Record<string, { text: string }>) => {
  const shown = showable({ kind: 'html', title: 'Scene', files });
  expect(Object.keys(shown)).toEqual(['/index.html']);
  const index = code(shown['/index.html']);
  expect(index).not.toContain('<script');
  return /<pre data-oby-error>(.*)<\/pre>/s.exec(index)![1];
};
it('artifactsapp_28_an_import_outside_the_set_says_what_is_there', () => {
  const three = "import * as THREE from 'three';\n\nexport default function App() {\n  return <p>{String(THREE)}</p>;\n}\n";
  expect(refusal({ 'App.tsx': { text: three } })).toBe(`This preview has no \`three\`. ${HAS}`);
  const scene = "import * as THREE from 'three';\n\nexport function Scene() {\n  return <p>{String(THREE)}</p>;\n}\n";
  expect(refusal({ 'App.tsx': { text: "import { Scene } from './Scene';\n\nexport default function App() {\n  return <Scene />;\n}\n" }, 'Scene.tsx': { text: scene } })).toBe(`This preview has no \`three\`. ${HAS}`);
  const two = "import * as THREE from 'three';\nimport { format } from 'date-fns';\nimport { Vector3 } from 'three';\n\nexport default function App() {\n  return <p>{format(new Date(), 'y')}{String(THREE)}{String(Vector3)}</p>;\n}\n";
  expect(refusal({ 'App.tsx': { text: two } })).toBe(`This preview has no \`three\`, \`date-fns\`. ${HAS}`);
  const lazy = "export default function App() {\n  import('three');\n  return <p />;\n}\n";
  expect(refusal({ 'App.tsx': { text: lazy } })).toBe(`This preview has no \`three\`. ${HAS}`);
  // what the set and its own files hold draws as before, sucrase's react/jsx-runtime import with it
  const fine = showable({ kind: 'html', title: 'Fine', files: {
    'App.tsx': { text: "import { useState } from 'react';\nimport { LineChart } from 'recharts';\nimport { clsx } from 'clsx';\nimport { Label } from './Label';\nimport { cn } from '/lib/utils';\nimport { X } from '/components/x';\n\nexport default function App() {\n  const [n] = useState(1);\n  return <main className={cn(clsx('p-4'))}><LineChart width={n} height={n} data={[]} /><Label text=\"ok\" /><X /></main>;\n}\n" },
    'Label.tsx': { text: LABEL },
    'lib/utils.ts': { text: "import { clsx, type ClassValue } from 'clsx';\nimport { twMerge } from 'tailwind-merge';\n\nexport function cn(...inputs: ClassValue[]) {\n  return twMerge(clsx(inputs));\n}\n" },
    'components/x.tsx': { text: 'export function X() {\n  return <b>x</b>;\n}\n' },
  } });
  expect(code(fine['/App.js'])).toContain('from "react/jsx-runtime"');
  const index = code(fine['/index.html']);
  expect(index).toContain('<div id="root"></div>');
  expect(index).toContain("import('/App.js')");
  expect(index).not.toContain('<pre data-oby-error>');
});

// ARTIFACTSAPP-23: the bridge, served as it is on a preview host only
it('artifactsapp_23_the_relay_serves_the_bridge', async () => {
  const bridge = await get(`k3j9-preview.localhost:${relay.port}`, '/__csb_oby/bridge.js');
  expect(bridge.status).toBe(200);
  expect(bridge.type).toBe('text/javascript; charset=utf-8');
  expect(bridge.body.equals(readFileSync(join(app, 'resources/preview-bridge/bridge.js')))).toBe(true);
  expect((await get(`localhost:${relay.port}`, '/__csb_oby/bridge.js')).status).toBe(404);
  expect(forbiddenIn('bridge.js', bridge.body.toString())).toEqual([]);
});

// one rebuild of the set into a scratch folder, for every test that compares it (it takes minutes)
let rebuild: { scratch: string; printed: Record<string, string[]> } | undefined;
const rebuilt = () => {
  if (rebuild) return rebuild;
  const scratch = mkdtempSync(join(tmpdir(), 'oby-vendor-'));
  rebuild = { scratch, printed: JSON.parse(execFileSync(process.execPath, [join(app, 'scripts/vendor-sandpack.mjs'), scratch], { cwd: app, stdio: 'pipe', encoding: 'utf8' })) };
  return rebuild;
};
afterAll(() => { if (rebuild) rmSync(rebuild.scratch, { recursive: true, force: true }); });

it('artifactsapp_13_each_vendored_file_is_its_pinned_packages_build_with_its_licence_beside_it', async () => {
  const { scratch, printed } = rebuilt();
  const committed = walk(vendored);
  expect(committed.filter((f) => f !== 'README.md')).toEqual(walk(scratch));
  for (const f of committed.filter((f) => f !== 'README.md')) expect(readFileSync(join(scratch, f)).equals(readFileSync(join(vendored, f))), f).toBe(true);
  // each built folder's THIRD_PARTY_LICENSES: a section per package the build took in, as the run said
  expect(Object.keys(printed).sort()).toEqual(['class-variance-authority', 'clsx', 'lucide-react', 'react', 'react-dom', 'recharts', 'shadcn', 'tailwind-merge']);
  for (const [folder, packages] of Object.entries(printed)) {
    const sections = readFileSync(join(vendored, folder, 'THIRD_PARTY_LICENSES'), 'utf8').split(/^--- (.+) \(.*\) ---$/m);
    expect(sections.filter((_, i) => i % 2 === 1), folder).toEqual(packages);
    for (const text of sections.filter((_, i) => i > 0 && i % 2 === 0)) expect(text.trim().length, folder).toBeGreaterThan(0);
  }
  const charts = printed.recharts.map((p) => p.slice(0, p.lastIndexOf('@')));
  expect(charts).toEqual(expect.arrayContaining(['recharts', 'react-is', 'react-redux', '@reduxjs/toolkit', 'immer']));
  expect(charts.some((n) => n.startsWith('d3-') || n === 'victory-vendor'), charts.join(', ')).toBe(true);
  expect(charts).not.toContain('react');
  expect(charts).not.toContain('react-dom');
  expect(printed['class-variance-authority'].map((p) => p.slice(0, p.lastIndexOf('@')))).toContain('clsx');
  const lock = JSON.parse(readFileSync(join(app, 'package-lock.json'), 'utf8')).packages;
  const readme = readFileSync(join(vendored, 'README.md'), 'utf8');
  for (const [folder, name] of [['react', 'react'], ['react-dom', 'react-dom'], ['scheduler', 'scheduler'], ['lucide-react', 'lucide-react'], ['tailwindcss-browser', '@tailwindcss/browser'], ['recharts', 'recharts'], ['clsx', 'clsx'], ['tailwind-merge', 'tailwind-merge'], ['class-variance-authority', 'class-variance-authority']]) {
    expect(readFileSync(join(vendored, folder, 'LICENSE')).equals(readFileSync(licenceOf(name))), folder).toBe(true);
    expect(readme, name).toMatch(new RegExp(`\\| ${name.replace('/', '\\/')}[ |(].*\\| ${lock[`node_modules/${name}`].version.replace(/\./g, '\\.')} \\|`));
  }
  for (const f of walk(vendored).filter((f) => /\.m?js$/.test(f))) expect(forbiddenIn(f, readFileSync(join(vendored, f), 'utf8')), f).toEqual([]);
  expect(readFileSync(join(vendored, 'lucide-react/lucide-react.mjs'), 'utf8')).not.toMatch(/dynamicIconImports|DynamicIcon/);
  // an ES package's own default export is kept: `import clsx from 'clsx'` resolves, as it does on npm
  const clsx = await import(/* @vite-ignore */ pathToFileURL(join(vendored, 'clsx/clsx.mjs')).href);
  expect(clsx.default, 'clsx default').toBe(clsx.clsx);
}, 180_000);

it('artifactsapp_27_each_shadcn_file_is_its_builds_output_with_its_licences_beside_it', () => {
  const committed = walk(shadcnOut);
  const { scratch, printed } = rebuilt();
  expect(walk(join(scratch, 'shadcn'))).toEqual(committed);
  for (const f of committed) expect(readFileSync(join(scratch, 'shadcn', f)).equals(readFileSync(join(shadcnOut, f))), f).toBe(true);
  expect(committed).toEqual(expect.arrayContaining(['LICENSE', 'THIRD_PARTY_LICENSES', 'lib/utils.mjs', ...SHADCN.map((n) => `ui/${n}.mjs`)]));
  const licence = readFileSync(join(shadcnOut, 'LICENSE'));
  expect(licence.equals(readFileSync(join(shadcnSrc, 'LICENSE')))).toBe(true);
  for (const line of ['Copyright (c) 2026 LibreChat', 'Copyright (c) 2023 shadcn']) expect(licence.toString()).toContain(line);
  // every Radix package at its lock version, and none of what stays outside
  const lock = JSON.parse(readFileSync(join(app, 'package-lock.json'), 'utf8')).packages;
  const radix = Object.keys(JSON.parse(readFileSync(join(app, 'package.json'), 'utf8')).devDependencies).filter((p) => p.startsWith('@radix-ui/react-'));
  expect(radix).toHaveLength(23);
  for (const p of radix) expect(printed.shadcn, p).toContain(`${p}@${lock[`node_modules/${p}`].version}`);
  const names = printed.shadcn.map((p) => p.slice(0, p.lastIndexOf('@')));
  for (const p of ['react', 'react-dom', 'lucide-react', 'clsx', 'tailwind-merge', 'class-variance-authority']) expect(names).not.toContain(p);
  const sections = readFileSync(join(shadcnOut, 'THIRD_PARTY_LICENSES'), 'utf8').split(/^--- (.+) \(.*\) ---$/m);
  expect(sections.filter((_, i) => i % 2 === 1)).toEqual(printed.shadcn);
  // a bare import is one of the seven the page's map sends to its one copy; any other is a file here
  const modules = committed.filter((f) => f.endsWith('.mjs'));
  const relative = new Map<string, string[]>();
  for (const f of modules) {
    const text = readFileSync(join(shadcnOut, f), 'utf8');
    expect(forbiddenIn(f, text), f).toEqual([]);
    const specs = importsOf(text);
    relative.set(f, []);
    for (const s of specs) {
      if (!s.startsWith('.')) { expect(EXTERNAL, `${f}: ${s}`).toContain(s); continue; }
      const at = join(f, '..', s);
      expect(modules, `${f}: ${s}`).toContain(at);
      relative.get(f)!.push(at);
    }
  }
  // one DismissableLayer, in a shared chunk that a dialog and a popover both reach
  const layer = modules.filter((f) => readFileSync(join(shadcnOut, f), 'utf8').includes('DismissableLayer'));
  expect(layer).toHaveLength(1);
  expect(layer[0]).toMatch(/^chunks\//);
  const reached = (from: string) => { const seen = new Set<string>(); const next = [from]; while (next.length) for (const g of relative.get(next.pop()!) ?? []) if (!seen.has(g)) { seen.add(g); next.push(g); } return seen; };
  for (const f of ['ui/dialog.mjs', 'ui/popover.mjs']) expect(reached(f).has(layer[0]), f).toBe(true);
}, 180_000);

it('artifactsapp_14_a_compile_error_shows_its_message_and_runs_nothing', () => {
  const shown = showable({ kind: 'html', title: 'Broken', files: { 'App.tsx': { text: 'export default function App() { return <h1>open <b>never closed</h1>; }' } } });
  expect(Object.keys(shown)).toEqual(['/index.html']);
  const index = code(shown['/index.html']);
  // marked as the page's error (ARTIFACTSAPP-22), for the bridge to say in drawn's place
  const pre = /<pre data-oby-error>(.*)<\/pre>/s.exec(index)![1];
  expect(pre.length).toBeGreaterThan(5);
  expect(pre).not.toContain('<');
  expect(index).not.toContain('<script');
  expect(index).not.toContain('<b>');
  // a page that compiled: React's errors are the bridge's to tell, the page left as it drew; it says
  // when it rendered on a timer, never a frame, and a failed import's <pre> is marked too
  const mount = /<script type="module">(.*)<\/script>/s.exec(code(showable({ kind: 'html', title: 'Card', files: { 'App.tsx': { text: APP } } })['/index.html']))![1];
  for (const gone of ['window.onerror', 'onUncaughtError', 'requestAnimationFrame', 'replaceChildren(pre); drawn', 'oby:rendered']) expect(mount, gone).not.toContain(gone);
  expect(mount).toMatch(/^[^]*?window\.__obyReact = true;[^]*import\('\/App\.js'\)/);
  expect(mount).toContain("setTimeout(() => { window.__obyRendered?.(); }, 0)");
  expect(mount).toContain("pre.setAttribute('data-oby-error', '')");
});

it('artifactsapp_14_the_error_text_is_inset', () => {
  const inset = /padding:16px;margin:0;white-space:pre-wrap/;
  const broken = code(showable({ kind: 'html', title: 'Broken', files: { 'App.tsx': { text: 'export default function App() { return <h1>; }' } } })['/index.html']);
  expect(/<style>pre\{([^}]*)\}<\/style>/.exec(broken)![1]).toMatch(inset);
  const index = code(showable({ kind: 'html', title: 'Card', files: { 'App.tsx': { text: APP } } })['/index.html']);
  expect(/pre\.style\.cssText = "([^"]*)"/.exec(index)![1]).toMatch(inset);
  expect(/<style>(.*?)<\/style>/s.exec(index)![1]).not.toMatch(/pre/);
});

// ARTIFACTSAPP-21: agents are told what the preview imports (ARTIFACTS-27), in one sentence of
// obid's; every bare package VENDORED maps is named in it, so the set can't grow without the line
it('artifactsapp_21_agents_are_told_every_package_the_preview_has', () => {
  const vendored = /const VENDORED[^=]*= \{([^}]*)\}/.exec(readFileSync(join(app, 'src/renderer/src/artifacts/react.ts'), 'utf8'))![1];
  const bare = [...vendored.matchAll(/^\s*'?([\w@./-]+)'?:/gm)].map((m) => m[1]).filter((k) => !k.includes('/'));
  expect(bare).toContain('react');
  const sentence = /macro_rules! preview_imports \{\s*\(\) => \{\s*"((?:[^"\\]|\\.)*)"/.exec(readFileSync(join(app, '../../core/crates/obid/src/brain/tools.rs'), 'utf8'))?.[1];
  expect(sentence).toBeTruthy();
  for (const name of bare) expect(sentence, name).toMatch(new RegExp(`(?<![\\w@/-])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\w/-])`));
  // shadcn's components (ARTIFACTSAPP-27) by their one phrase, in the sentence and the refusal line
  expect(sentence).toContain('shadcn/ui at /components/ui/<name>');
  expect(/const missing = .*/.exec(readFileSync(join(app, 'src/renderer/src/artifacts/react.ts'), 'utf8'))![0]).toContain('shadcn/ui at /components/ui/<name>');
});
