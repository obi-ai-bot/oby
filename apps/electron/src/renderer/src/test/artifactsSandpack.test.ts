import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createContext, runInContext } from 'node:vm';
import { describe, expect, it } from 'vitest';
import { SandpackStatic } from 'oby-sandpack-static';
// @ts-expect-error a plain script
import { sandpackConsoleIsolation } from '../../../../scripts/sandpack-isolation.mjs';

// Sandpack's static client's console hook, kept apart from the page (docs/contracts/artifacts-app.md,
// ARTIFACTSAPP-19): LibreChat's plugin (scripts/sandpack-isolation.mjs) on the installed chunk, found
// by name as LibreChat's own test finds it, and the chunk as vitest loads it through vite.config.ts.
const dist = resolve(__dirname, '../../../../node_modules/@codesandbox/sandpack-client/dist');
const named = (re: RegExp) => join(dist, readdirSync(dist).find((name) => re.test(name))!);

describe('ARTIFACTSAPP-19', () => {
  it('artifactsapp_19_the_console_hook_is_wrapped_once_in_sandpacks_static_client', () => {
    const plugin = sandpackConsoleIsolation();
    const id = named(/^index-.*\.mjs$/);
    const source = readFileSync(id, 'utf8');
    expect(plugin.transform.filter.id.test(id)).toBe(true);
    expect(plugin.transform.filter.id.test('/app/artifact.js')).toBe(false);
    const { code } = plugin.transform.handler(source);
    expect(code.split(String.raw`(function () {\n`)).toHaveLength(2);
    expect(code).not.toContain(String.raw`<script>\n      const scope = ".concat(`);
    expect(() => plugin.transform.handler('SandpackStatic.prototype.injectScriptIntoHead = changed;')).toThrow(/Sandpack static script injection changed/);
    expect(plugin.transform.handler('export const value = 1;')).toBeNull();
  });

  it('artifactsapp_19_a_page_declaring_scope_runs_beside_the_console_hook', async () => {
    const inject = (SandpackStatic.prototype as unknown as { injectScriptIntoHead(content: string, opts: { script: string; scope: object }): string }).injectScriptIntoHead;
    expect.soft(inject.toString()).toContain(String.raw`(function () {\n`);
    const { c: consoleHook } = await import(pathToFileURL(named(/^consoleHook-.*\.mjs$/)).href) as { c: string };
    const page = "<!doctype html><html><head></head><body><script>const scope = 'artifact'; let t = 3; class e {} window.ran = [scope, t];</script></body></html>";
    const html = inject.call(SandpackStatic.prototype, page, { script: consoleHook, scope: { channelId: 'x' } });
    // each classic script in document order, sharing one global scope as a page's do
    const context: Record<string, unknown> = { console, setTimeout, parent: { postMessage() {} } };
    context.window = context;
    createContext(context);
    for (const [, body] of html.matchAll(/<script>([\s\S]*?)<\/script>/g)) runInContext(body, context);
    expect(context.ran).toEqual(['artifact', 3]);
  });
});
