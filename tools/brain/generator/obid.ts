import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import type { BrainConfig } from './config.js';

/** What `obid-extract` (tools/brain/obid-extract, Rust + syn) prints: obid's source as data. */
export interface RustExtract {
  files: Array<{ path: string; module: string; lines: number; doc: string | null }>;
  modules: Array<{ id: string; file: string; lines: number; uses: string[] }>;
  externalUses: Array<{ file: string; crates: string[] }>;
  routes: Array<{
    method: string; path: string; router: string; file: string; line: number; handler: string;
    handlerFile: string | null; handlerLine: number | null; handlerDoc: string | null; extractors: string[];
    errors: Array<{ status: string; message: string; line: number }>;
  }>;
  phoneRoutes: Array<{ method: string; pattern: string; line: number }>;
  spawns: Array<{ file: string; line: number; kind: string; inFn: string | null; inFnLine: number | null; doc: string | null }>;
  storedFiles: Array<{ name: string; file: string; line: number; inFn: string | null }>;
  structs: Array<{ name: string; file: string; line: number; renameAll: string | null; fields: Array<{ name: string; json: string; optional: boolean; skipIfNone: boolean; flatten: boolean; ty: string | null }> }>;
  consts: Array<{ name: string; file: string; line: number; value: string }>;
  fns: Array<{ file: string; name: string; line: number; test: boolean; doc: string | null }>;
  errors: Array<{ file: string; message: string }>;
}

const newest = (dir: string): number => {
  let t = 0;
  for (const name of readdirSync(dir)) {
    if (name === 'target' || name.startsWith('.')) continue;
    const full = join(dir, name);
    const s = statSync(full);
    t = Math.max(t, s.isDirectory() ? newest(full) : s.mtimeMs);
  }
  return t;
};

/** Builds the extractor when its source is newer than its binary (the first run takes a minute), then runs it. */
export function extractObid(config: BrainConfig): RustExtract {
  const dir = join(config.root, config.obid.extractor);
  const bin = join(dir, 'target', 'release', 'obid-extract');
  if (!existsSync(bin) || statSync(bin).mtimeMs < newest(dir)) {
    execFileSync('cargo', ['build', '--release', '--quiet', '--manifest-path', join(dir, 'Cargo.toml')], {
      stdio: ['ignore', 'ignore', 'pipe'], env: { ...process.env, CARGO_TARGET_DIR: join(dir, 'target') },
    });
  }
  const out = execFileSync(bin, [config.root], { maxBuffer: 256 * 1024 * 1024, encoding: 'utf8' });
  return JSON.parse(out) as RustExtract;
}
