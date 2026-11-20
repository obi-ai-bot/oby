// Every board beside its screen, by hand, without obid-spec: starts this app's dev server on a free
// port of 127.0.0.1, drives each board with drive-lab.mjs, prints how far each screen is from its
// board, writes the side-by-side PNGs to the folder given (or none), and stops everything.
//   node scripts/check.mjs [out-folder] [board numbers…]
import { spawn } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';
import { createServer } from 'vite';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const [out, ...only] = process.argv.slice(2);
// Chrome for Testing from obid-spec's own cache (a lab.browser workflow installs it), as ui.rs finds it
const chrome = process.env.OBID_SPEC_CHROME || (() => {
  const dir = join(homedir(), 'Library/Caches/obid-spec/agent-browser-chrome/.agent-browser/browsers');
  const newest = readdirSync(dir).filter((d) => d.startsWith('chrome-')).sort().pop();
  return join(dir, newest, 'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
})();
const server = await createServer({ root, configFile: join(root, 'vite.config.ts'), server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' });
await server.listen();
const port = server.httpServer.address().port;
const helper = spawn('node', [join(here, 'drive-lab.mjs')], { stdio: ['pipe', 'pipe', 'inherit'] });
const lines = createInterface({ input: helper.stdout })[Symbol.asyncIterator]();
const ask = async (r) => { helper.stdin.write(JSON.stringify(r) + '\n'); const { value } = await lines.next(); const a = JSON.parse(value); if (!a.ok) throw new Error(a.error); return a.output; };
const { BOARDS } = await server.ssrLoadModule('/src/boards.tsx').catch(() => ({ BOARDS: null }));
const list = (BOARDS ?? []).map((b) => ({ n: b.n, file: b.file, width: b.width, height: b.height }));
try {
  for (const b of list) {
    if (only.length && !only.includes(String(b.n))) continue;
    await ask({ op: 'open', chrome, url: `http://127.0.0.1:${port}/#/${b.n}` });
    const c = await ask({ op: 'compare', board: b.file });
    console.log(`${String(b.n).padStart(2)} ${b.file.padEnd(18)} worst ${String(c.worst).padStart(6)}px  off ${c.offCount}  counts ${c.counts.length}  words ${c.words.length}  today-off ${c.todayCount}`);
    if (process.env.VERBOSE) console.log(JSON.stringify({ off: c.off.slice(0, 14).map((o) => `${o.piece} by ${o.by}: screen ${JSON.stringify(o.screen)} board ${JSON.stringify(o.board)}`), counts: c.counts, words: c.words.slice(0, 8), today: process.env.VERBOSE === '2' ? c.today.slice(0, 20).map((o) => o.by ? `${o.piece} by ${o.by}: screen ${JSON.stringify(o.screen)} board ${JSON.stringify(o.board)}` : o) : undefined }, null, 1));
    if (out) await ask({ op: 'sidebyside', board: b.file, path: resolve(out, `${String(b.n).padStart(2, '0')}-${b.file}.png`), label: `#/${b.n}` });
  }
} finally {
  await ask({ op: 'close' }).catch(() => {});
  await server.close();
}
