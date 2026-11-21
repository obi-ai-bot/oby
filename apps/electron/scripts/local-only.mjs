// What the built app may never hold (docs/contracts/artifacts-app.md, ARTIFACTSAPP-2): a hosted
// bundler's or CDN's address, which would send an artifact off this Mac, or Nodebox's code, whose
// licence (the Sustainable Use License) isn't open source. A Vite plugin for every part of the build
// (electron.vite.config.ts, vite.config.ts): it fails the build when a chunk holds one, or when a
// module would come from the real Nodebox package instead of stubs/nodebox.
//
// Sandpack's static client names one hosted address, its default bundler, which Oby always replaces
// with its own relay (main/sandpackRelay.ts): that one string is taken out as the module is read,
// and the build fails if it isn't there to take out (a new Sandpack, to be looked at again).

/** Each forbidden thing, and what it is. */
export const FORBIDDEN = [
  [/codesandbox\.io/i, "a CodeSandbox host (its hosted bundler, packager or preview server)"],
  [/blazingly\.io/i, "sandpack-bundler's npm CDN"],
  [/cdn\.tailwindcss\.com/i, "Tailwind's CDN"],
  [/cdn\.jsdelivr\.net/i, 'jsDelivr'],
  [/unpkg\.com/i, 'unpkg'],
  [/esm\.sh\b/i, 'esm.sh'],
  [/cdn\.skypack\.dev/i, 'Skypack'],
  [/Sustainable Use License/, "Nodebox's licence text"],
];

/** What a chunk holds that it may not: one line per thing found, none when it's clean. */
export function forbiddenIn(name, text) {
  return FORBIDDEN.filter(([re]) => re.test(text)).map(([re, what]) => `${name} holds ${what} (${re.source}).`);
}

const STATIC_DEFAULT = '"https://preview.sandpack-static-server.codesandbox.io"';
const STATIC_CHUNK = /@codesandbox[\\/]sandpack-client[\\/]dist[\\/]index-[0-9a-f]+\.mjs$/;

/** The plugin: Sandpack's static default taken out, and every chunk checked. */
export function localOnly() {
  return {
    name: 'oby-local-only',
    enforce: 'pre',
    async resolveId(source, importer, options) {
      if (source !== '@codesandbox/nodebox' && !source.includes('@codesandbox/nodebox/')) return null;
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true });
      if (resolved && !/[\\/]stubs[\\/]nodebox[\\/]/.test(resolved.id)) this.error(`Nodebox would be built in from ${resolved.id}, not stubs/nodebox: it is never part of Oby.`);
      return resolved;
    },
    transform(code, id) {
      if (!STATIC_CHUNK.test(id) || !code.includes('static-browser-server')) return null;
      if (!code.includes(STATIC_DEFAULT)) this.error(`Sandpack's static client no longer names ${STATIC_DEFAULT}: look at it again before building it in.`);
      return { code: code.replace(STATIC_DEFAULT, '""'), map: null };
    },
    generateBundle(_options, bundle) {
      const found = [];
      for (const [name, chunk] of Object.entries(bundle)) {
        const text = chunk.type === 'chunk' ? chunk.code : typeof chunk.source === 'string' ? chunk.source : '';
        found.push(...forbiddenIn(name, text));
      }
      if (found.length) this.error(`Not local (docs/contracts/artifacts-app.md, ARTIFACTSAPP-2):\n${found.join('\n')}`);
    },
  };
}
