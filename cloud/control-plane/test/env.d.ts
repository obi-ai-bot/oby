/// <reference types="@cloudflare/vitest-plugin/types" />
import type { D1Migration } from "cloudflare:test";
import type { Env as WorkerEnv } from "../src/env";

// What `env` from cloudflare:test holds in the tests: the Worker's bindings, the migrations, and a
// second D1 that test/retired.test.ts migrates step by step.
declare global {
  namespace Cloudflare {
    interface Env extends WorkerEnv {
      TEST_MIGRATIONS: D1Migration[];
      MIGRATION_DB: D1Database;
    }
  }
}

export {};
