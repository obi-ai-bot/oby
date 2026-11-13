import { fileURLToPath } from "node:url";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";

// The tests run inside workerd (Miniflare) with a real, empty D1 that test/setup.ts migrates, and
// a second one (MIGRATION_DB) that test/retired.test.ts migrates itself; APNs is a fake the tests
// pass in (src/app.ts, `handle`).
export default defineConfig(async () => {
  const migrations = await readD1Migrations(fileURLToPath(new URL("migrations", import.meta.url)));
  return {
    plugins: [
      cloudflareTest({
        wrangler: { configPath: "./wrangler.jsonc" },
        // The tests' own instance: wrangler.jsonc's top level is a template with nothing set.
        miniflare: {
          bindings: {
            TEST_MIGRATIONS: migrations,
            APNS_TOPICS: "com.deskree.oby.ios",
          },
          d1Databases: { MIGRATION_DB: "oby-control-migration-test" },
        },
      }),
    ],
    test: { setupFiles: ["./test/setup.ts"] },
  };
});
