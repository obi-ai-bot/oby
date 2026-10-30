import { existsSync, readFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import type { Plugin, ViteDevServer } from 'vite';
import type { BrainConfig } from './generator/config.js';
import { generateBrain } from './generator/index.js';
import type { BrainData } from './generator/types.js';

const WATCHED = /\.(rs|ts|tsx|md)$/;

/**
 * Serves the Brain at /brain.json, generated from the repository when the server starts, and again (telling the page)
 * whenever obid, the app or the API doc changes. Nothing is written to disk.
 */
export function brainPlugin(config: BrainConfig): Plugin {
  let data: BrainData | null = null;
  let error: string | null = null;
  let timer: NodeJS.Timeout | null = null;

  const generate = (server?: ViteDevServer) => {
    try {
      data = generateBrain(config);
      error = null;
      server?.config.logger.info(`brain: ${data.stats.routes} routes, ${data.stats.screens} screens, ${data.stats.checks} checks (${data.durationMs} ms)`, { timestamp: true });
    } catch (err) {
      error = err instanceof Error ? `${err.message}\n${err.stack ?? ''}` : String(err);
      server?.config.logger.error(`brain: couldn't generate: ${error}`, { timestamp: true });
    }
    server?.ws.send({ type: 'custom', event: 'brain:update', data: { generatedAt: data?.generatedAt, error } });
  };

  const sources = [config.obid.crate, config.obid.apiDoc, 'AGENTS.md', 'apps/electron/src'].map(d => `${config.root}/${d}`);

  return {
    name: 'oby-brain',
    configureServer(server) {
      generate(server);
      server.watcher.add(sources);
      const onChange = (file: string) => {
        const rel = relative(config.root, file).split(sep).join('/');
        if (!WATCHED.test(rel) || rel.startsWith('tools/') || rel.includes('/node_modules/') || rel.includes('/target/')) return;
        if (timer) clearTimeout(timer);
        timer = setTimeout(() => generate(server), 600);
      };
      server.watcher.on('change', onChange);
      server.watcher.on('add', onChange);
      server.watcher.on('unlink', onChange);
      server.middlewares.use('/brain.json', (_req, res) => {
        res.setHeader('Cache-Control', 'no-store');
        if (!data) {
          res.statusCode = 500;
          res.setHeader('Content-Type', 'text/plain; charset=utf-8');
          res.end(error ?? 'The Brain isn\'t generated yet');
          return;
        }
        res.setHeader('Content-Type', 'application/json; charset=utf-8');
        res.end(JSON.stringify(error ? { ...data, checks: [{ kind: 'extract', message: `The last run failed; this is the previous Brain. ${error.split('\n')[0]}` }, ...data.checks] } : data));
      });
      // make brain-shots' screenshots (.brain/shots): each state's PNG, and index.json
      const shots = join(config.root, 'tools/brain/.brain/shots');
      server.middlewares.use('/shots/', (req, res) => {
        const name = (req.url ?? '').split('?')[0].replace(/^\//, '');
        const file = join(shots, name);
        if (!/^[\w.-]+\.(png|json)$/.test(name) || !existsSync(file)) { res.statusCode = 404; res.end(); return; }
        res.setHeader('Cache-Control', 'no-store');
        res.setHeader('Content-Type', name.endsWith('.png') ? 'image/png' : 'application/json');
        res.end(readFileSync(file));
      });
      server.middlewares.use('/brain/regenerate', (req, res) => {
        if (req.method !== 'POST') { res.statusCode = 405; res.end(); return; }
        generate(server);
        res.statusCode = error ? 500 : 204;
        res.end(error ?? undefined);
      });
    },
  };
}
