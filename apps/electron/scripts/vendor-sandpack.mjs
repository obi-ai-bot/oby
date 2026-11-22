// The React artifact's vendored set (docs/contracts/artifacts-app.md, ARTIFACTSAPP-13): React,
// react-dom, lucide-react and Tailwind's browser build, from the installed packages, written to
// resources/sandpack-vendor/ (or the folder given) for main's relay to serve at /__csb_vendor/.
// React ships CommonJS only, so each .mjs is built with Vite's own build() in library mode: ES,
// minified, NODE_ENV production. Every file but react.mjs keeps `react` external (client.mjs
// `react-dom` too), so a preview's frame has one React. The same packages give the same bytes.
//   npm run vendor:sandpack [-- <folder>]
import { copyFileSync, mkdirSync, writeFileSync } from 'node:fs';
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
];
/** Each package's LICENSE, by the folder it goes in. */
const LICENSES = [
  ['react', 'react'],
  ['react-dom', 'react-dom'],
  ['scheduler', 'scheduler'],
  ['lucide-react', 'lucide-react'],
  ['tailwindcss-browser', '@tailwindcss/browser'],
];

const packageDir = (name) => dirname(require.resolve(`${name}/package.json`));

/** The entry for `module`: its named exports, and React's CommonJS default beside them. */
function entry(module) {
  if (module === 'lucide-react') return "export * from 'lucide-react';\n";
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
    plugins: [{ name: 'oby-vendor-entry', enforce: 'pre', resolveId: (s) => (s === join(app, 'vendor-entry') ? id : null), load: (i) => (i === id ? entry(module) : null) }],
    build: {
      write: false,
      minify: 'esbuild',
      lib: { entry: 'vendor-entry', formats: ['es'], fileName: () => 'out.mjs' },
      rollupOptions: { external },
    },
  });
  const [{ output }] = Array.isArray(out) ? out : [out];
  // Vite's lib mode keeps an ES build's whitespace: its own esbuild takes it out
  return (await transformWithEsbuild(output.find((o) => o.type === 'chunk' && o.isEntry).code, 'out.mjs', { minify: true, format: 'esm' })).code;
}

export async function vendor(into = join(app, 'resources', 'sandpack-vendor')) {
  for (const [folder, file, module, external] of BUILT) {
    mkdirSync(join(into, folder), { recursive: true });
    writeFileSync(join(into, folder, file), await built(module, external));
  }
  mkdirSync(join(into, 'tailwindcss-browser'), { recursive: true });
  copyFileSync(require.resolve('@tailwindcss/browser'), join(into, 'tailwindcss-browser', 'index.global.js'));
  for (const [folder, name] of LICENSES) {
    mkdirSync(join(into, folder), { recursive: true });
    copyFileSync(join(packageDir(name), 'LICENSE'), join(into, folder, 'LICENSE'));
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await vendor(process.argv[2] && resolve(process.argv[2]));
