// The React artifact's vendored set (docs/contracts/artifacts-app.md, ARTIFACTSAPP-13 and 26): React,
// react-dom, lucide-react, recharts, clsx, tailwind-merge, class-variance-authority and Tailwind's
// browser build, from the installed packages, written to resources/sandpack-vendor/ (or the folder
// given) for main's relay to serve at /__csb_vendor/. React ships CommonJS only, so each .mjs is
// built with Vite's own build() in library mode: ES, minified, NODE_ENV production. Every file but
// react.mjs keeps `react` external (client.mjs and recharts.mjs `react-dom` too), so a preview's
// frame has one React. Beside each built folder's files go its package's LICENSE and
// THIRD_PARTY_LICENSES, every package the build took in with its licence, read from the build's
// module ids; those lists are printed as one line of JSON. The same packages give the same bytes.
//   npm run vendor:sandpack [-- <folder>]
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build, transformWithEsbuild } from 'vite';

// React's names are read from its production build, whatever NODE_ENV the caller runs in (vitest's `test`)
process.env.NODE_ENV = 'production';
const app = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(join(app, 'package.json'));

/** Each built file: its folder and name, the module it is, what stays external. */
const BUILT = [
  ['react', 'react.mjs', 'react', []],
  ['react', 'jsx-runtime.mjs', 'react/jsx-runtime', ['react']],
  ['react-dom', 'react-dom.mjs', 'react-dom', ['react']],
  ['react-dom', 'client.mjs', 'react-dom/client', ['react', 'react-dom']],
  ['lucide-react', 'lucide-react.mjs', 'lucide-react', ['react']],
  ['recharts', 'recharts.mjs', 'recharts', ['react', 'react-dom']],
  ['clsx', 'clsx.mjs', 'clsx', ['react']],
  ['tailwind-merge', 'tailwind-merge.mjs', 'tailwind-merge', ['react']],
  ['class-variance-authority', 'class-variance-authority.mjs', 'class-variance-authority', ['react']],
];
/** Each package's licence file, by the folder its LICENSE goes in. */
const LICENSES = [
  ['react', 'react'],
  ['react-dom', 'react-dom'],
  ['scheduler', 'scheduler'],
  ['lucide-react', 'lucide-react'],
  ['tailwindcss-browser', '@tailwindcss/browser'],
  ['recharts', 'recharts'],
  ['clsx', 'clsx'],
  ['tailwind-merge', 'tailwind-merge'],
  ['class-variance-authority', 'class-variance-authority'],
];

// ponytail: npm puts every devDependency at the top of node_modules; a package's exports may hide its package.json from require
const packageDir = (name) => join(app, 'node_modules', name);

/** Whether a package ships ES modules, by its own package.json: `module`, `"type": "module"` or an `import` condition. */
function isEs(name) {
  const pkg = JSON.parse(readFileSync(join(packageDir(name), 'package.json'), 'utf8'));
  const imports = (e) => !!e && typeof e === 'object' && Object.entries(e).some(([k, v]) => k === 'import' || imports(v));
  return !!pkg.module || pkg.type === 'module' || imports(pkg.exports);
}

/** The entry for `module`: an ES package's own exports (its default too, when `exports` has one), or a CommonJS one's named exports with its default beside them. */
async function entry(module, exports) {
  if (isEs(module.split('/').slice(0, module.startsWith('@') ? 2 : 1).join('/'))) {
    // `export *` leaves out a default: kept as the package has it, so `import clsx from 'clsx'` resolves
    return `export * from '${module}';\n${(await exports()).includes('default') ? `export { default } from '${module}';\n` : ''}`;
  }
  const names = Object.keys(require(module)).filter((k) => /^[A-Za-z_$][\w$]*$/.test(k) && k !== 'default').sort();
  return `import m from '${module}';\nexport default m;\nexport const { ${names.join(', ')} } = m;\n`;
}

async function built(module, external) {
  const id = `\0vendor:${module}`;
  const out = await build({
    configFile: false,
    root: app,
    logLevel: 'silent',
    mode: 'production',
    define: { 'process.env.NODE_ENV': '"production"' },
    // lib mode resolves its entry from the root: that path is this module's entry, written here
    plugins: [{ name: 'oby-vendor-entry', enforce: 'pre', resolveId: (s) => (s === join(app, 'vendor-entry') ? id : null), async load(i) {
      if (i !== id) return null;
      // the ES module's own exports, as this build resolves and reads it
      return entry(module, async () => (await this.load(await this.resolve(module, i))).exports ?? []);
    } }],
    build: {
      write: false,
      minify: 'esbuild',
      lib: { entry: 'vendor-entry', formats: ['es'], fileName: () => 'out.mjs' },
      rollupOptions: { external },
    },
  });
  const [{ output }] = Array.isArray(out) ? out : [out];
  const chunk = output.find((o) => o.type === 'chunk' && o.isEntry);
  // Vite's lib mode keeps an ES build's whitespace: its own esbuild takes it out
  return { code: (await transformWithEsbuild(chunk.code, 'out.mjs', { minify: true, format: 'esm' })).code, ids: chunk.moduleIds };
}

/** A module's package: the folder of its nearest package.json with a name and a version (none off node_modules). */
function packageOf(id) {
  const path = id.replace(/^\0/, '').split('?')[0];
  if (!path.includes('/node_modules/')) return null;
  for (let dir = dirname(path); dir !== dirname(dir); dir = dirname(dir)) {
    const file = join(dir, 'package.json');
    if (!existsSync(file)) continue;
    const { name, version, license } = JSON.parse(readFileSync(file, 'utf8'));
    if (name && version) return { name, version, license, dir };
  }
  return null;
}

/** A package folder's licence file (`LICENSE`, `license`, `LICENSE.md`, …): the first by name. */
function licenceIn(dir, name) {
  const file = readdirSync(dir).filter((f) => /^licen[cs]e/i.test(f)).sort()[0];
  if (!file) throw new Error(`${name} has no licence file in ${dir}: it can't be vendored without one.`);
  return join(dir, file);
}

/** THIRD_PARTY_LICENSES: a section per package, by name, each with its own licence file's text. */
function thirdParty(packages) {
  return packages.map(({ name, version, license, dir }) => `--- ${name}@${version} (${license}) ---\n\n${readFileSync(licenceIn(dir, `${name}@${version}`), 'utf8').trimEnd()}\n\n`).join('');
}

export async function vendor(into = join(app, 'resources', 'sandpack-vendor')) {
  const taken = {};
  for (const [folder, file, module, external] of BUILT) {
    mkdirSync(join(into, folder), { recursive: true });
    const { code, ids } = await built(module, external);
    writeFileSync(join(into, folder, file), code);
    taken[folder] ??= new Map();
    for (const p of ids.map(packageOf)) if (p) taken[folder].set(`${p.name}@${p.version}`, p);
  }
  const listed = {};
  for (const [folder, packages] of Object.entries(taken)) {
    const sorted = [...packages.values()].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    writeFileSync(join(into, folder, 'THIRD_PARTY_LICENSES'), thirdParty(sorted));
    listed[folder] = sorted.map((p) => `${p.name}@${p.version}`);
  }
  mkdirSync(join(into, 'tailwindcss-browser'), { recursive: true });
  copyFileSync(require.resolve('@tailwindcss/browser'), join(into, 'tailwindcss-browser', 'index.global.js'));
  for (const [folder, name] of LICENSES) {
    mkdirSync(join(into, folder), { recursive: true });
    copyFileSync(licenceIn(packageDir(name), name), join(into, folder, 'LICENSE'));
  }
  return listed;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) console.log(JSON.stringify(await vendor(process.argv[2] && resolve(process.argv[2]))));
