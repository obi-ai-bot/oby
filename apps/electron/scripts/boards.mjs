// The real window held to the browser boards with no obid-spec (docs/contracts/boards.md, BOARDS-1 to 5):
// each board of boards/browser.json launched hidden through drive-app.mjs (spawned as it is, one helper
// per app), compared with its design board (board-compare.mjs, OBY_BOARDS_DIR for another design
// folder) and pictured beside it. Run from apps/electron, after npm ci and npm run build:
//   node scripts/boards.mjs <out-folder> [board numbers…]
// Exit 0 when every check holds; 1 with one line per miss, the scratch home kept.
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { connect, createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { appRoot, electronBinary, sleep } from './drive-lib.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const BUILD = 'run npm ci and npm run build in apps/electron';
const PORT = 'unset OBY_BOARDS_PORT or name a free port other than 7777';

/** The boards, in order: each with its checks, or a gap saying why it has none. */
export function loadBoards(file = join(HERE, 'boards/browser.json')) {
  const list = JSON.parse(readFileSync(file, 'utf8'));
  for (const b of list) if (!b.checks?.length && !b.gap) throw new Error(`board ${b.n} ${b.board}: neither checks nor a gap`);
  return list;
}

/** The boards the run's arguments name, by number (`01` is board 1); all of them with none. An argument that names no board is refused. */
export function pick(list, only) {
  for (const arg of only) if (!list.some((b) => Number(arg) === b.n)) throw new Error(`no board ${arg}`);
  return only.length ? list.filter((b) => only.some((arg) => Number(arg) === b.n)) : list;
}

const listens = (port) => new Promise((done) => {
  const s = connect({ host: '127.0.0.1', port });
  s.once('connect', () => { s.destroy(); done(true); });
  s.once('error', () => done(false));
});

/** What stops a run, each with what to do: none when the build is there and current and the port free. */
export async function ready({ root = appRoot, port }) {
  const problems = [];
  const out = ['out/main/index.js', 'out/renderer/index.html'].map((f) => join(root, f));
  for (const f of [...out, join(root, relative(appRoot, electronBinary))]) if (!existsSync(f)) problems.push(`no ${relative(root, f)}: ${BUILD}`);
  if (!problems.length) {
    const built = Math.min(...out.map((f) => statSync(f).mtimeMs));
    const src = join(root, 'src');
    // vitest keeps its cache in src/renderer/node_modules: not the app's source
    const newer = readdirSync(src, { recursive: true }).filter((f) => !f.split(sep).includes('node_modules')).map((f) => join(src, f))
      .find((f) => statSync(f).isFile() && statSync(f).mtimeMs > built);
    if (newer) problems.push(`out/ is older than ${relative(root, newer)}: ${BUILD}`);
  }
  if (!Number.isInteger(port) || port < 1 || port > 65535) problems.push(`port ${port} isn't a port: ${PORT}`);
  else if (port === 7777) problems.push(`port 7777 is the owner's obid: ${PORT}`);
  else if (await listens(port)) problems.push(`port ${port} is busy, something listens on 127.0.0.1:${port}: ${PORT}`);
  return problems;
}

const box = (b) => `x ${b.x} y ${b.y} w ${b.w} h ${b.h}`;

/** The lines one check prints: none when it holds. */
export function misses(entry, check, result) {
  const head = `board ${entry.n} ${entry.board}${check.state ? ` (${check.state})` : ''}:`;
  if (result.error) return [`${head} ${result.error}`];
  return [
    ...(result.nothingCompared ? [`${head} nothing compared`] : []),
    ...result.off.map((o) => `${head} ${o.piece} ${o.by}px off: app ${box(o.app)}, board ${box(o.board)}`),
    ...result.words.map((w) => `${head} ${w.piece} words differ: app "${w.app}", board "${w.board}"`),
    ...result.counts.map((c) => `${head} ${c.piece} count differs: app ${c.app}, board ${c.board}`),
  ];
}

// A free port of 127.0.0.1, as obid-spec's idle_port() takes one: bound, read, let go
const idlePort = () => new Promise((done) => {
  const s = createServer().listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => done(port)); });
});

// One drive-app.mjs helper: a request a line in, an answer a line out (an `ok: false` thrown)
function helper() {
  const child = spawn(process.execPath, [join(HERE, 'drive-app.mjs')], { env: process.env, stdio: ['pipe', 'pipe', 'inherit'] });
  const exited = new Promise((done) => child.once('exit', done));
  child.stdin.on('error', () => {});
  const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const ask = async (r) => {
    child.stdin.write(`${JSON.stringify(r)}\n`);
    const { value, done } = await lines.next();
    if (done) throw new Error(`drive-app.mjs ended before answering ${r.op}`);
    const a = JSON.parse(value);
    if (!a.ok) throw new Error(`${r.op}: ${a.error}`);
    return a.output;
  };
  return { ask, end: () => { child.stdin.end(); return exited; } };
}

// Asked again for up to 10 s, until `good` holds; the last answer kept
async function within(ask, r, good) {
  const deadline = Date.now() + 10_000;
  for (;;) {
    const a = await ask(r);
    if (good(a) || Date.now() >= deadline) return a;
    await sleep(250);
  }
}

async function steps(ask, before = []) {
  for (const s of before) {
    if (!s.wait) { await ask(s); continue; }
    if ((await within(ask, { op: 'read', target: s.wait }, (a) => a.count >= 1)).count < 1) throw new Error(`waited 10 s for ${JSON.stringify(s.wait)}`);
  }
}

async function run(out, boards) {
  const port = process.env.OBY_BOARDS_PORT ? Number(process.env.OBY_BOARDS_PORT) : await idlePort();
  const problems = await ready({ port });
  if (problems.length) {
    for (const p of problems) console.error(p);
    return 1;
  }
  const home = mkdtempSync(join(tmpdir(), 'oby-boards-'));
  const obid = { port, home: join(home, 'no-obid') };
  mkdirSync(obid.home);
  const checks = [], skipped = [], noCheck = [];
  for (const entry of boards) {
    const nn = String(entry.n).padStart(2, '0');
    const scratch = join(home, nn);
    mkdirSync(scratch);
    const { ask, end } = helper();
    const name = (state) => `${nn}-${entry.board}${state ? `-${state}` : ''}`;
    const picture = (state) => ask({ op: 'sidebyside', path: join(out, `${name(state)}-beside.png`), board: entry.board, state, label: `board ${entry.n}${state ? `, ${state}` : ''}` });
    const judged = new Map();
    let error;
    try {
      await ask({ op: 'launch', mock: true, query: entry.query, size: entry.size, storage: entry.storage, scratch, userData: join(scratch, 'userData'), obid });
      if (entry.gap) {
        await steps(ask, entry.before);
        await picture();
      }
      for (const check of entry.checks ?? []) {
        await steps(ask, check.before);
        const result = await within(ask, { op: 'compare', board: entry.board, state: check.state, tolerance: check.tolerance, skip: check.skip ?? [] }, (a) => a.within && a.sameWords);
        await picture(check.state);
        judged.set(check, result);
      }
    } catch (e) { error = e.message; }
    const { pageErrors } = await ask({ op: 'close' }).catch((e) => ({ pageErrors: [`close: ${e.message}`] }));
    await end();
    if (entry.gap) {
      noCheck.push({ n: entry.n, board: entry.board, gap: entry.gap, ...(error ? { error } : {}) });
      writeFileSync(join(out, `${name()}.json`), JSON.stringify({ n: entry.n, board: entry.board, gap: entry.gap, ...(error ? { error } : {}), pageErrors }, null, 2));
      if (error) checks.push({ n: entry.n, board: entry.board, pass: false, misses: misses(entry, {}, { error }) });
    }
    // a check the failure reached, or one after it, fails with it
    for (const check of entry.checks ?? []) {
      const result = judged.get(check) ?? { error };
      const lines = misses(entry, check, result);
      writeFileSync(join(out, `${name(check.state)}.json`), JSON.stringify({ n: entry.n, board: entry.board, state: check.state ?? null, skip: check.skip ?? [], ...(result.error ? { error: result.error } : { result }), pageErrors }, null, 2));
      checks.push({ n: entry.n, board: entry.board, state: check.state ?? null, pass: lines.length === 0, misses: lines });
      if (check.skip) skipped.push({ n: entry.n, board: entry.board, state: check.state ?? null, pieces: check.skip });
    }
  }
  writeFileSync(join(out, 'result.json'), JSON.stringify({ port, home, checks, gaps: { skipped, noCheck } }, null, 2));
  for (const c of checks) {
    if (c.pass) console.log(`board ${c.n} ${c.board}${c.state ? ` (${c.state})` : ''}: holds`);
    else for (const line of c.misses) console.log(line);
  }
  for (const s of skipped) console.log(`known gap: board ${s.n} ${s.board} skips ${s.pieces.join(', ')}`);
  for (const g of noCheck) console.log(`known gap: board ${g.n} ${g.board} has no check: ${g.gap}`);
  if (checks.every((c) => c.pass)) {
    rmSync(home, { recursive: true, force: true });
    return 0;
  }
  console.log(`scratch home kept: ${home}`);
  return 1;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [out, ...only] = process.argv.slice(2);
  if (!out) {
    console.error('usage: node scripts/boards.mjs <out-folder> [board numbers…]');
    process.exit(1);
  }
  let boards;
  try { boards = pick(loadBoards(), only); } catch (e) {
    console.error(`${e.message}: board numbers are 1 to 17 (scripts/boards/browser.json)`);
    console.error('usage: node scripts/boards.mjs <out-folder> [board numbers…]');
    process.exit(1);
  }
  mkdirSync(resolve(out), { recursive: true });
  process.exit(await run(resolve(out), boards));
}
