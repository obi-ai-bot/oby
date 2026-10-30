import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['test/**/*.test.ts'],
    // The fixture repository has a test file of its own: the brain must leave it out, vitest must not run it.
    exclude: ['test/fixtures/**', 'node_modules/**'],
  },
});
