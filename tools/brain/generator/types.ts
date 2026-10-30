/**
 * The Brain's JSON (served at /brain.json): obid read by `obid-extract` (Rust, syn), the Electron app read by
 * `design.ts` (ts-morph), and the checks that join them. The interface reads only this.
 */

export interface Source { file: string; line: number }

/** Who may call an obid route: the owner's token on loopback, an agent's job token, a paired phone, or no one's. */
export type AccessKind = 'owner' | 'agent' | 'phone' | 'pairing' | 'public';

// ---- obid ------------------------------------------------------------------------------------------------------------

export interface RouteError { status: string; message: string; line: number }

export interface Route {
  /** "GET /v1/jobs/{id}" */
  id: string;
  method: string;
  path: string;
  /** The module whose router registers it: api, gateway, connections, tetrix. */
  router: string;
  source: Source;
  handler: string;
  handlerSource: Source | null;
  handlerDoc: string | null;
  extractors: string[];
  errors: RouteError[];
  access: AccessKind[];
  /** A paired phone may call it through the gateway (gateway::ROUTES). */
  phone: boolean;
  /** Mentioned as `METHOD /path` in docs/runner-api.md. */
  documented: boolean;
  /** Where the app calls it: renderer api methods and main-process literals. */
  callers: Caller[];
}

export interface Caller { via: string; source: Source }

export interface PhoneRoute { method: string; pattern: string; line: number; matches: string[] }

export interface Area {
  id: string;
  title: string;
  summary: string;
  /** Domain areas may not import adapters (HTTP, processes, files, network). */
  domain: boolean;
  modules: string[];
  lines: number;
}

export interface Module {
  id: string;
  file: string;
  lines: number;
  area: string | null;
  uses: string[];
  usedBy: string[];
  external: string[];
}

export interface Spawn { source: Source; kind: string; inFn: string | null; inFnLine: number | null; doc: string | null; module: string }

export interface StoredFile { name: string; sites: Array<Source & { inFn: string | null }> }

export interface WireField { key: string; optional: boolean }

export interface WireType {
  name: string;
  source: Source;
  keys: WireField[];
  rust: { name: string; source: Source; keys: WireField[] } | null;
  /** In obid-wire.ts, not in the Rust struct (often keys obid adds with json!). */
  onlyTs: string[];
  /** In the Rust struct, not declared in obid-wire.ts. */
  onlyRust: string[];
}

export interface RuleLink { label: string; kind: 'code' | 'test'; file: string; symbol: string; source: Source | null }

export interface Rule {
  id: string;
  title: string;
  /** The rule's words, as AGENTS.md says it. */
  text: string;
  links: RuleLink[];
}

// ---- the Electron app (Sangam's design) ------------------------------------------------------------------------------

export type LocalKind = 'localStorage' | 'useKeptSetting' | 'useSetting' | 'sessionStorage';

export interface ApiMethod { name: string; method: string; path: string; source: Source; route: string | null }

export interface ScreenUse<T extends string = string> { name: T; line: number; detail?: string }

export interface Screen {
  /** apps/electron/src/renderer/src/Home.tsx */
  file: string;
  name: string;
  kind: 'screen' | 'store' | 'main';
  lines: number;
  doc: string | null;
  /** obid calls: api.x used here, directly or through the stores it imports (via). */
  live: Array<{ api: string; route: string | null; line: number; via: string | null }>;
  /** The screens it renders (components imported from other .tsx files). */
  renders: string[];
  /** Values imported from mock.ts. */
  mock: ScreenUse[];
  /** Choices kept in the app itself. */
  local: Array<{ key: string; kind: LocalKind; line: number }>;
  /** Lines gated on ?everything (hidden in v1) and on obid being there. */
  everything: number[];
  onObid: number[];
  /** What a person reads on it. */
  words: Array<{ text: string; line: number }>;
}

export interface LocalKey {
  key: string;
  kind: LocalKind;
  sites: Source[];
  /** "app": a per-viewer convenience that belongs in the app; "obid": obid owes it (issue); null: not classified yet. */
  owner: 'app' | 'obid' | null;
  issue: number | null;
  note: string | null;
}

export interface JumpState { label: string; group: string; url: string; note: string | null }

// ---- checks ----------------------------------------------------------------------------------------------------------

export type CheckKind =
  | 'route-undocumented' | 'doc-without-route' | 'phone-without-route' | 'call-without-route' | 'route-unused'
  | 'local-unclassified' | 'local-owed' | 'layering' | 'internal-host' | 'rule-unlinked' | 'wire' | 'extract';

export interface Check { kind: CheckKind; message: string; source?: Source; ref?: string }

export interface BrainData {
  name: string;
  root: string;
  generatedAt: string;
  durationMs: number;
  git: { branch: string; commit: string; dirty: boolean } | null;
  stats: Record<string, number>;
  routes: Route[];
  phoneRoutes: PhoneRoute[];
  docRoutes: Array<{ id: string; line: number; exists: boolean }>;
  areas: Area[];
  modules: Module[];
  cycles: string[][];
  spawns: Spawn[];
  storedFiles: StoredFile[];
  wire: WireType[];
  rules: Rule[];
  api: ApiMethod[];
  screens: Screen[];
  localKeys: LocalKey[];
  states: JumpState[];
  checks: Check[];
}
