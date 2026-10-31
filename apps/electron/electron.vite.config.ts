import { defineConfig } from 'electron-vite';
import renderer from './vite.config';

export default defineConfig({
  // the app ships no node_modules (electron-builder.yml): main's one package, the updater, goes in
  // its bundle
  main: { build: { externalizeDeps: { exclude: ['electron-updater'] } } },
  // the window is sandboxed (main/index.ts): a sandboxed preload can't be an ES module, and may
  // require only electron's own renderer modules (its imports are bundled in)
  preload: { build: { rollupOptions: { output: { format: 'cjs' } } } },
  renderer,
});
