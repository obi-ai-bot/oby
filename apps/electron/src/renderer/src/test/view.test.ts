import { readFileSync } from 'node:fs';
import { posix } from 'node:path';
import { describe, expect, it } from 'vitest';

// The live path without the mock (docs/contracts/live-state.md, LIVE-22): every module a file
// imports, static or dynamic, as a path from src/renderer/src, without its extension
const importsOf = (file: string) => {
  const source = readFileSync(new URL(`../${file}`, import.meta.url), 'utf8');
  const specs = [...source.matchAll(/(?:\bfrom\s*|\bimport\s*\(\s*|^\s*import\s+)['"]([^'"]+)['"]/gm)].map((m) => m[1]);
  return specs.filter((s) => s.startsWith('.')).map((s) => posix.join('.', s).replace(/\.tsx?$/, '').replace(/\/index$/, ''));
};

describe('LIVE-22', () => {
  it('live_22_live_ts_imports_nothing_from_the_mock', () => {
    const imports = importsOf('live.ts');
    expect(imports).toContain('view');
    expect(imports).not.toContain('mock');
  });

  it('live_22: view.ts imports nothing from the mock or lean.ts', () => {
    const imports = importsOf('view.ts');
    expect(imports).toContain('brands');
    expect(imports).not.toContain('mock');
    expect(imports).not.toContain('lean');
  });
});
