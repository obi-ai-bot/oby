// MIT License
//
// Copyright (c) 2026 LibreChat
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

// From danny-avila/LibreChat, client/src/lib/sandpack.ts at commit 4700c44, its TypeScript taken out
// (docs/contracts/artifacts-app.md, ARTIFACTSAPP-19): Sandpack's static client's console hook, put
// into every page, runs inside its own function, so a page may declare `scope` and the hook's names.

const injection = String.raw`var scriptToInsert = "\n    <script>\n      const scope = ".concat(JSON.stringify(scope), ";\n      ").concat(script, "\n    </script>\n    ").trim();`;
const scopedInjection = String.raw`var scriptToInsert = "\n    <script>\n      (function () {\n      const scope = ".concat(JSON.stringify(scope), ";\n      ").concat(script, "\n      })();\n    </script>\n    ").trim();`;

/** Sandpack 2.19.8 leaks console-hook bindings into the artifact's global scope. */
export function sandpackConsoleIsolation() {
  return {
    name: 'sandpack-console-isolation',
    transform: {
      filter: {
        id: /[\\/]node_modules[\\/]@codesandbox[\\/]sandpack-client[\\/]dist[\\/]index-[^\\/]+\.(?:mjs|js)(?:\?|$)/,
      },
      handler(code) {
        if (!code.includes('SandpackStatic.prototype.injectScriptIntoHead')) {
          return null;
        }
        // Fail on dependency drift instead of silently restoring collisions.
        if (code.split(injection).length !== 2) {
          throw new Error('Sandpack static script injection changed; update console isolation.');
        }
        return { code: code.replace(injection, scopedInjection), map: null };
      },
    },
  };
}
