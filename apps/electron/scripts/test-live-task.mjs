// LIVETASK (docs/contracts/live-task.md), through the built app with real input: Electron → a real
// obid in a scratch home, with stand-in agents and a local fixture repository. Nothing reaches a real
// agent, account or GitHub. What only this proves: the card opens in place into the agent's
// conversation and not the task's page, a second click folds it, the box tells it, its follow-up's
// card opens, Stop in the box stops it, a tool line fetches its detail and stays that call's as the
// to-dos change, a deleted job's card goes, and the card's menu opens the task's page.
// Screenshots go to OBY_LIVE_SHOTS (a folder), when it's set.
// Run: OBY_TEST_OBID=<obid> (else core/target/debug/obid), npm run build, then npm run test:live-task.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { buildFixture } from '../../../e2e/lib/fixture.mjs';
import { appRoot, launch, sleep, until } from './drive-lib.mjs';

const repoRoot = resolve(appRoot, '../..');
const agents = join(repoRoot, 'core/crates/obid/tests/agents');
const binary = process.env.OBY_TEST_OBID || join(repoRoot, 'core/target/debug/obid');
const shots = process.env.OBY_LIVE_SHOTS;
const root = join(homedir(), 'Library/Application Support/oby-live-task');
const home = join(root, 'home');
const json = (path, value) => writeFileSync(path, JSON.stringify(value), { mode: 0o600 });
const ok = (line) => console.log(`ok  ${line}`);
const ended = (j) => ['done', 'failed', 'interrupted', 'stopped'].includes(j.status);
let daemon, app;

async function startObid() {
  rmSync(root, { recursive: true, force: true });
  for (const dir of ['home/accounts/stub', 'env-home', 'tmp', 'electron-data']) mkdirSync(join(root, dir), { recursive: true });
  const fixture = buildFixture(join(root, 'fixture'));
  json(join(home, 'accounts/stub/.credentials.json'), { claudeAiOauth: { accessToken: 'fake-claude', expiresAt: (Math.floor(Date.now() / 1000) + 86400) * 1000 } });
  const server = createServer();
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  const { port } = server.address();
  await new Promise((done) => server.close(done));
  json(join(root, 'config.json'), {
    listen: `127.0.0.1:${port}`, home, owner: 'wellington',
    claudeKeychain: false, cursorKeychain: false, allowFileRemotes: true,
    agents: { claude: { bin: join(agents, 'fake-harness-jobs'), accounts: [{ id: 'stub', name: 'Stub', configDir: join(home, 'accounts/stub') }] } },
    repos: [{ repo: 'obi-e2e/fixture-app', remote: fixture.remoteUrl, defaultBranch: 'main', check: 'node --test', pullRequests: true }],
  });
  daemon = spawn(binary, ['serve', '--config', join(root, 'config.json')], { env: {
    PATH: `${agents}:${process.env.PATH}`, HOME: join(root, 'env-home'), TMPDIR: join(root, 'tmp'),
    OBID_SECURITY_BIN: join(agents, 'fake-security'), OBID_PRESTART: 'off', OBID_USAGE_READS: 'off',
    OBID_LOAD: '0', OBID_MEMORY_AVAILABLE: '1099511627776',
  }, stdio: 'ignore' });
  const call = async (method, path, body) => {
    const token = readFileSync(join(home, 'api-token'), 'utf8').trim();
    const response = await fetch(`http://127.0.0.1:${port}${path}`, {
      method, headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const text = await response.text();
    assert.ok(response.ok, `${method} ${path}: ${response.status}: ${text}`);
    return text ? JSON.parse(text) : null;
  };
  await until('the scratch obid', async () => (await call('GET', '/v1/status')).accounts);
  return { port, call };
}

async function main() {
  const s = await startObid();
  app = await launch({ dir: root, userData: join(root, 'electron-data'), env: { ...process.env, OBID_HOME: home, OBID_PORT: String(s.port) } });
  const page = await app.firstWindow();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.waitForLoadState('domcontentloaded');
  await page.evaluate(() => { for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1'); });
  await page.reload();
  await until('Home', async () => (await page.locator('.app').count()) > 0);
  const size = async (width, height = 780) => {
    await app.evaluate(({ BrowserWindow }, [w, h]) => BrowserWindow.getAllWindows()[0].setSize(w, h), [width, height]);
    await until('the window’s size', async () => (await page.evaluate(() => innerWidth)) === width);
    await sleep(250);
  };
  await size(1180);
  const shot = async (name) => { if (shots) { mkdirSync(shots, { recursive: true }); await page.screenshot({ path: join(shots, `${name}.png`) }); } };
  const type = async (selector, words) => {
    const box = typeof selector === 'string' ? page.locator(selector).last() : selector;
    await until('the box shown', () => box.evaluate((el) => el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })));
    await box.click();
    await page.keyboard.insertText(words);
    await page.keyboard.press('Enter');
  };
  const dispatch = async (title, task, selector) => {
    await type(selector, `OBY oby_dispatch ${JSON.stringify({ repo: 'obi-e2e/fixture-app', kind: 'research', title, task })}`);
    const job = await until(`the job ${title}`, async () => (await s.call('GET', '/v1/jobs')).find((j) => j.title === title));
    await until(`the card of ${title}`, () => page.locator('.msg-task.is-link', { hasText: title }).count());
    return job;
  };
  const card = (title) => page.locator('.msg-task-work').filter({ hasText: title });
  // a card opened in place: the agent's own conversation, under its head
  const thread = (title) => card(title).locator('.lt-thread');

  // LIVETASK-1: the card shows the list and what it's doing, as it goes
  const job = await dispatch('The list', 'TODOS', '[contenteditable="true"]');
  await until('its to-dos on the card', () => card('The list').locator('.lt-todos li.is-on').filter({ hasText: 'First thing' }).count(), 30_000);
  assert.equal(await card('The list').locator('.lt-todos li.is-up').filter({ hasText: 'Second thing' }).count(), 1);
  await until('its last steps under the one it’s on', () => card('The list').locator('.lt-tool', { hasText: 'echo beta' }).count());
  await shot('1-card');
  ok('LIVETASK-1 the card shows the list, the current one lit, and the last things it did');

  // LIVETASK-5: a click opens the agent's conversation in place, not the task's page
  const layers = () => page.locator('.chat-layer').count();
  const under = await layers();
  await card('The list').locator('.msg-task.is-link').click();
  await until('its conversation', () => thread('The list').count());
  assert.equal(await layers(), under, 'no page opened over the chat');
  assert.equal(await card('The list').locator('.msg-task.is-link').getAttribute('aria-expanded'), 'true');
  assert.equal(await thread('The list').locator('.lt-thread-todos li').count(), 2, 'its to-dos on top');
  assert.equal(await thread('The list').locator('.ss-plan').count(), 0, 'and not in the conversation as well');
  assert.equal(await card('The list').locator('.lt-now').count(), 0, 'nor on the card as well');
  await shot('2-opened');
  ok('LIVETASK-5 a click opens its own conversation in place, under its card');

  // LIVETASK-11, -15: a line fetches what it ran, and stays that call's as the to-dos change
  const alpha = thread('The list').locator('.ss-tool', { hasText: 'echo alpha' });
  const planBefore = (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length;
  await alpha.locator('.ss-tool-row').click();
  await until('its output', () => alpha.locator('.ss-out', { hasText: 'alpha said this' }).count(), 10_000);
  ok('LIVETASK-11 a tool line opens to what it gave back');
  await until('a to-do ticked', async () => (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length > planBefore, 15_000);
  await until('the tick shown', () => thread('The list').locator('.lt-thread-todos li.is-done').count(), 15_000);
  assert.equal(await thread('The list').locator('.ss-tool.is-open').count(), 1);
  assert.equal(await thread('The list').locator('.ss-tool.is-open', { hasText: 'echo alpha' }).count(), 1, 'the same call stays open');
  await shot('3-line-open');
  ok('LIVETASK-15 an opened line stays that call’s as the to-dos change');

  // LIVETASK-5: a second click folds it, and another opens it again
  await card('The list').locator('.msg-task.is-link').click();
  await until('folded', async () => (await thread('The list').count()) === 0);
  assert.equal(await card('The list').locator('.msg-task.is-link').getAttribute('aria-expanded'), 'false');
  await card('The list').locator('.msg-task.is-link').click();
  await until('open again', () => thread('The list').count());
  assert.equal(await page.locator('.chat-layer').count(), under, 'nothing opened beside or over the chat');
  ok('LIVETASK-5 a second click folds it, and another opens it again');

  // LIVETASK-4: ended, one line
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${job.id}`)), 60_000);
  await until('the card folds', async () => (await card('The list').locator('.lt-now').count()) === 0, 15_000);
  await shot('7-ended');
  ok('LIVETASK-4 ended, the card folds');

  // LIVETASK-7: the card's box tells it, and the follow-up's card shows opened where it's listed
  assert.equal(await thread('The list').getByRole('button', { name: 'Stop' }).count(), 0, 'nothing to stop');
  await type(thread('The list').locator('[contenteditable="true"]'), 'ECHO one more thing');
  const next = await until('the follow-up', async () => (await s.call('GET', '/v1/jobs')).find((j) => j.followUpOf === job.id), 30_000);
  await until('your words in its opened card', () => page.locator('.lt-thread .ss-you', { hasText: 'one more thing' }).count(), 15_000);
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${next.id}`)), 60_000);
  ok('LIVETASK-7 the card’s box tells it, and a finished one’s follow-up shows opened');

  // LIVETASK-5: Stop in the card's box stops it, and the card says so
  const stopping = await dispatch('The long one', 'SLEEP', '.chat-layer > .composer [contenteditable="true"]');
  await until('at work', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'running', 30_000);
  await card('The long one').locator('.msg-task.is-link').click();
  await until('its conversation', () => thread('The long one').count());
  await thread('The long one').getByRole('button', { name: 'Stop' }).click();
  await until('stopped', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'stopped', 30_000);
  await until('the card says so', () => card('The long one').locator('.msg-task-meta', { hasText: 'Stopped ·' }).count(), 15_000);
  assert.equal(await card('The long one').locator('[aria-label="Stopped"]').count(), 1);
  await until('no Stop any more', async () => (await thread('The long one').getByRole('button', { name: 'Stop' }).count()) === 0);
  await shot('8-stopped');
  ok('LIVETASK-5 Stop in the card’s box stops it, and the card says so');

  // LIVETASK-15: deleted, its opened card goes with it
  await s.call('POST', `/v1/jobs/${stopping.id}/delete`).catch(() => s.call('DELETE', `/v1/jobs/${stopping.id}`));
  await until('it goes', async () => (await card('The long one').count()) === 0, 15_000);
  ok('LIVETASK-15 a deleted job’s opened card goes with it');

  // LIVETASK-18: the card's menu (right-click), View details: the task's page; then, LIVETASK-15 again
  // where every list is in the session, Full session
  const second = await dispatch('The second list', 'TODOS', '.chat-layer > .composer [contenteditable="true"]');
  await card('The second list').locator('.msg-task.is-link').click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'View details' }).click();
  ok('LIVETASK-18 the card’s menu opens the task’s page');
  await page.getByRole('button', { name: 'Full session' }).click();
  const session = page.getByRole('region', { name: 'The second list: session' });
  const line = session.locator('.ss-tool', { hasText: 'echo alpha' });
  await line.locator('.ss-tool-row').click();
  await until('its output', () => line.locator('.ss-out', { hasText: 'alpha said this' }).count(), 10_000);
  await until('a to-do ticked', () => session.locator('.ss-plan .is-done').count(), 15_000);
  assert.equal(await session.locator('.ss-plan').count(), 1, 'one list, the newest');
  assert.equal(await session.locator('.ss-tool.is-open', { hasText: 'echo alpha' }).count(), 1, 'the same call stays open');
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${second.id}`)), 60_000);
  ok('LIVETASK-13, -15 Full session: one list, and an opened line stays that call’s');

  assert.deepEqual(errors, [], 'no errors on the page');
}

main().then(
  async () => { await app.evaluate(({ app: a }) => a.exit(0)).catch(() => {}); daemon.kill(); await sleep(500); if (!process.env.OBY_TEST_KEEP) rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 }); console.log('passed'); process.exit(0); },
  async (e) => { console.error(e); try { const w = app?.windows()[0]; if (shots && w) await w.screenshot({ path: join(shots, 'failed.png') }); } catch {} await app?.evaluate(({ app: a }) => a.exit(0)).catch(() => {}); daemon?.kill(); process.exit(1); },
);
