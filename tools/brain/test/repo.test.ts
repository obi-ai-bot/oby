/**
 * The Brain against this repository. What is checked here holds for any version of the code, so adding a route or a
 * screen never breaks it: the extraction is compared with the code itself, and the checks that must stay at zero do.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { config } from '../brain.config.js';
import { generateBrain, normPath, patternMatches, readApiDoc } from '../generator/index.js';

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

// docs/contracts/retired.md, "Routes obid does not serve"
describe('retired routes', () => {
  it('retired_7_every_phone_route_is_served', () => {
    expect(count('phone-without-route').map(c => c.message)).toEqual([]);
  });

  it('retired_8_every_documented_route_is_served', () => {
    expect(count('doc-without-route').map(c => c.message)).toEqual([]);
    expect(count('gone-but-routed').map(c => c.message)).toEqual([]);
    const text = [
      'A phone once paired a Watch with `POST /v1/watches`, and `GET /v1/nowhere` was promised.',
      'A turn reaches `POST /v1/agent/mcp/conversation-<id>`.',
      '## Routes obid does not serve',
      '- `POST /v1/watches` (gone 2026-11-02): the Swift runner\'s.',
      '- `GET /v1/status` (gone 2026-11-02): listed by mistake.',
      '## After the list',
      'Then `PATCH /v1/elsewhere`.',
    ].join('\n');
    const doc = readApiDoc(text, new Set(['GET /v1/status', 'POST /v1/agent/mcp/{}']), { file: 'api.md', gone: 'Routes obid does not serve' });
    expect(doc.checks.map(c => `${c.kind} ${c.ref}`)).toEqual([
      'doc-without-route GET /v1/nowhere',
      'doc-without-route PATCH /v1/elsewhere',
      'gone-but-routed GET /v1/status',
    ]);
    expect([...doc.documented]).toEqual(['POST /v1/agent/mcp/{}']);
  });

  it('retired_9_the_swift_client_calls_only_served_routes', () => {
    const exceptions = [
      'POST /v1/jobs/{}/proposals/{}/file', // the task page's file-a-proposal button: removing it changes a screen, its own task
    ];
    const swift = count('call-without-route').filter(c => config.swiftClient.some(d => c.source?.file.startsWith(`${d}/`)) && c.source!.file.endsWith('.swift'));
    expect(swift.filter(c => !exceptions.includes(c.ref!)).map(c => c.message)).toEqual([]);
    // a scan that reads nothing can't pass
    expect(brain.routes.find(r => r.id === 'GET /v1/status')!.callers.some(c => c.source.file === 'apps/shared/Runner.swift')).toBe(true);
    expect(brain.routes.flatMap(r => r.callers).filter(c => c.source.file.endsWith('.swift')).length).toBeGreaterThan(50);
  });
});

describe('the app', () => {
  it('reaches a real route with every call it makes', () => {
    expect(brain.api.length).toBeGreaterThan(20);
    // the Swift client's calls are RETIRED-9's
    expect(count('call-without-route').filter(c => !c.source?.file.endsWith('.swift')).map(c => c.message)).toEqual([]);
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
