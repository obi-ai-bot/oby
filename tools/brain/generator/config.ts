/**
 * What the Brain needs to know about Oby beyond its code: where things live, which modules make up each area, which
 * of the app's kept-locally choices obid owes (and in which issue), and which code keeps each of AGENTS.md's rules.
 * Everything else is read from the code; when the code moves away from this file, a check says so.
 */
export interface BrainConfig {
  name: string;
  /** Absolute path of the repository. */
  root: string;
  obid: {
    /** The Rust extractor's Cargo project (relative to root). */
    extractor: string;
    /** obid's crate, for the file watcher. */
    crate: string;
    /** The API doc whose `METHOD /path` mentions count as documenting a route. */
    apiDoc: string;
  };
  app: {
    /** The renderer (Sangam's screens), main and preload, relative to root. */
    renderer: string;
    main: string[];
    /** The file with `export const api = { … }`. */
    apiFile: string;
    /** The hand-written wire types. */
    wireFile: string;
    /** obid-wire.ts type name → Rust struct name, where they differ. */
    wireRust: Record<string, string>;
    /** Where the renderer runs on the mock, for the state links (`npm run web` on oby-app-web's port). */
    webUrl: string;
    /** The file whose `const STEPS = [...]` lists the first run's steps (`?onboarding=<step>`). */
    firstRun: string;
    /** The Control Center file, whose `SCENARIOS` are `?dev=<id>` states. */
    controlCenter: string;
    flags: Array<{ flag: string; label: string; note?: string }>;
    /** State plumbing not to follow (`file#name`): setting state reaches every subscriber, which says nothing about a screen. */
    stopAt: string[];
  };
  /** Each area of obid's code (docs/runner-api.md and the 2026-10-29 plan) and the top-level modules in it. */
  areas: Array<{ id: string; title: string; summary: string; domain?: boolean; modules: string[] }>;
  /** Choices the app keeps itself: who owns each. A key matches exactly, or a prefix ending in `*`. */
  local: Array<{ key: string; owner: 'app' | 'obid'; issue?: number; note?: string }>;
  /** AGENTS.md's "Rules Oby never breaks", each tied to the code that keeps it and the tests that prove it. */
  rules: Array<{ id: string; title: string; text: string; code: string[]; tests: string[] }>;
  /** Internal hosts that must never be in a literal in the open-source code (the 2026-10-28 call). */
  internalHosts: string[];
  /** Folders scanned for internal hosts, relative to root. */
  hostScan: string[];
}
