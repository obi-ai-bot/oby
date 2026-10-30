/**
 * The Brain against this repository. What is checked here holds for any version of the code, so adding a route or a
 * screen never breaks it: the extraction is compared with the code itself, and the checks that must stay at zero do.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { config } from '../brain.config.js';
import { generateBrain, normPath, patternMatches } from '../generator/index.js';

const brain = generateBrain(config);
const lineAt = (file: string, line: number) => readFileSync(join(config.root, file), 'utf8').split('\n')[line - 1] ?? '';
const count = (kind: string) => brain.checks.filter(c => c.kind === kind);

describe('obid', () => {
  it('reads every route from the line that registers it', () => {
    expect(brain.routes.length).toBeGreaterThan(80);
    for (const r of brain.routes) expect(lineAt(r.source.file, r.source.line) + lineAt(r.source.file, r.source.line + 1), r.id).toMatch(/\.route\(/);
  });

  it('finds every handler', () => {
    for (const r of brain.routes) expect(r.handlerSource, `${r.id} → ${r.handler}`).not.toBeNull();
  });

  it('puts every top-level module in an area', () => {
    expect(count('layering').map(c => c.message)).toEqual([]);
  });

  it('keeps the phone allowlist matcher right', () => {
    expect(patternMatches('/v1/jobs/*/stop', '/v1/jobs/{id}/stop')).toBe(true);
    expect(patternMatches('/v1/jobs/*', '/v1/jobs/{id}/stop')).toBe(false);
    expect(patternMatches('/v1/artifacts/**', '/v1/artifacts/{id}/{version}/{*path}')).toBe(true);
    expect(normPath('/v1/jobs/{id}?limit=50')).toBe('/v1/jobs/{}');
  });
});

describe('the app', () => {
  it('reaches a real route with every call it makes', () => {
    expect(brain.api.length).toBeGreaterThan(20);
    expect(count('call-without-route').map(c => c.message)).toEqual([]);
  });

  it('says who owns every choice it keeps itself', () => {
    expect(count('local-unclassified').map(c => c.message)).toEqual([]);
  });

  it('ties every choice obid owes to an issue', () => {
    for (const k of brain.localKeys.filter(k => k.owner === 'obid')) expect(k.issue, k.key).toBeTruthy();
  });

  it('lists the first run\'s steps as states to open', () => {
    expect(brain.states.some(s => s.url.endsWith('?onboarding=hello'))).toBe(true);
  });
});

describe('AGENTS.md', () => {
  it('ties every rule to code that is still there and to a test', () => {
    expect(count('rule-unlinked').map(c => c.message)).toEqual([]);
  });

  it('keeps internal hosts out of the open-source code', () => {
    expect(count('internal-host').map(c => `${c.source!.file}:${c.source!.line}`)).toEqual([]);
  });
});
