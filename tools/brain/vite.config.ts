import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { config } from './brain.config.js';
import { brainPlugin } from './vite-plugin-brain.js';

// Local only: the server listens on 127.0.0.1 and nothing is built for deployment.
export default defineConfig({
  plugins: [react(), tailwindcss(), brainPlugin(config)],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: { host: '127.0.0.1', port: 4320 },
  preview: { host: '127.0.0.1', port: 4320 },
});
