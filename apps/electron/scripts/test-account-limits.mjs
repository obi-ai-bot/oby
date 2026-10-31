// Real Electron → preload → main/device → signed HTTP → real obid regression for #1563.
// Uses only stand-in agents and an isolated runner home. Keep Electron's HOME: replacing it
// makes macOS safeStorage unavailable even with a separate userData folder.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
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

function room() {
  return { fiveHour: 0.1, sevenDay: 0.2, fiveHourResets: now() + 3600, sevenDayResets: now() + 86400,
    fiveHourObservedAt: now(), sevenDayObservedAt: now() };
}

async function setup(name, { seed = true, available = false, reset = now() + 3600 } = {}) {
  const root = join(scratch, name), home = join(root, 'home');
  for (const dir of ['home/jobs', 'home/accounts/stub', 'home/accounts/codex', 'env-home', 'tmp', 'electron-data']) mkdirSync(join(root, dir), { recursive: true });
  const fixture = buildFixture(join(root, 'fixture'));
  json(join(home, 'accounts/stub/.credentials.json'), { claudeAiOauth: { accessToken: 'fake-claude', expiresAt: (now() + 86400) * 1000 } });
  json(join(home, 'accounts/codex/auth.json'), { OPENAI_API_KEY: 'fake-codex' });
  if (seed) for (const id of ['aaaa0001', 'aaaa0002']) json(join(home, `jobs/${id}.json`), seededJob(id, reset));
  json(join(home, 'usage.json'), available ? { codex: room() } : (reset < now() ? { stub: room() } : {}));
  const port = await freePort();
  const config = {
    listen: `127.0.0.1:${port}`, home, owner: 'wellington', telemetry: false,
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

async function openApp(s, unavailable = false) {
  // Set userData before ready or any key read; retain the real HOME for the OS Keychain.
  const launcher = join(s.root, 'electron-launcher.cjs');
  writeFileSync(launcher, `const { app, safeStorage, ipcMain } = require('electron');
app.setPath('userData', ${JSON.stringify(join(s.root, 'electron-data'))});
${unavailable ? `safeStorage.isEncryptionAvailable = () => false;
// Controlled IPC latency makes the pending UI observable. The actual registered handler,
// preload, secure-storage error and response all still run; no API/bridge is substituted.
const handle = ipcMain.handle.bind(ipcMain);
ipcMain.handle = (channel, listener) => handle(channel, channel === 'obid-limit-answer'
  ? async (...args) => { await new Promise((done) => setTimeout(done, 500)); return listener(...args); }
  : listener);` : ''}
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
  const card = await limitCard(page);
  assert.match(await card.innerText(), /2 tasks are waiting/, 'one group, not one card per task');
  return { app, page, card, errors };
}

// The decision is a card of kind limit in Home's stack (AskStack); brought to the top, past any
// other card that needs the owner, and given AskBlock's half-second before a click counts.
const limitCard = async (page) => {
  const card = page.locator('.ask-card.is-up').filter({ has: page.locator('.tc-ask-kind.is-limit') });
  await until('the limit decision on top of Home’s stack', async () => {
    if (await card.count()) return true;
    const next = page.locator('.ask-card.is-up button[aria-label="Next"]');
    if (await next.count()) await next.click();
    return false;
  }, 30_000);
  await page.waitForTimeout(600);
  return card;
};

async function close(s, app) {
  if (app) { await app.close(); apps.delete(app); }
  s.daemon.kill('SIGINT');
  await until('scratch obid shutdown', () => s.daemon.exitCode != null || s.daemon.signalCode != null, 10_000);
  processes.delete(s.daemon);
  writeFileSync(join(s.root, 'obid.log'), s.logs.join(''));
}

async function noRepeatedInterruption(page) {
  await page.locator('button.chip').filter({ hasText: /^Tasks$/ }).click();
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

  for (const unavailable of [true, false]) {
    const s = await setup(unavailable ? 'keychain-unavailable' : 'stop');
    const { app, page, card, errors } = await openApp(s, unavailable);
    try {
      if (unavailable) {
        await card.getByRole('button', { name: 'Keep stopped' }).click();
        await card.waitFor({ state: 'detached', timeout: 2000 });  // out of the stack while the answer is in flight
        const back = await limitCard(page);
        assert.match(await back.innerText(), /Keychain isn’t available/);
        assert.ok(await back.getByRole('button', { name: 'Keep stopped' }).isEnabled(), 'failed signing can be retried');
        assert.equal(read(join(s.home, 'limit-decisions.json'))[0].choice, 'pending');
        assert.equal(existsSync(join(s.root, 'electron-data/device-key')), false, 'no plaintext fallback');
      } else {
        await card.getByRole('button', { name: 'Keep stopped' }).click();
        await card.waitFor({ state: 'detached' });
        // (the card leaves Home's stack as it's answered; obid saves the choice a moment later)
        await until('the owner\'s choice is saved', () => read(join(s.home, 'limit-decisions.json'))[0].choice === 'stop');
        assert.ok(existsSync(join(s.root, 'electron-data/device-key')), 'encrypted device key created in isolated userData');
        for (const id of ['aaaa0001', 'aaaa0002']) assert.equal((await s.call('GET', `/v1/jobs/${id}`)).status, 'interrupted');
        await noRepeatedInterruption(page);
        const unsigned = await fetch(`http://127.0.0.1:${s.port}/v1/jobs/aaaa0001/messages`, {
          method: 'POST', headers: { Authorization: `Bearer ${readFileSync(join(s.home, 'api-token'), 'utf8').trim()}`, 'Content-Type': 'application/json' },
          body: JSON.stringify({ text: 'Try again, picking up where you left off. Finish with a short summary as asked.' }),
        });
        assert.equal(unsigned.status, 403, 'the API token alone cannot continue a limit-stopped job');
        // the owner's own words: typed in a box, Enter pressed (main signs only right after a real
        // keystroke, and the preload only the words it held: docs/contracts/signed-words.md)
        const words = 'Try again, picking up where you left off. Finish with a short summary as asked.';
        await page.evaluate((text) => { const box = document.body.appendChild(document.createElement('textarea')); box.value = text; box.focus(); }, words);
        await page.keyboard.press('Enter');
        const manual = await page.evaluate((text) => window.app.obid.call('POST', '/v1/jobs/aaaa0001/messages', { text }), words);
        assert.equal(manual.status, 200, manual.text);
        await until('signed manual continuation after Keep stopped', async () => (await s.call('GET', '/v1/jobs/aaaa0001')).status === 'done', 60_000);
      }
      assert.deepEqual(errors, []);
      console.log(`PASS ${unavailable ? 'unavailable Keychain gives visible error without plaintext key' : 'real Electron signed Keep stopped for two jobs'}`);
    } finally { await close(s, app); }
  }

  {
    const s = await setup('unrelated-permission');
    // a mode that asks: a new install runs in Bypass (#1602), which asks nothing about the network
    await s.call('PUT', '/v1/settings/permissions', { mode: 'ask' });
    let app;
    try {
      const posted = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', agent: 'claude', account: 'stub',
        task: 'ASK: {"tool":"Bash","input":{"command":"curl https://example.invalid"}}' } });
      await until('unrelated permission waits for the owner', async () => (await s.call('GET', `/v1/jobs/${posted.id}`)).permission);
      const opened = await openApp(s); app = opened.app;
      await opened.page.locator('button.chip').filter({ hasText: /^Tasks$/ }).click();
      await opened.page.getByRole('tab', { name: /^Running/ }).click();
      await opened.page.locator('.group-layer:not(.is-leaving) .tc-ask-kind').filter({ hasText: /^Permission$/ }).waitFor();
      await opened.page.keyboard.press('Escape');
      await opened.page.locator('.group-layer').waitFor({ state: 'detached' });
      await opened.card.getByRole('button', { name: 'Keep stopped' }).click();
      await opened.card.waitFor({ state: 'detached' });
      await opened.page.locator('button.chip').filter({ hasText: /^Tasks$/ }).click();
      await opened.page.getByRole('tab', { name: /^Running/ }).click();
      await opened.page.locator('.group-layer:not(.is-leaving) .tc-ask-kind').filter({ hasText: /^Permission$/ }).waitFor();
      assert.equal(await opened.page.locator('.group-layer:not(.is-leaving) .tc-ask-head').filter({ hasText: /^It was interrupted$/ }).count(), 0);
      console.log('PASS grouped decision preserves an unrelated live permission before and after the group is answered');
    } finally { await close(s, app); }
  }

  {
    const s = await setup('stale-revision', { reset: null });
    let app;
    try {
      const opened = await openApp(s); app = opened.app;
      const old = (await s.call('GET', '/v1/limit-decisions'))[0];
      const posted = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', agent: 'claude', account: 'stub', task: 'LIMITED: 3600' } });
      await until('a new job joins the group', async () => {
        await s.call('GET', `/v1/jobs/${posted.id}`);
        const current = (await s.call('GET', '/v1/limit-decisions'))[0];
        return current?.jobs.length === 3 && current.revision > old.revision;
      });
      const refused = await opened.page.evaluate((d) => window.app.obid.limitAnswer(d.id, d.revision, 'stop'), old);
      assert.equal(refused.status, 409);
      assert.match(JSON.parse(refused.text).error, /choice changed/);
      assert.equal(read(join(s.home, 'limit-decisions.json'))[0].choice, 'pending');
      assert.equal(existsSync(join(s.root, 'electron-data/device-key')), false, 'a changed scope is refused before signing');
      console.log('PASS real Electron IPC rejects an old revision after another job joins, before signing');
    } finally { await close(s, app); }
  }

  {
    const s = await setup('move', { seed: false, available: true });
    let app;
    try {
      const ids = [];
      for (let i = 0; i < 2; i++) {
        const job = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', task: 'LIMITED: 3600', agent: 'claude', account: 'stub' } });
        ids.push(job.id);
        await until('Claude limit stops the job', async () => (await s.call('GET', `/v1/jobs/${job.id}`)).status === 'interrupted');
      }
      const before = await Promise.all(ids.map((id) => s.call('GET', `/v1/jobs/${id}`)));
      ({ app } = await openApp(s));
      const page = await app.firstWindow();
      const card = await limitCard(page);
      await card.getByRole('button', { name: /Move all to Codex destination/ }).click();
      await card.waitFor({ state: 'detached' });
      for (const [i, id] of ids.entries()) {
        const done = await until('Codex continues the job', async () => {
          const job = await s.call('GET', `/v1/jobs/${id}`);
          return job.status === 'done' && job.agent === 'codex' ? job : null;
        }, 60_000);
        assert.equal(done.branch, before[i].branch);
        assert.equal(done.worktree, before[i].worktree);
        assert.equal(done.account, 'codex');
        assert.equal(done.moves.length, 1);
        const threads = join(s.home, 'logins', id, 'threads');
        const prompts = readdirSync(threads).map((name) => readFileSync(join(threads, name), 'utf8')).join('\n');
        assert.match(prompts, /same worktree and branch/);
      }
      assert.equal(read(join(s.home, 'limit-decisions.json'))[0].choice, 'move');
      console.log('PASS real Electron signed move of two limit-stopped Claude jobs to Codex, same branches/worktrees and handoffs');
    } finally { await close(s, app); }
  }

  {
    const s = await setup('resume', { reset: now() - 120 });
    const { app, page, card } = await openApp(s);
    try {
      await card.getByRole('button', { name: 'Resume when available' }).click();
      await card.waitFor({ state: 'detached' });
      // (the card leaves Home's stack as it's answered; obid saves the choice a moment later)
      await until('the owner\'s choice is saved', () => read(join(s.home, 'limit-decisions.json'))[0].choice === 'resume');
      await noRepeatedInterruption(page);
      for (const id of ['aaaa0001', 'aaaa0002']) {
        const done = await until('approved reset resumes job', async () => {
          const job = await s.call('GET', `/v1/jobs/${id}`);
          return job.status === 'done' ? job : null;
        }, 60_000);
        assert.equal(done.round, 2);
        assert.equal(done.account, 'stub');
        assert.equal(done.branch, `oby/research-${id}`);
        assert.equal(done.moves.length, 0);
      }
      console.log('PASS real Electron signed reset opt-in resumes both jobs once on their original account');
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
