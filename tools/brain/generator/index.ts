import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import type { BrainConfig } from './config.js';
import { scanDesign } from './design.js';
import { extractObid, type RustExtract } from './obid.js';
import type {
  AccessKind, Area, BrainData, Check, JumpState, LocalKey, Module, PhoneRoute, Route, Rule, RuleLink, StoredFile, WireType,
} from './types.js';

/** `/v1/jobs/{id}/stop` and `/v1/jobs/{}/stop` both → `/v1/jobs/{}/stop`; a trailing `{*path}` → `/**`. */
export const normPath = (p: string) => p.split('?')[0].replace(/\{\*[^}]*\}/g, '**').replace(/\{[^}]*\}/g, '{}').replace(/\/$/, '');

/** A phone allowlist pattern (`*` one segment, `**` last only, one or more) against a route's path. */
export function patternMatches(pattern: string, path: string): boolean {
  const ps = pattern.split('/'); const rs = normPath(path).split('/');
  for (let i = 0; i < ps.length; i++) {
    if (ps[i] === '**') return rs.length > i;
    if (i >= rs.length) return false;
    if (ps[i] === '*') { if (rs[i] === '**') return false; continue; }
    if (ps[i] !== rs[i]) return false;
  }
  return ps.length === rs.length;
}

const TEXT = /\.(rs|ts|tsx|js|mjs|swift|md|json|jsonc|ya?ml|sh|py|toml|plist|xcconfig|html)$/;
function walkText(dir: string, out: string[] = []): string[] {
  if (!existsSync(dir)) return out;
  if (!statSync(dir).isDirectory()) { out.push(dir); return out; }
  for (const name of readdirSync(dir)) {
    if (['node_modules', 'target', '.git', 'dist', 'out', 'build', '.brain', 'worktrees'].includes(name)) continue;
    const full = join(dir, name);
    const s = statSync(full);
    if (s.isDirectory()) walkText(full, out);
    else if (TEXT.test(name) && s.size < 2_000_000) out.push(full);
  }
  return out;
}

function git(root: string): BrainData['git'] {
  try {
    const run = (...a: string[]) => execFileSync('git', ['-C', root, ...a], { encoding: 'utf8' }).trim();
    return { branch: run('rev-parse', '--abbrev-ref', 'HEAD'), commit: run('rev-parse', '--short', 'HEAD'), dirty: run('status', '--porcelain').length > 0 };
  } catch {
    return null;
  }
}

const FORBIDDEN_IN_DOMAIN = ['axum', 'reqwest', 'std::process', 'tokio::process', 'std::fs', 'tokio::fs', 'std::net', 'hyper'];

export function generateBrain(config: BrainConfig): BrainData {
  const started = Date.now();
  const root = config.root;
  const rust: RustExtract = extractObid(config);
  const design = scanDesign(config);
  const checks: Check[] = [];
  for (const e of rust.errors) checks.push({ kind: 'extract', message: `obid-extract couldn't read it: ${e.message}`, source: { file: e.file, line: 1 } });

  // ---- docs: every `METHOD /v1/...` the API doc mentions
  const docText = existsSync(join(root, config.obid.apiDoc)) ? readFileSync(join(root, config.obid.apiDoc), 'utf8') : '';
  const docMentions = new Map<string, number>();
  docText.split('\n').forEach((line, i) => {
    for (const m of line.matchAll(/`(GET|POST|PUT|PATCH|DELETE) (\/v1\/[^`\s]*)`/g)) {
      const id = `${m[1]} ${normPath(m[2])}`;
      if (!docMentions.has(id)) docMentions.set(id, i + 1);
    }
  });

  // ---- callers: the renderer's api methods and main's literals, matched to routes
  const routeKey = (method: string, path: string) => `${method} ${normPath(path)}`;
  const rustKeys = new Set(rust.routes.map(r => routeKey(r.method, r.path)));
  const idOfKey = new Map(rust.routes.map(r => [routeKey(r.method, r.path), `${r.method} ${r.path}`]));
  const callersOf = new Map<string, Route['callers']>();
  const addCaller = (key: string, via: string, file: string, line: number) => {
    if (!callersOf.has(key)) callersOf.set(key, []);
    callersOf.get(key)!.push({ via, source: { file, line } });
  };
  for (const a of design.api) {
    const key = routeKey(a.method, a.path);
    if (rustKeys.has(key)) { a.route = idOfKey.get(key)!; addCaller(key, `api.${a.name}`, a.source.file, a.source.line); }
    else checks.push({ kind: 'call-without-route', message: `api.${a.name} calls ${a.method} ${a.path}, which obid doesn't route`, source: a.source, ref: a.name });
  }
  for (const c of design.mainCalls) {
    const key = routeKey(c.method, c.path);
    if (rustKeys.has(key)) addCaller(key, c.file.split('/').pop()!, c.file, c.line);
    else checks.push({ kind: 'call-without-route', message: `${c.file.split('/').pop()} calls ${c.method} ${c.path}, which obid doesn't route`, source: { file: c.file, line: c.line } });
  }
  const apiRoute = new Map(design.api.map(a => [a.name, a.route]));
  for (const s of design.screens) for (const l of s.live) l.route = apiRoute.get(l.api) ?? null;

  // ---- routes
  const phone = rust.phoneRoutes;
  const routes: Route[] = rust.routes.map(r => {
    const id = `${r.method} ${r.path}`;
    const key = routeKey(r.method, r.path);
    const onPhone = phone.some(p => p.method === r.method && patternMatches(p.pattern, r.path));
    const agentRoute = /^\/v1\/agent\/(mcp|tetrix)\//.test(r.path);
    const access: AccessKind[] = r.router === 'gateway' ? ['pairing'] : agentRoute ? ['owner', 'agent'] : ['owner'];
    if (onPhone && !access.includes('phone')) access.push('phone');
    // A screen's own call also counts as a caller of the route behind it.
    const callers = [...(callersOf.get(key) ?? [])];
    return {
      id, method: r.method, path: r.path, router: r.router, source: { file: r.file, line: r.line }, handler: r.handler,
      handlerSource: r.handlerFile ? { file: r.handlerFile, line: r.handlerLine ?? 1 } : null, handlerDoc: r.handlerDoc,
      extractors: r.extractors, errors: r.errors, access, phone: onPhone, documented: docMentions.has(key), callers,
    };
  }).sort((a, b) => a.path.localeCompare(b.path) || a.method.localeCompare(b.method));
  for (const r of routes) if (!r.documented && r.router !== 'gateway') checks.push({ kind: 'route-undocumented', message: `${r.id} isn't in ${config.obid.apiDoc}`, source: r.source, ref: r.id });
  const docRoutes = [...docMentions].map(([key, line]) => ({ id: idOfKey.get(key) ?? key, line, exists: rustKeys.has(key) }));
  for (const d of docRoutes) if (!d.exists) checks.push({ kind: 'doc-without-route', message: `${config.obid.apiDoc} documents ${d.id}, which obid doesn't route`, source: { file: config.obid.apiDoc, line: d.line }, ref: d.id });
  const phoneRoutes: PhoneRoute[] = phone.map(p => ({ ...p, matches: routes.filter(r => r.method === p.method && patternMatches(p.pattern, r.path)).map(r => r.id) }));
  for (const p of phoneRoutes) if (!p.matches.length) checks.push({ kind: 'phone-without-route', message: `The phone allowlist lets ${p.method} ${p.pattern} through, but no route matches it`, source: { file: 'core/crates/obid/src/gateway.rs', line: p.line } });

  // ---- modules, areas, layering
  const areaOf = new Map<string, string>();
  for (const a of config.areas) for (const m of a.modules) areaOf.set(m, a.id);
  const externalByModule = new Map<string, Set<string>>();
  const moduleOfFile = new Map(rust.files.map(f => [f.path, f.module.split('::')[0] || 'lib']));
  for (const e of rust.externalUses) {
    const m = moduleOfFile.get(e.file) ?? 'lib';
    if (!externalByModule.has(m)) externalByModule.set(m, new Set());
    for (const c of e.crates) externalByModule.get(m)!.add(c);
  }
  const usedBy = new Map<string, string[]>();
  for (const m of rust.modules) for (const u of m.uses) { if (!usedBy.has(u)) usedBy.set(u, []); usedBy.get(u)!.push(m.id); }
  const modules: Module[] = rust.modules.map(m => ({
    id: m.id, file: m.file, lines: m.lines, area: areaOf.get(m.id) ?? null, uses: m.uses, usedBy: (usedBy.get(m.id) ?? []).sort(),
    external: [...(externalByModule.get(m.id) ?? [])].sort(),
  })).sort((a, b) => b.lines - a.lines);
  for (const m of modules) if (!m.area && !['lib', 'main'].includes(m.id)) checks.push({ kind: 'layering', message: `Module ${m.id} isn't in any area of brain.config.ts`, source: { file: m.file, line: 1 }, ref: m.id });
  const areas: Area[] = config.areas.map(a => ({
    id: a.id, title: a.title, summary: a.summary, domain: !!a.domain, modules: a.modules.filter(m => rust.modules.some(x => x.id === m)),
    lines: modules.filter(m => m.area === a.id).reduce((n, m) => n + m.lines, 0),
  }));
  for (const a of config.areas) {
    for (const m of a.modules) if (!rust.modules.some(x => x.id === m)) checks.push({ kind: 'layering', message: `brain.config.ts puts ${m} in ${a.title}, but obid has no such module`, ref: m });
    if (!a.domain) continue;
    for (const m of a.modules) {
      const bad = [...(externalByModule.get(m) ?? [])].filter(c => FORBIDDEN_IN_DOMAIN.includes(c));
      if (bad.length) checks.push({ kind: 'layering', message: `${m} is domain code (${a.title}) but uses ${bad.join(', ')}`, source: { file: rust.modules.find(x => x.id === m)?.file ?? '', line: 1 }, ref: m });
    }
  }
  // two-module cycles: a uses b and b uses a
  const cycles: string[][] = [];
  for (const m of rust.modules) for (const u of m.uses) if (m.id < u && rust.modules.find(x => x.id === u)?.uses.includes(m.id)) cycles.push([m.id, u]);

  // ---- background work, stored files
  const spawns = rust.spawns.map(s => ({ source: { file: s.file, line: s.line }, kind: s.kind, inFn: s.inFn, inFnLine: s.inFnLine, doc: s.doc, module: moduleOfFile.get(s.file) ?? 'lib' }));
  const storedMap = new Map<string, StoredFile>();
  for (const f of rust.storedFiles) {
    if (!storedMap.has(f.name)) storedMap.set(f.name, { name: f.name, sites: [] });
    storedMap.get(f.name)!.sites.push({ file: f.file, line: f.line, inFn: f.inFn });
  }
  const storedFiles = [...storedMap.values()].sort((a, b) => a.name.localeCompare(b.name));

  // ---- wire: obid-wire.ts beside obid's serde structs
  const structs = new Map(rust.structs.map(s => [s.name, s]));
  // a struct's JSON keys, with what each `#[serde(flatten)]` field's own struct brings in
  type RustStruct = (typeof rust.structs)[number];
  const keysOf = (s: RustStruct, seen = new Set<string>()): Array<{ key: string; optional: boolean }> => {
    seen.add(s.name);
    return s.fields.flatMap(f => {
      if (!f.flatten) return [{ key: f.json, optional: f.optional || f.skipIfNone }];
      const inner = f.ty ? structs.get(f.ty) : undefined;
      return inner && !seen.has(inner.name) ? keysOf(inner, seen) : [];
    });
  };
  const wire: WireType[] = design.wire.map(w => {
    const s = structs.get(config.app.wireRust[w.name] ?? w.name);
    const rustKeys2 = s ? keysOf(s) : [];
    const tsKeys = new Set(w.keys.map(k => k.key));
    return {
      name: w.name, source: { file: w.file, line: w.line }, keys: w.keys,
      rust: s ? { name: s.name, source: { file: s.file, line: s.line }, keys: rustKeys2 } : null,
      onlyTs: s ? w.keys.filter(k => !rustKeys2.some(r => r.key === k.key)).map(k => k.key) : [],
      onlyRust: s ? rustKeys2.filter(r => !tsKeys.has(r.key)).map(r => r.key) : [],
    };
  });
  for (const w of wire) if (w.onlyTs.length) checks.push({ kind: 'wire', message: `${w.name}: the app expects ${w.onlyTs.join(', ')}, which ${w.rust!.name} doesn't declare (added by hand with json!, or missing)`, source: w.source, ref: w.name });

  // ---- AGENTS.md's rules, tied to code and tests
  const findSymbol = (spec: string): RuleLink['source'] => {
    const [file, symbol] = spec.split('#');
    const fn = rust.fns.find(f => f.file === file && (f.name === symbol || f.name.endsWith(`::${symbol}`)));
    if (fn) return { file: fn.file, line: fn.line };
    const c = rust.consts.find(x => x.file === file && x.name === symbol);
    if (c) return { file: c.file, line: c.line };
    if (!existsSync(join(root, file))) return null;
    if (!symbol) return { file, line: 1 };
    // Anything else the extractor doesn't list (a slice const, a static, a TypeScript name): its definition by text.
    const lines = readFileSync(join(root, file), 'utf8').split('\n');
    const at = lines.findIndex(l => new RegExp(`\\b(const|static|fn|struct|enum|type|interface|function|it|test)\\b[\\s(']*${symbol}\\b`).test(l));
    return at >= 0 ? { file, line: at + 1 } : null;
  };
  const rules: Rule[] = config.rules.map(r => {
    const links: RuleLink[] = [
      ...r.code.map(spec => ({ label: spec.split('#')[1] ?? spec, kind: 'code' as const, file: spec.split('#')[0], symbol: spec.split('#')[1] ?? '', source: findSymbol(spec) })),
      ...r.tests.map(spec => ({ label: spec.split('#')[1] ?? spec, kind: 'test' as const, file: spec.split('#')[0], symbol: spec.split('#')[1] ?? '', source: findSymbol(spec) })),
    ];
    for (const l of links) if (!l.source) checks.push({ kind: 'rule-unlinked', message: `"${r.title}": ${l.kind === 'test' ? 'test' : 'code'} ${l.file}#${l.symbol} isn't there any more`, ref: r.id });
    if (!r.tests.length) checks.push({ kind: 'rule-unlinked', message: `"${r.title}" has no test tied to it in brain.config.ts`, ref: r.id });
    return { id: r.id, title: r.title, text: r.text, links };
  });

  // ---- choices the app keeps itself
  const keyMap = new Map<string, LocalKey>();
  for (const s of design.screens) for (const l of s.local) {
    const id = `${l.kind}:${l.key}`;
    if (!keyMap.has(id)) {
      const rule = config.local.find(c => c.key === l.key || (c.key.endsWith('*') && l.key.startsWith(c.key.slice(0, -1))));
      keyMap.set(id, { key: l.key, kind: l.kind, sites: [], owner: rule?.owner ?? null, issue: rule?.issue ?? null, note: rule?.note ?? null });
    }
    keyMap.get(id)!.sites.push({ file: s.file, line: l.line });
  }
  const localKeys = [...keyMap.values()].sort((a, b) => (a.owner === 'obid' ? 0 : a.owner ? 1 : -1) - (b.owner === 'obid' ? 0 : b.owner ? 1 : -1) || a.key.localeCompare(b.key));
  for (const k of localKeys) {
    if (!k.owner) checks.push({ kind: 'local-unclassified', message: `The app keeps "${k.key}" (${k.kind}) and brain.config.ts doesn't say whether it's the app's or owed by obid`, source: k.sites[0], ref: k.key });
    else if (k.owner === 'obid') checks.push({ kind: 'local-owed', message: `"${k.key}" is kept in the app until obid takes it${k.issue ? ` (#${k.issue})` : ''}`, source: k.sites[0], ref: k.key });
  }

  // ---- internal hosts in literals
  const brainConfig = join(root, 'tools/brain/brain.config.ts');
  for (const dir of config.hostScan) {
    for (const file of walkText(join(root, dir))) {
      if (file === brainConfig) continue;
      const lines = readFileSync(file, 'utf8').split('\n');
      lines.forEach((text, i) => {
        for (const h of config.internalHosts) if (text.includes(h)) checks.push({ kind: 'internal-host', message: `${h} appears in the open-source code`, source: { file: relative(root, file).split(sep).join('/'), line: i + 1 } });
      });
    }
  }

  // ---- states to jump to on the mock
  const states: JumpState[] = [
    ...config.app.flags.map(f => ({ id: `flag-${f.flag}`, group: 'URL flags', label: f.label, url: `${config.app.webUrl}?${f.flag}`, note: f.note ?? null })),
    ...design.steps.map(s => ({ id: `step-${s}`, group: 'First run', label: s, url: `${config.app.webUrl}?onboarding=${s}`, note: null })),
    // a scenario's own address, else the Control Center's default (dev/ControlCenter.tsx, hrefOf)
    ...design.scenarios.map(s => ({ id: `dev-${s.id}`, group: 'Control Center scenarios', label: s.label, url: `${config.app.webUrl}${s.href ?? `?welcome&dev=${s.id}`}`, note: null })),
  ];

  const used = new Set(routes.filter(r => r.callers.length).map(r => r.id));
  const data: BrainData = {
    name: config.name, root, generatedAt: new Date().toISOString(), durationMs: 0, git: git(root),
    stats: {
      routes: routes.length, routesCalled: used.size, phoneRoutes: phoneRoutes.length, modules: modules.length, cycles: cycles.length,
      obidLines: rust.files.reduce((n, f) => n + f.lines, 0), spawns: spawns.length, storedFiles: storedFiles.length,
      screens: design.screens.filter(s => s.kind === 'screen').length, apiMethods: design.api.length,
      localKeys: localKeys.length, owed: localKeys.filter(k => k.owner === 'obid').length, tests: rust.fns.filter(f => f.test).length,
      checks: checks.filter(c => c.kind !== 'local-owed').length,
    },
    routes, phoneRoutes, docRoutes, areas, modules, cycles, spawns, storedFiles, wire, rules,
    api: design.api, screens: design.screens.sort((a, b) => a.file.localeCompare(b.file)), localKeys, states, checks,
  };
  data.durationMs = Date.now() - started;
  return data;
}
