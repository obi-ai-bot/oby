#!/usr/bin/env node
// A job's check runs in a clean checkout with no network (docs/contracts/publishing.md,
// PUBLISHING-6): node_modules isn't there. The check's HOME is the job's temp
// (process::sandbox_env); Cursor's agent often has a different HOME, so the npm cache must
// live under CFFIXED_USER_HOME (also the job's temp) for both. --ignore-scripts skips the
// native addon a path with spaces can break. With modules already there, nothing runs.
import { accessSync, constants } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const tsc = join(root, 'node_modules', 'typescript', 'bin', 'tsc');
try {
  accessSync(tsc, constants.F_OK);
  process.exit(0);
} catch {
  // install below
}

const home = process.env.CFFIXED_USER_HOME || process.env.HOME || '';
const cache = process.env.npm_config_cache || (home ? join(home, '.npm') : '');
const args = ['install', '--offline', '--ignore-scripts'];
if (cache) args.push('--cache', cache);

const r = spawnSync('npm', args, { cwd: root, stdio: 'inherit', env: process.env });
if ((r.status ?? 1) !== 0) {
  console.error(
    `ensure-modules: npm install --offline failed (cache ${cache || 'default'}). ` +
      'Fetch packages into CFFIXED_USER_HOME/.npm while the agent works.',
  );
}
process.exit(r.status ?? 1);
