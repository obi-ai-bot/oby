// Real Electron → preload → main/device → signed HTTP → real obid regression for what an older
// release's #1563 limit decisions leave (docs/contracts/account-limits.md, LIMITS-25, LIMITS-26):
// obid settles them when it starts, so no limit card shows; a kept Stop stays, taken on only by
// the owner's signed words. Uses only stand-in agents and an isolated runner home. Keep Electron's HOME: replacing it
// makes macOS safeStorage unavailable even with a separate userData folder.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { _electron as electron } from 'playwright-core';
import { buildFixture } from '../../../e2e/lib/fixture.mjs';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(appRoot, '../..');
const agents = join(repoRoot, 'core/crates/obid/tests/agents');
const binary = process.env.OBY_TEST_OBID || join(repoRoot, 'core/target/debug/obid');
const electronBinary = join(appRoot, 'node_modules/electron/dist/Electron.app/Contents/MacOS/Electron');
const scratch = mkdtempSync(join(tmpdir(), 'oby-electron-limits-'));
const processes = new Set();
const apps = new Set();
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const now = () => Math.floor(Date.now() / 1000);
const json = (path, value) => writeFileSync(path, JSON.stringify(value), { mode: 0o600 });
const read = (path) => JSON.parse(readFileSync(path, 'utf8'));

// the Tasks cards, Home's chip's: hidden chat first (docs/contracts/chat-first.md), so as the control center opens a page
async function openTasks(page) {
  await page.evaluate(() => dispatchEvent(new CustomEvent('oby:open', { cancelable: true, detail: { kind: 'group', id: 'tasks', tab: 'running' } })));
}

async function until(label, f, ms = 45_000) {
  const deadline = Date.now() + ms;
  let last;
  while (Date.now() < deadline) {
    try { const value = await f(); if (value) return value; } catch (e) { last = e; }
    await sleep(100);
  }
  throw new Error(`${label} timed out${last ? `: ${last.message}` : ''}`);
}

async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  const { port } = server.address();
  await new Promise((done) => server.close(done));
  return port;
}

function seededJob(id, reset) {
  return {
    id, request: 'Look around', task: 'Look around', title: `Task ${id}`, agent: 'claude', account: 'stub',
    repo: 'obi-e2e/fixture-app', branch: `oby/research-${id}`, kind: 'research', status: 'interrupted',
    step: '', summary: '', created: now() - 120, started: now() - 120, round: 1, session: `${id}-0000-4000-8000-000000000000`, reason: 'Usage limit reached',
    accountLimitBlocked: true, accountLimitNonce: `nonce-${id}`, accountLimitReset: reset,
  };
}

// An older release's `home/limit-decisions.json`: one decision for both seeded jobs, none applied.
function olderDecision(home, choice) {
  json(join(home, 'limit-decisions.json'), [{ id: `d-${choice}`, revision: 3, agent: 'claude', account: 'stub',
    resetAt: now() + 3600, createdAt: now() - 600, jobs: ['aaaa0001', 'aaaa0002'],
    nonces: { aaaa0001: 'nonce-aaaa0001', aaaa0002: 'nonce-aaaa0002' }, choice,
    destination: choice === 'move' ? 'codex' : null, snapshot: null, applied: [] }]);
}

function room() {
  return { fiveHour: 0.1, sevenDay: 0.2, fiveHourResets: now() + 3600, sevenDayResets: now() + 86400,
    fiveHourObservedAt: now(), sevenDayObservedAt: now() };
}

async function setup(name, { choice = 'pending', available = false, reset = now() + 3600 } = {}) {
  const root = join(scratch, name), home = join(root, 'home');
  for (const dir of ['home/jobs', 'home/accounts/stub', 'home/accounts/codex', 'env-home', 'tmp', 'electron-data']) mkdirSync(join(root, dir), { recursive: true });
  const fixture = buildFixture(join(root, 'fixture'));
  json(join(home, 'accounts/stub/.credentials.json'), { claudeAiOauth: { accessToken: 'fake-claude', expiresAt: (now() + 86400) * 1000 } });
  json(join(home, 'accounts/codex/auth.json'), { OPENAI_API_KEY: 'fake-codex' });
  for (const id of ['aaaa0001', 'aaaa0002']) json(join(home, `jobs/${id}.json`), seededJob(id, reset));
  olderDecision(home, choice);
  json(join(home, 'usage.json'), available ? { codex: room() } : (reset < now() ? { stub: room() } : {}));
  const port = await freePort();
  const config = {
    listen: `127.0.0.1:${port}`, home, owner: 'wellington',
    claudeKeychain: false, cursorKeychain: false, allowFileRemotes: true,
    agents: {
      claude: { bin: join(agents, 'fake-claude'), accounts: [{ id: 'stub', name: 'Claude source', configDir: join(home, 'accounts/stub') }] },
      ...(available ? { codex: { bin: join(agents, 'fake-codex'), accounts: [{ id: 'codex', name: 'Codex destination', configDir: join(home, 'accounts/codex') }] } } : {}),
    },
    repos: [{ repo: 'obi-e2e/fixture-app', remote: fixture.remoteUrl, defaultBranch: 'main', check: 'node --test', pullRequests: true }],
  };
  json(join(root, 'config.json'), config);
  const logs = [];
  const daemon = spawn(binary, ['serve', '--config', join(root, 'config.json')], { env: {
    PATH: `${agents}:${process.env.PATH}`, HOME: join(root, 'env-home'), TMPDIR: join(root, 'tmp'),
    OBID_SECURITY_BIN: join(agents, 'fake-security'), OBID_PRESTART: 'off', OBID_USAGE_READS: 'off',
    OBID_LOAD: '0', OBID_MEMORY_AVAILABLE: '1099511627776',
  }, stdio: ['ignore', 'pipe', 'pipe'] });
  processes.add(daemon);
  daemon.stdout.on('data', (b) => logs.push(b.toString()));
  daemon.stderr.on('data', (b) => logs.push(b.toString()));
  const call = async (method, path, body) => {
    const token = readFileSync(join(home, 'api-token'), 'utf8').trim();
    const response = await fetch(`http://127.0.0.1:${port}${path}`, {
      method, headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const value = await response.json();
    assert.ok(response.ok, `${response.status}: ${JSON.stringify(value)}`);
    return value;
  };
  await until('scratch obid startup', () => call('GET', '/v1/status'));
  return { root, home, port, call, daemon, logs };
}

async function openApp(s) {
  // Set userData before ready or any key read; retain the real HOME for the OS Keychain.
  const launcher = join(s.root, 'electron-launcher.cjs');
  writeFileSync(launcher, `const { app } = require('electron');
app.setPath('userData', ${JSON.stringify(join(s.root, 'electron-data'))});
import(${JSON.stringify(pathToFileURL(join(appRoot, 'out/main/index.js')).href)});
`);
  const app = await electron.launch({ executablePath: electronBinary, args: [launcher], env: {
    ...process.env, OBID_HOME: s.home, OBID_PORT: String(s.port),
  } });
  apps.add(app);
  const page = await app.firstWindow();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.waitForLoadState('domcontentloaded');
  // set up already (FirstRun.tsx), so Home shows rather than the first run's screens
  await page.evaluate(() => { for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1'); });
  await page.reload();
  await page.waitForLoadState('domcontentloaded');
  await page.keyboard.press('Escape');
  return { app, page, errors };
}

// No limit card in Home's stack (AskStack), on top or behind another: obid lists no decision
// (LIMITS-25), so the app has none to show. Given the app's first reads time to land.
async function noLimitCard(page, s) {
  assert.deepEqual(await s.call('GET', '/v1/limit-decisions'), []);
  await page.waitForTimeout(3000);
  assert.equal(await page.locator('.tc-ask-kind.is-limit').count(), 0, 'no limit card');
}

// The older file, settled, is gone (LIMITS-26).
const settled = (s) => assert.equal(existsSync(join(s.home, 'limit-decisions.json')), false, 'the older file goes once its jobs are saved');

async function close(s, app) {
  if (app) { await app.close(); apps.delete(app); }
  s.daemon.kill('SIGINT');
  await until('scratch obid shutdown', () => s.daemon.exitCode != null || s.daemon.signalCode != null, 10_000);
  processes.delete(s.daemon);
  writeFileSync(join(s.root, 'obid.log'), s.logs.join(''));
}

async function noRepeatedInterruption(page) {
  await openTasks(page);
  for (const tab of ['Running', 'Completed']) {
    await page.getByRole('tab', { name: new RegExp(`^${tab}`) }).click();
    assert.equal(await page.locator('.group-layer:not(.is-leaving) .tc-ask-head').filter({ hasText: /^It was interrupted$/ }).count(), 0,
      'a settled account-wide choice must not become a per-job interruption question');
  }
  await page.keyboard.press('Escape');
  await page.locator('.group-layer').waitFor({ state: 'detached' });
}

try {
  assert.equal(process.platform, 'darwin', 'this real Keychain/Electron regression runs on macOS');
  assert.ok(existsSync(binary), 'build obid first: cargo build --manifest-path ../../core/Cargo.toml -p obid');
  assert.ok(existsSync(join(appRoot, 'out/main/index.js')), 'build Electron first: npm run build');

  {
    const s = await setup('kept-stop', { choice: 'stop' });
    const { app, page, errors } = await openApp(s);
    try {
      await noLimitCard(page, s);
      settled(s);
      for (const id of ['aaaa0001', 'aaaa0002']) {
        const job = await s.call('GET', `/v1/jobs/${id}`);
        assert.equal(job.status, 'interrupted');
        assert.equal(job.accountLimit?.state, 'stop', 'the kept Stop shows on the job');
      }
      await noRepeatedInterruption(page);
      // An answer to the older decision: main finds it no longer listed and refuses before signing.
      const refused = await page.evaluate(() => window.app.obid.limitAnswer('d-stop', 3, 'stop'));
      assert.equal(refused.status, 409);
      assert.match(JSON.parse(refused.text).error, /choice changed/);
      assert.equal(existsSync(join(s.root, 'electron-data/device-key')), false, 'refused before signing');
      const unsigned = await fetch(`http://127.0.0.1:${s.port}/v1/jobs/aaaa0001/messages`, {
        method: 'POST', headers: { Authorization: `Bearer ${readFileSync(join(s.home, 'api-token'), 'utf8').trim()}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: 'Try again, picking up where you left off. Finish with a short summary as asked.' }),
      });
      assert.equal(unsigned.status, 403, 'the API token alone cannot continue a kept-stopped job');
      // the owner's own words: typed in a box, Enter pressed (main signs only right after a real
      // keystroke, and the preload only the words it held: docs/contracts/signed-words.md)
      const words = 'Try again, picking up where you left off. Finish with a short summary as asked.';
      await page.evaluate((text) => { const box = document.body.appendChild(document.createElement('textarea')); box.value = text; box.focus(); }, words);
      await page.keyboard.press('Enter');
      const manual = await page.evaluate((text) => window.app.obid.call('POST', '/v1/jobs/aaaa0001/messages', { text }), words);
      assert.equal(manual.status, 200, manual.text);
      assert.ok(existsSync(join(s.root, 'electron-data/device-key')), 'encrypted device key created in isolated userData');
      await until('signed manual continuation of the kept Stop', async () => (await s.call('GET', '/v1/jobs/aaaa0001')).status === 'done', 60_000);
      assert.equal((await s.call('GET', '/v1/jobs/aaaa0002')).status, 'interrupted', 'the other stays stopped');
      assert.deepEqual(errors, []);
      console.log('PASS an older kept Stop stays stopped with no card; only the owner\'s signed words in real Electron take it on');
    } finally { await close(s, app); }
  }

  {
    const s = await setup('pending-moves', { available: true });
    const { app, page, errors } = await openApp(s);
    try {
      await noLimitCard(page, s);
      settled(s);
      for (const id of ['aaaa0001', 'aaaa0002']) {
        const done = await until('Codex continues the job', async () => {
          const job = await s.call('GET', `/v1/jobs/${id}`);
          return job.status === 'done' && job.agent === 'codex' ? job : null;
        }, 60_000);
        assert.equal(done.branch, `oby/research-${id}`);
        assert.equal(done.account, 'codex');
        assert.equal(done.moves.length, 1);
        assert.equal(done.accountLimit, null);
        // Codex runs in the own lane (CODEX-1): its thread is in the account's own folder.
        assert.match(readFileSync(join(s.home, 'accounts/codex/threads', `${done.session}.json`), 'utf8'), /same worktree and branch/);
      }
      assert.deepEqual(errors, []);
      console.log('PASS an older pending decision moves both jobs to Codex by itself, same branches and handoffs, no card');
    } finally { await close(s, app); }
  }

  {
    const s = await setup('resume-waits', { choice: 'resume' });
    const { app, page, errors } = await openApp(s);
    try {
      await noLimitCard(page, s);
      settled(s);
      for (const id of ['aaaa0001', 'aaaa0002']) {
        const job = await s.call('GET', `/v1/jobs/${id}`);
        assert.equal(job.status, 'queued');
        assert.ok(job.resumesAt, 'it waits for its reset or an account with room');
        assert.equal(job.accountLimit, null);
        assert.equal(job.round, 1);
        assert.equal(job.branch, `oby/research-${id}`);
      }
      assert.deepEqual(errors, []);
      console.log('PASS an older approved Resume with no room anywhere waits on its own, no card');
    } finally { await close(s, app); }
  }
  console.log('Account-limit Electron integration passed. All daemon and app data were isolated.');
  rmSync(scratch, { recursive: true, force: true });
} catch (error) {
  console.error(error);
  console.error(`Scratch evidence retained at ${scratch}`);
  process.exitCode = 1;
} finally {
  for (const app of apps) await app.close().catch(() => {});
  for (const child of processes) child.kill('SIGINT');
}
