#!/usr/bin/env node
// A job's check runs in a clean checkout with no network (docs/contracts/publishing.md,
// PUBLISHING-6): node_modules isn't there, but the agent's npm cache is in HOME. Install from
// that cache before typecheck, test or build; --ignore-scripts skips the native addon a path
// with spaces can break. With modules already there, nothing runs.
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

const r = spawnSync(
  'npm',
  ['install', '--offline', '--ignore-scripts'],
  { cwd: root, stdio: 'inherit', env: process.env },
);
process.exit(r.status ?? 1);
