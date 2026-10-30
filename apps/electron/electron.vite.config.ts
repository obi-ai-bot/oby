import { defineConfig } from 'electron-vite';
import renderer from './vite.config';

export default defineConfig({
  // the app ships no node_modules (electron-builder.yml): main's one package, the updater, goes in
  // its bundle
  main: { build: { externalizeDeps: { exclude: ['electron-updater'] } } },
  preload: {},
  renderer,
});
