import { existsSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { Node, Project, SyntaxKind, ts, type SourceFile } from 'ts-morph';
import type { BrainConfig } from './config.js';
import type { ApiMethod, LocalKind, Screen } from './types.js';

/**
 * Sangam's app, read as code (no type checker: the syntax and the imports). For each file: which obid calls it makes
 * (directly through `api`, or through the stores it imports), which mock values it shows, which choices it keeps
 * itself, where it is gated, and the words it shows.
 */
export interface DesignScan {
  api: ApiMethod[];
  screens: Screen[];
  mainCalls: Array<{ method: string; path: string; file: string; line: number }>;
  steps: string[];
  scenarios: Array<{ id: string; label: string; href: string | null }>;
  wire: Array<{ name: string; file: string; line: number; keys: Array<{ key: string; optional: boolean }> }>;
}

const rel = (root: string, file: string) => relative(root, file).split(sep).join('/');

function walk(dir: string, out: string[] = []): string[] {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (name === 'node_modules' || name.startsWith('.')) continue;
    if (statSync(full).isDirectory()) walk(full, out);
    else if (/\.(ts|tsx)$/.test(name) && !/\.d\.ts$/.test(name) && !/\.test\.tsx?$/.test(name)) out.push(full);
  }
  return out;
}

/** `/v1/jobs/${id(j)}/stop` → `/v1/jobs/{}/stop`; a query built with `${q(...)}` is dropped. */
function templatePath(node: Node): string | null {
  if (Node.isStringLiteral(node) || Node.isNoSubstitutionTemplateLiteral(node)) return node.getLiteralText().split('?')[0];
  if (Node.isTemplateExpression(node)) {
    let s = node.getHead().getLiteralText();
    for (const span of node.getTemplateSpans()) {
      const lit = span.getLiteral().getLiteralText();
      const expr = span.getExpression().getText();
      // A query (`${q({...})}`, `?${…}`) isn't part of the path.
      if (/^q\(/.test(expr) || s.endsWith('?')) { s = s.replace(/\?$/, ''); break; }
      s += `{}${lit}`;
    }
    return s.split('?')[0];
  }
  return null;
}

const WORD_PROPS = new Set(['placeholder', 'title', 'label', 'aria-label', 'about', 'help', 'headline', 'text', 'description', 'note', 'alt']);

interface Decl { file: string; name: string; node: Node }

/** Every identifier under a node, and every `api.x` access, with their lines: one pass over the raw TypeScript nodes. */
interface Raw {
  ids: Array<{ name: string; line: number; inImport: boolean }>;
  apis: Array<{ name: string; line: number }>;
  /** Calls with a literal-ish first argument: callee text, the argument node, line. */
  calls: Array<{ callee: string; arg: ts.Expression | undefined; line: number }>;
  words: Array<{ text: string; line: number }>;
  jsx: boolean;
}

/** Identifiers, `api.x` accesses, calls and JSX words under a node, with their lines: one pass over the raw nodes. */
function scanRaw(node: Node): Raw {
  const sf = node.getSourceFile().compilerNode;
  const out: Raw = { ids: [], apis: [], calls: [], words: [], jsx: false };
  const lineOf = (n: ts.Node) => sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1;
  const isStr = (e: ts.Node | undefined): e is ts.StringLiteral | ts.NoSubstitutionTemplateLiteral => !!e && (ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e));
  // A name that is declared or read as a property here isn't a reference to the top-level thing of that name.
  const notARef = (n: ts.Identifier) => {
    const p = n.parent;
    return (ts.isPropertyAccessExpression(p) && p.name === n) || (ts.isPropertyAssignment(p) && p.name === n)
      || ((ts.isPropertySignature(p) || ts.isPropertyDeclaration(p) || ts.isMethodDeclaration(p) || ts.isMethodSignature(p)) && p.name === n)
      || (ts.isJsxAttribute(p) && p.name === n) || (ts.isBindingElement(p) && (p.propertyName === n || p.name === n))
      || (ts.isParameter(p) && p.name === n) || (ts.isVariableDeclaration(p) && p.name === n) || (ts.isFunctionDeclaration(p) && p.name === n)
      || ts.isQualifiedName(p) || ts.isTypeReferenceNode(p);
  };
  const visit = (n: ts.Node) => {
    if (ts.isImportDeclaration(n)) return;
    if (ts.isPropertyAccessExpression(n) && ts.isIdentifier(n.expression) && n.expression.text === 'api') out.apis.push({ name: n.name.text, line: lineOf(n) });
    else if (ts.isIdentifier(n) && !notARef(n)) out.ids.push({ name: n.text, line: lineOf(n), inImport: false });
    else if (ts.isCallExpression(n)) out.calls.push({ callee: n.expression.getText(sf), arg: n.arguments[0], line: lineOf(n) });
    else if (ts.isJsxElement(n) || ts.isJsxSelfClosingElement(n)) out.jsx = true;
    else if (ts.isJsxText(n)) out.words.push({ text: n.text, line: lineOf(n) });
    else if (ts.isJsxAttribute(n) && WORD_PROPS.has(n.name.getText(sf)) && n.initializer && ts.isStringLiteral(n.initializer)) out.words.push({ text: n.initializer.text, line: lineOf(n) });
    else if (ts.isJsxExpression(n) && isStr(n.expression)) out.words.push({ text: n.expression.text, line: lineOf(n) });
    ts.forEachChild(n, visit);
  };
  visit(node.compilerNode);
  return out;
}

export function scanDesign(config: BrainConfig): DesignScan {
  const root = config.root;
  const rendererDir = join(root, config.app.renderer);
  const sharedDir = dirname(config.app.wireFile);
  const files = [...walk(rendererDir), ...config.app.main.flatMap(d => walk(join(root, d))), ...walk(join(root, sharedDir))];
  const project = new Project({ useInMemoryFileSystem: false, skipAddingFilesFromTsConfig: true, compilerOptions: { jsx: 4, allowJs: false } });
  for (const f of files) project.addSourceFileAtPath(f);
  const byPath = new Map(project.getSourceFiles().map(sf => [rel(root, sf.getFilePath()), sf]));

  // ---- api: export const api = { name: (...) => call<T>('GET', `/v1/...`) }
  const apiFile = byPath.get(config.app.apiFile);
  const api: ApiMethod[] = [];
  if (apiFile) {
    const decl = apiFile.getVariableDeclaration('api');
    const obj = decl?.getInitializerIfKind(SyntaxKind.ObjectLiteralExpression);
    for (const p of obj?.getProperties() ?? []) {
      if (!Node.isPropertyAssignment(p)) continue;
      const call = p.getDescendantsOfKind(SyntaxKind.CallExpression).find(c => c.getExpression().getText() === 'call');
      const [m, path] = call?.getArguments() ?? [];
      const method = m && Node.isStringLiteral(m) ? m.getLiteralText() : null;
      const pathText = path ? templatePath(path) : null;
      if (method && pathText) api.push({ name: p.getName(), method, path: pathText, source: { file: config.app.apiFile, line: p.getStartLineNumber() }, route: null });
    }
  }
  const apiNames = new Set(api.map(a => a.name));

  // ---- imports per file: local name → { file, name } (named imports and namespace-less defaults)
  const importsOf = new Map<string, Map<string, { file: string; name: string; typeOnly: boolean }>>();
  const resolveSpec = (from: SourceFile, spec: string): string | null => {
    if (!spec.startsWith('.')) return null;
    const base = join(from.getDirectoryPath(), spec);
    for (const cand of [base, `${base}.ts`, `${base}.tsx`, join(base, 'index.ts'), join(base, 'index.tsx')]) {
      const r = rel(root, cand);
      if (byPath.has(r)) return r;
    }
    return null;
  };
  for (const [path, sf] of byPath) {
    const map = new Map<string, { file: string; name: string; typeOnly: boolean }>();
    for (const imp of sf.getImportDeclarations()) {
      const target = resolveSpec(sf, imp.getModuleSpecifierValue());
      if (!target) continue;
      const allType = imp.isTypeOnly();
      for (const n of imp.getNamedImports()) map.set(n.getAliasNode()?.getText() ?? n.getName(), { file: target, name: n.getName(), typeOnly: allType || n.isTypeOnly() });
      const def = imp.getDefaultImport();
      if (def) map.set(def.getText(), { file: target, name: 'default', typeOnly: allType });
    }
    importsOf.set(path, map);
  }

  // ---- top-level declarations per file, so a store's function can be followed to the api calls inside it
  const decls = new Map<string, Decl>();
  for (const [path, sf] of byPath) {
    for (const fn of sf.getFunctions()) if (fn.getName()) decls.set(`${path}#${fn.getName()}`, { file: path, name: fn.getName()!, node: fn });
    for (const v of sf.getVariableDeclarations()) decls.set(`${path}#${v.getName()}`, { file: path, name: v.getName(), node: v });
    const def = sf.getDefaultExportSymbol()?.getDeclarations()[0];
    if (def) decls.set(`${path}#default`, { file: path, name: 'default', node: def });
  }

  // What a declaration reaches: api names, followed through same-file and imported declarations (memoised, cycle-safe).
  const memo = new Map<string, Set<string>>();
  const stops = new Set(config.app.stopAt);
  // A component imported from another .tsx file is a screen it renders, not a call it makes.
  const isComponent = (file: string, name: string) => file.endsWith('.tsx') && (name === 'default' || /^[A-Z]/.test(name));
  const reach = (key: string, stack = new Set<string>()): Set<string> => {
    const hit = memo.get(key);
    if (hit) return hit;
    const d = decls.get(key);
    const out = new Set<string>();
    if (!d || stack.has(key) || stops.has(key)) return out;
    stack.add(key);
    const imports = importsOf.get(d.file)!;
    const isApiFile = d.file === config.app.apiFile;
    const raw = scanRaw(d.node);
    if (imports.get('api')?.file === config.app.apiFile || isApiFile) for (const a of raw.apis) if (apiNames.has(a.name)) out.add(a.name);
    for (const name of new Set(raw.ids.map(i => i.name))) {
      const imp = imports.get(name);
      if (imp && isComponent(imp.file, imp.name) && imp.file !== d.file) continue;
      const target = imp && !imp.typeOnly ? `${imp.file}#${imp.name}` : decls.has(`${d.file}#${name}`) && `${d.file}#${name}` !== key ? `${d.file}#${name}` : null;
      if (target && target !== key) for (const a of reach(target, stack)) out.add(a);
    }
    stack.delete(key);
    memo.set(key, out);
    return out;
  };

  const lean = new Set(['everything', 'onObid']);
  const screens: Screen[] = [];
  for (const [path, sf] of byPath) {
    if (path === config.app.apiFile || path.startsWith(`${sharedDir}/`)) continue;
    const imports = importsOf.get(path)!;
    const isMain = config.app.main.some(m => path.startsWith(m));
    const name = path.split('/').pop()!.replace(/\.(tsx?)$/, '');

    // obid calls: direct api.x, and imported functions that reach api
    const live: Screen['live'] = [];
    const seen = new Set<string>();
    const raw = scanRaw(sf);
    const hasJsx = raw.jsx;
    if (imports.get('api')?.file === config.app.apiFile) {
      for (const a of raw.apis) {
        const k = `${a.name}|`;
        if (apiNames.has(a.name) && !seen.has(k)) { seen.add(k); live.push({ api: a.name, route: null, line: a.line, via: null }); }
      }
    }
    const firstUse = new Map<string, number>();
    for (const i of raw.ids) if (!firstUse.has(i.name)) firstUse.set(i.name, i.line);
    const renders = new Set<string>();
    for (const [idName, line] of firstUse) {
      const imp = imports.get(idName);
      if (!imp || imp.typeOnly || imp.file === config.app.apiFile) continue;
      if (isComponent(imp.file, imp.name)) { renders.add(imp.file); continue; }
      for (const a of reach(`${imp.file}#${imp.name}`)) {
        const via = `${imp.file.split('/').pop()} › ${imp.name}`;
        const k = `${a}|${via}`;
        if (!seen.has(k)) { seen.add(k); live.push({ api: a, route: null, line, via }); }
      }
    }

    // mock values shown
    const mock: Screen['mock'] = [];
    for (const imp of sf.getImportDeclarations()) {
      if (resolveSpec(sf, imp.getModuleSpecifierValue())?.endsWith('/mock.ts') && !imp.isTypeOnly()) {
        for (const n of imp.getNamedImports()) if (!n.isTypeOnly()) mock.push({ name: n.getName(), line: n.getStartLineNumber() });
      }
    }

    // choices the app keeps itself
    const local: Screen['local'] = [];
    const sfc = sf.compilerNode;
    for (const c of raw.calls) {
      const callee = c.callee;
      const arg = c.arg;
      let kind: LocalKind | null = null;
      if (/^(window\.)?localStorage\.(getItem|setItem|removeItem)$/.test(callee)) kind = 'localStorage';
      else if (/^(window\.)?sessionStorage\.(getItem|setItem|removeItem)$/.test(callee)) kind = 'sessionStorage';
      else if (callee === 'useKeptSetting') kind = 'useKeptSetting';
      else if (callee === 'useSetting') kind = 'useSetting';
      if (!kind || !arg) continue;
      let key: string | null = null;
      if (ts.isStringLiteral(arg) || ts.isNoSubstitutionTemplateLiteral(arg)) key = arg.text;
      else if (ts.isTemplateExpression(arg)) key = `${arg.head.text}*`;
      else if (ts.isIdentifier(arg)) {
        // const KEY = 'oby.folders'
        const v = sf.getVariableDeclaration(arg.getText(sfc))?.getInitializer();
        if (v && (Node.isStringLiteral(v) || Node.isNoSubstitutionTemplateLiteral(v))) key = v.getLiteralText();
      }
      if (key && !local.some(l => l.key === key && l.kind === kind)) local.push({ key, kind, line: c.line });
    }

    // gates
    const everything: number[] = [];
    const onObid: number[] = [];
    for (const i of raw.ids) {
      if (!lean.has(i.name) || imports.get(i.name)?.file.endsWith('/lean.ts') !== true) continue;
      const list = i.name === 'everything' ? everything : onObid;
      if (!list.includes(i.line)) list.push(i.line);
    }

    // words a person reads
    const words: Screen['words'] = [];
    const addWord = (text: string, line: number) => {
      const t = text.replace(/\s+/g, ' ').trim();
      if (t.length < 2 || !/[A-Za-z]/.test(t) || words.some(w => w.text === t)) return;
      if (words.length < 300) words.push({ text: t, line });
    };
    for (const w of raw.words) addWord(w.text, w.line);

    const first = sf.getStatements()[0];
    const docLine = first?.getLeadingCommentRanges()[0]?.getText().replace(/^\/\*+|\*+\/$|^\/\/\s?/g, '').split('\n')[0].trim() ?? null;
    screens.push({
      file: path, name, kind: isMain ? 'main' : hasJsx ? 'screen' : 'store', lines: sf.getEndLineNumber(), doc: docLine || null,
      live, renders: [...renders].sort(), mock, local, everything: everything.sort((a, b) => a - b), onObid: onObid.sort((a, b) => a - b), words,
    });
  }

  // ---- main process: '/v1/...' literals with the method beside them (notify.ts, device.ts, obid.ts)
  const mainCalls: DesignScan['mainCalls'] = [];
  for (const [path, sf] of byPath) {
    if (!config.app.main.some(m => path.startsWith(m))) continue;
    for (const call of sf.getDescendantsOfKind(SyntaxKind.CallExpression)) {
      const args = call.getArguments();
      for (let i = 0; i < args.length; i++) {
        const p = templatePath(args[i]);
        if (!p?.startsWith('/v1/') || p.length < 5 || !Node.isCallExpression(call) || /\.(startsWith|test|includes)$/.test(call.getExpression().getText())) continue;
        const prev = args[i - 1];
        // call('POST', path, body) names it; call(path, body) with a body is a POST, without one a GET
        const method = prev && Node.isStringLiteral(prev) && /^(GET|POST|PUT|PATCH|DELETE)$/.test(prev.getLiteralText()) ? prev.getLiteralText() : args[i + 1] ? 'POST' : 'GET';
        mainCalls.push({ method, path: p, file: path, line: call.getStartLineNumber() });
      }
    }
  }

  // ---- first-run steps and Control Center scenarios
  const steps: string[] = [];
  const fr = byPath.get(config.app.firstRun)?.getVariableDeclaration('STEPS')?.getInitializerIfKind(SyntaxKind.ArrayLiteralExpression);
  for (const e of fr?.getElements() ?? []) if (Node.isStringLiteral(e)) steps.push(e.getLiteralText());
  const scenarios: DesignScan['scenarios'] = [];
  const sc = byPath.get(config.app.controlCenter)?.getVariableDeclaration('SCENARIOS')?.getInitializerIfKind(SyntaxKind.ArrayLiteralExpression);
  for (const e of sc?.getElements() ?? []) {
    if (!Node.isObjectLiteralExpression(e)) continue;
    const get = (k: string) => { const p = e.getProperty(k); const v = p && Node.isPropertyAssignment(p) ? p.getInitializer() : undefined; return v && Node.isStringLiteral(v) ? v.getLiteralText() : null; };
    const id = get('id'); const label = get('label');
    if (id && label) scenarios.push({ id, label, href: get('href') });
  }

  // ---- obid-wire.ts: each interface / object type and its keys
  const wire: DesignScan['wire'] = [];
  const wf = byPath.get(config.app.wireFile);
  for (const i of wf?.getInterfaces() ?? []) {
    wire.push({ name: i.getName(), file: config.app.wireFile, line: i.getStartLineNumber(), keys: i.getProperties().map(p => ({ key: p.getName(), optional: p.hasQuestionToken() })) });
  }
  for (const t of wf?.getTypeAliases() ?? []) {
    const lit = t.getTypeNode();
    if (!lit || !Node.isTypeLiteral(lit)) continue;
    wire.push({ name: t.getName(), file: config.app.wireFile, line: t.getStartLineNumber(), keys: lit.getProperties().map(p => ({ key: p.getName(), optional: p.hasQuestionToken() })) });
  }

  return { api, screens, mainCalls, steps, scenarios, wire };
}
