// Oby's core, end to end, through the real app: Electron → preload → main → signed HTTP → a real
// obid in a scratch home, with stand-in agents (core/crates/obid/tests/agents) and a local fixture
// repository. Nothing reaches a real agent, account or GitHub. What it proves, through the UI:
//   1. Words in Home's box start a task that runs to its end, its card in the conversation.
//   2. A task started on its own (POST /v1/jobs, as the Tasks view does) has a conversation of its
//      own (docs/contracts/task-conversations.md, TASKCONV-1).
//   3. A follow-up asked on that task's page lands in that conversation, with its card (TASKCONV-3).
//   4. A task with no conversation gets one when its page's words reach it, and the app opens it
//      (TASKCONV-4).
// Screenshots of each step go to OBY_CORE_SHOTS (a folder), when it's set.
// Run: cargo build -p obid (or OBY_TEST_OBID=<obid>), npm run build, then npm run test:core.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
const shots = process.env.OBY_CORE_SHOTS;
const scratch = mkdtempSync(join(tmpdir(), 'oby-core-'));
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const json = (path, value) => writeFileSync(path, JSON.stringify(value), { mode: 0o600 });
let daemon, app;

async function until(label, f, ms = 60_000) {
  const deadline = Date.now() + ms;
  let last;
  while (Date.now() < deadline) {
    try { const value = await f(); if (value) return value; } catch (e) { last = e; }
    await sleep(150);
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

// A scratch obid: one stand-in Claude account, the fixture repository with its check
async function startObid() {
  const root = scratch, home = join(root, 'home');
  for (const dir of ['home/accounts/stub', 'env-home', 'tmp', 'electron-data']) mkdirSync(join(root, dir), { recursive: true });
  const fixture = buildFixture(join(root, 'fixture'));
  json(join(home, 'accounts/stub/.credentials.json'), { claudeAiOauth: { accessToken: 'fake-claude', expiresAt: (Math.floor(Date.now() / 1000) + 86400) * 1000 } });
  const port = await freePort();
  json(join(root, 'config.json'), {
    listen: `127.0.0.1:${port}`, home, owner: 'wellington', telemetry: false,
    claudeKeychain: false, cursorKeychain: false, allowFileRemotes: true,
    agents: { claude: { bin: join(agents, 'fake-harness-jobs'), accounts: [{ id: 'stub', name: 'Stub', configDir: join(home, 'accounts/stub') }] } },
    repos: [{ repo: 'obi-e2e/fixture-app', remote: fixture.remoteUrl, defaultBranch: 'main', check: 'node --test', pullRequests: true }],
  });
  const logs = [];
  daemon = spawn(binary, ['serve', '--config', join(root, 'config.json')], { env: {
    PATH: `${agents}:${process.env.PATH}`, HOME: join(root, 'env-home'), TMPDIR: join(root, 'tmp'),
    OBID_SECURITY_BIN: join(agents, 'fake-security'), OBID_PRESTART: 'off', OBID_USAGE_READS: 'off',
    OBID_LOAD: '0', OBID_MEMORY_AVAILABLE: '1099511627776',
  }, stdio: ['ignore', 'pipe', 'pipe'] });
  daemon.stdout.on('data', (b) => logs.push(b.toString()));
  daemon.stderr.on('data', (b) => logs.push(b.toString()));
  const call = async (method, path, body) => {
    const token = readFileSync(join(home, 'api-token'), 'utf8').trim();
    const response = await fetch(`http://127.0.0.1:${port}${path}`, {
      method, headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const text = await response.text();
    const value = text ? JSON.parse(text) : null;
    assert.ok(response.ok, `${method} ${path}: ${response.status}: ${text}`);
    return value;
  };
  await until('the scratch obid', () => call('GET', '/v1/status'));
  return { root, home, port, call, logs };
}

async function openApp(s) {
  const launcher = join(s.root, 'electron-launcher.cjs');
  writeFileSync(launcher, `const { app } = require('electron');
app.setPath('userData', ${JSON.stringify(join(s.root, 'electron-data'))});
import(${JSON.stringify(pathToFileURL(join(appRoot, 'out/main/index.js')).href)});
`);
  app = await electron.launch({ executablePath: electronBinary, args: [launcher], env: { ...process.env, OBID_HOME: s.home, OBID_PORT: String(s.port) } });
  const page = await app.firstWindow();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.waitForLoadState('domcontentloaded');
  // set up already (FirstRun.tsx), so Home shows rather than the first run's screens
  await page.evaluate(() => { for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1'); });
  await page.reload();
  await page.waitForLoadState('domcontentloaded');
  await page.setViewportSize({ width: 1280, height: 820 }).catch(() => {});
  return { page, errors };
}

const shot = async (page, name) => { if (shots) { mkdirSync(shots, { recursive: true }); await page.screenshot({ path: join(shots, `${name}.png`) }); } };
// what the app shows now, as text
const shown = (page) => page.evaluate(() => document.body.innerText);
// types words into the message box with this placeholder, and sends them
async function send(page, placeholder, words) {
  const box = page.locator(`[contenteditable="true"][aria-placeholder="${placeholder}"]`).last();
  await box.click();
  await page.keyboard.insertText(words);
  await page.keyboard.press('Enter');
}
const jobs = async (s) => s.call('GET', '/v1/jobs');
const ended = (j) => ['done', 'failed', 'interrupted', 'stopped'].includes(j.status);

async function main() {
  const s = await startObid();
  const { page, errors } = await openApp(s);
  await until('Home', async () => (await page.locator('.app').count()) > 0);
  await shot(page, '0-home');

  // 1. Home's box: the words go to Oby's brain (the stand-in's OBY lines), which starts a task
  const box = await until('Home’s message box', async () => {
    const all = page.locator('[contenteditable="true"]');
    return (await all.count()) ? all.last() : null;
  });
  await box.click();
  await page.keyboard.insertText('OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"research","title":"Core flow","task":"Look around"}');
  await page.keyboard.press('Enter');
  const first = await until('a task from Home’s box', async () => (await jobs(s)).find((j) => j.title === 'Core flow'));
  assert.ok(first.conversation, 'a task from Home’s box is in the conversation its words started');
  const done = await until('it runs to its end', async () => { const j = await s.call('GET', `/v1/jobs/${first.id}`); return ended(j) && j; }, 120_000);
  assert.equal(done.status, 'done', JSON.stringify(done));
  await until('its card in the conversation', async () => (await shown(page)).includes('Core flow'));
  await shot(page, '1-task-from-home');

  // 2. A task started on its own (POST /v1/jobs, as the Tasks view does): its own conversation
  const own = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', title: 'Fix CI leccoflow #1', task: 'Look around the fixture' } });
  assert.ok(own.conversation && own.conversation !== first.conversation, 'TASKCONV-1: a conversation of its own');
  await until('the task ends', async () => ended(await s.call('GET', `/v1/jobs/${own.id}`)), 120_000);
  const listed = await s.call('GET', '/v1/conversations');
  assert.ok(listed.some((c) => c.id === own.conversation && c.title === 'Fix CI leccoflow #1'), 'TASKCONV-1: named after the task');

  // 3. Its page: a follow-up asked there lands in its conversation (the stand-in brain's OBY line
  // stands in for the words the real brain reads)
  await until('the task in the app', async () => (await page.evaluate((id) => !!window.app, own.id)));
  await page.evaluate((id) => dispatchEvent(new CustomEvent('oby:open', { detail: { kind: 'task', id }, cancelable: true })), own.id);
  await until('the task’s page', async () => (await page.locator('[aria-placeholder="Tell Oby about this task…"]').count()) > 0);
  await shot(page, '2-task-page');
  await send(page, 'Tell Oby about this task…', `OBY oby_tell_job {"id":"${own.id}","words":"why wasn't this merged?"}`);
  const follow = await until('the follow-up', async () => (await jobs(s)).find((j) => j.followUpOf === own.id));
  assert.equal(follow.conversation, own.conversation, 'TASKCONV-3: the follow-up is in the task’s conversation');
  const turns = await s.call('GET', `/v1/conversation?id=${own.conversation}`);
  assert.ok(turns.some((t) => t.who === 'oby' && t.jobs?.includes(follow.id)), 'TASKCONV-3: its card in the conversation');
  await until('the conversation open, the follow-up’s card in it', async () => (await shown(page)).includes('Follow-up: Fix CI leccoflow #1'));
  await shot(page, '3-follow-up-in-conversation');

  // 4. A task with no conversation (here, its conversation deleted): its page's words make one,
  // and the app opens it
  const old = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', title: 'Old task', task: 'Look around again' } });
  await until('the old task ends', async () => ended(await s.call('GET', `/v1/jobs/${old.id}`)), 120_000);
  await s.call('DELETE', `/v1/conversations/${old.conversation}`);
  await page.evaluate((id) => dispatchEvent(new CustomEvent('oby:open', { detail: { kind: 'task', id }, cancelable: true })), old.id);
  await until('the old task’s page', async () => (await page.locator('[aria-placeholder="Tell Oby about this task…"]').count()) > 0);
  await send(page, 'Tell Oby about this task…', 'why wasn’t this merged?');
  const again = await until('the old task in a new conversation', async () => {
    const j = await s.call('GET', `/v1/jobs/${old.id}`);
    return j.conversation && j.conversation !== old.conversation && j;
  });
  const words = await s.call('GET', `/v1/conversation?id=${again.conversation}`);
  assert.ok(words.some((t) => t.who === 'user' && t.text === 'why wasn’t this merged?'), 'TASKCONV-4: the words are in it');
  await until('the app shows the words in the conversation', async () => (await shown(page)).includes('why wasn’t this merged?'));
  await shot(page, '4-old-task-gets-a-conversation');

  assert.deepEqual(errors, [], 'no page errors');
  console.log('core: Home → task → its conversation → follow-ups: all passed');
}

try {
  await main();
} catch (e) {
  console.error(e);
  process.exitCode = 1;
} finally {
  await app?.close().catch(() => {});
  if (daemon && daemon.exitCode === null) { const gone = new Promise((done) => daemon.once('exit', done)); daemon.kill(); await gone; }
  if (!process.env.OBY_CORE_KEEP) rmSync(scratch, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}
