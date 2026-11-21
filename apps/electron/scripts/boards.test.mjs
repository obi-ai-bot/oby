// boards.mjs and board-compare.mjs, held to docs/contracts/boards.md (BOARDS-1 to 5): node's own runner,
// never the app or the real out/.
//   node --test scripts/boards.test.mjs
import { after, before, test } from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const scratch = mkdtempSync(join(tmpdir(), 'oby-boards-test-'));
const design = join(scratch, 'design');
let compare, boards, lib;

before(async () => {
  cpSync(resolve(HERE, '../../../tools/obid-spec/lab-ui/browser/design'), design, { recursive: true });
  const main = join(design, 'boards/Main.dc.html');
  writeFileSync(main, readFileSync(main, 'utf8').replace('is using this tab', 'is driving this tab'));
  process.env.OBY_BOARDS_DIR = design;
  compare = await import('./board-compare.mjs');
  lib = await import('./drive-lib.mjs');
});
after(() => rmSync(scratch, { recursive: true, force: true }));
// boards.mjs is imported in each test, so one that doesn't need it shows its own failure
const load = async () => (boards ??= await import('./boards.mjs'));

// every piece empty on both sides but the ones given
const maps = (filled = {}) => Object.fromEntries([...compare.PIECES, ...Object.values(compare.HELD).flat()].map((p) => [p, filled[p] ?? []]));
const freePort = () => new Promise((done) => {
  const s = createServer().listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => done(port)); });
});

test('BOARDS-1: browser.json names the 17 boards, each a design board at its size, with today\'s skips', async () => {
  const list = (await load()).loadBoards();
  assert.deepEqual(list.map((b) => b.n), Array.from({ length: 17 }, (_, i) => i + 1));
  const skips = Object.fromEntries([...readFileSync(join(HERE, 'browser-workflows.py'), 'utf8').split('BOARD_SKIPS = {')[1].split('\n}')[0]
    .matchAll(/^\s*(\d+): (\[.*\]),$/gm)].map(([, n, l]) => [n, JSON.parse(l)]));
  let total = 0;
  for (const b of list) {
    assert.equal(b.query, `browser=${b.n}`);
    assert.deepEqual(b.storage, { 'oby.chatPanelOpen': [3, 13].includes(b.n) ? '1' : '0' });
    const root = readFileSync(join(design, 'boards', `${b.board}.dc.html`), 'utf8').match(/<div class="oby [^"]*" style="width: (\d+)px; height: (\d+)px/);
    assert.ok(root, `board ${b.n} ${b.board}: no root <div class="oby …">`);
    assert.deepEqual(b.size, { width: Number(root[1]), height: Number(root[2]) }, `board ${b.n} ${b.board}`);
    assert.equal(!!b.gap, [9, 10, 13, 15].includes(b.n), `board ${b.n}: a gap only on 9, 10, 13 and 15`);
    if (b.gap) assert.equal(b.checks, undefined);
    else assert.ok(b.checks.length > 0);
    for (const c of b.checks ?? []) {
      assert.equal(c.tolerance, 4);
      assert.deepEqual(c.skip, skips[b.n], `board ${b.n}: its skip is BOARD_SKIPS[${b.n}]`);
    }
    total += b.checks?.length ?? 1;
  }
  assert.deepEqual(list[5].checks.map((c) => c.state), ['held', 'gaveBack']);
  assert.equal(total, 18);
});

test('BOARDS-2: a held piece\'s words differ, and the line names the board, the piece and both words', async () => {
  const at = { x: 660, y: 92, width: 420, height: 40 };
  const r = compare.judge('Main', maps({ '.br-control-text': [{ ...at, text: 'Codex is using this tab' }] }),
    maps({ '.br-control-text': [{ ...at, text: 'Codex is driving this tab' }] }));
  assert.equal(r.sameWords, false);
  assert.equal(r.words[0].piece, '.br-control-text[0]');
  const lines = (await load()).misses({ n: 2, board: 'Main' }, {}, r);
  assert.equal(lines.length, 1);
  for (const w of ['2', 'Main', '.br-control-text[0]', 'Codex is using this tab', 'Codex is driving this tab']) assert.ok(lines[0].includes(w), `${w} in ${lines[0]}`);
});

test('BOARDS-3: a held piece 6px off at tolerance 4, and the line names the piece, the pixels and both boxes', async () => {
  const r = compare.judge('Main', maps({ '.br-tab': [{ x: 706, y: 12, width: 180, height: 36, text: 'Sign in' }] }),
    maps({ '.br-tab': [{ x: 700, y: 12, width: 180, height: 36, text: 'Sign in' }] }), { tolerance: 4 });
  assert.equal(r.within, false);
  assert.equal(r.off[0].by, 6);
  const [line] = (await load()).misses({ n: 2, board: 'Main' }, {}, r);
  for (const w of ['board 2 Main', '.br-tab[0]', '6px', 'x 706 y 12 w 180 h 36', 'x 700 y 12 w 180 h 36']) assert.ok(line.includes(w), `${w} in ${line}`);
});

test('BOARDS-4: nothing compared fails, as "nothing compared"', async () => {
  const r = compare.judge('Closed', maps(), maps());
  assert.equal(r.compared, 0);
  assert.equal(r.within, false);
  assert.equal(r.nothingCompared, true);
  const lines = (await load()).misses({ n: 1, board: 'Closed' }, {}, r);
  assert.ok(lines.some((l) => l.includes('nothing compared')), lines.join('\n'));
});

test('BOARDS-4: board 1 holds the globe\'s tip, and no other board does', () => {
  const tip = { '.tip-under': [{ x: 1120, y: 92, width: 140, height: 26, text: 'Show the browser⇧⌘B' }] };
  assert.equal(compare.judge('Closed', maps(tip), maps(tip)).compared, 1);
  assert.equal(compare.judge('Main', maps(tip), maps(tip)).compared, 0);
});

test('BOARDS-5: no build, a stale build, port 7777 or a busy port each say what to do', async () => {
  const { ready } = await load();
  const root = join(scratch, 'app');
  const binary = join(root, relative(lib.appRoot, lib.electronBinary));
  const files = ['out/main/index.js', 'out/renderer/index.html', 'src/main/index.ts', 'src/renderer/node_modules/.vite/results.json'].map((f) => join(root, f));
  for (const f of [binary, ...files]) { mkdirSync(dirname(f), { recursive: true }); writeFileSync(f, ''); }
  const build = (problems) => problems.some((p) => p.includes('npm ci') && p.includes('npm run build'));
  const port = await freePort();
  const old = new Date(Date.now() - 60_000);
  utimesSync(files[2], old, old);
  // vitest's cache, written after the build, isn't the app's source
  utimesSync(files[3], new Date(Date.now() + 60_000), new Date(Date.now() + 60_000));
  assert.deepEqual(await ready({ root, port }), []);

  rmSync(files[0]);
  assert.ok(build(await ready({ root, port })), 'no out/main/index.js');
  writeFileSync(files[0], '');
  utimesSync(files[2], new Date(Date.now() + 60_000), new Date(Date.now() + 60_000));
  assert.ok(build(await ready({ root, port })), 'a file under src/ newer than out/');
  utimesSync(files[2], old, old);

  const notPort = await ready({ root, port: Number('ten') });
  assert.ok(notPort.some((p) => p.includes('OBY_BOARDS_PORT')), notPort.join('\n'));
  const owner = await ready({ root, port: 7777 });
  assert.ok(owner.some((p) => p.includes('7777') && p.includes('OBY_BOARDS_PORT')), owner.join('\n'));
  const server = createServer().listen(0, '127.0.0.1');
  await new Promise((done) => server.once('listening', done));
  try {
    const busy = await ready({ root, port: server.address().port });
    assert.ok(busy.some((p) => p.includes(String(server.address().port)) && p.includes('OBY_BOARDS_PORT')), busy.join('\n'));
  } finally { server.close(); }
});

test('BOARDS-5: board numbers name boards by number, and one that names none is refused', async () => {
  const { loadBoards, pick } = await load();
  const list = loadBoards();
  assert.deepEqual(pick(list, ['01']).map((b) => b.n), [1]);
  assert.deepEqual(pick(list, ['6', '10']).map((b) => b.n), [6, 10]);
  assert.equal(pick(list, []).length, 17);
  for (const arg of ['18', 'O1', '']) assert.throws(() => pick(list, [arg]), new RegExp(`no board ${arg}`));
});

test('OBY_BOARDS_DIR: the design folder is the one named, its boards and its base', () => {
  assert.equal(compare.DESIGN, design);
  const url = compare.boardFile('Main', undefined, join(scratch, 'pages'));
  const page = readFileSync(fileURLToPath(url), 'utf8');
  assert.ok(page.includes('is driving this tab'));
  assert.ok(page.includes(`<base href="${pathToFileURL(`${design}/`).href}">`));
});
