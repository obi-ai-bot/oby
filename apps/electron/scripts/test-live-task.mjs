// LIVETASK (docs/contracts/live-task.md), through the built app with real input: Electron → a real
// obid in a scratch home, with stand-in agents and a local fixture repository. Nothing reaches a real
// agent, account or GitHub. What only this proves: the card opens the agent's conversation and not
// the task's page, it follows its follow-up, the box tells it, Stop stops it, Esc closes it, a tool
// line fetches its detail and stays that call's as the to-dos change, a deleted job's conversation
// closes, and the layout at its widths.
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
    const box = page.locator(selector).last();
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
  const pane = page.locator('.agent-pane');
  const box = (el) => el.evaluate((e) => { const r = e.getBoundingClientRect(); return { left: r.left, right: r.right, width: r.width }; });

  // LIVETASK-1: the card shows the list and what it's doing, as it goes
  const job = await dispatch('The list', 'TODOS', '[contenteditable="true"]');
  await until('its to-dos on the card', () => card('The list').locator('.lt-todos li.is-on').filter({ hasText: 'First thing' }).count(), 30_000);
  assert.equal(await card('The list').locator('.lt-todos li.is-up').filter({ hasText: 'Second thing' }).count(), 1);
  await until('its last steps under the one it’s on', () => card('The list').locator('.lt-tool', { hasText: 'echo beta' }).count());
  await shot('1-card');
  ok('LIVETASK-1 the card shows the list, the current one lit, and the last things it did');

  // LIVETASK-5: a click opens the agent's conversation, not the task's page
  const layers = () => page.locator('.chat-layer').count();
  const under = await layers();
  await card('The list').locator('.msg-task.is-link').click();
  await until('its conversation', () => pane.count());
  assert.equal(await layers(), under, 'no page opened over the chat');
  assert.equal(await pane.locator('.agent-pane-title').filter({ hasText: 'The list' }).count(), 1);
  assert.equal(await pane.locator('.agent-pane-todos li').count(), 2, 'its to-dos on top');
  assert.equal(await pane.locator('.ss-plan').count(), 0, 'and not in the conversation as well');
  await shot('2-pane');
  ok('LIVETASK-5 a click opens its own conversation beside the chat');

  // LIVETASK-11, -15: a line fetches what it ran, and stays that call's as the to-dos change
  const alpha = pane.locator('.ss-tool', { hasText: 'echo alpha' });
  const planBefore = (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length;
  await alpha.locator('.ss-tool-row').click();
  await until('its output', () => alpha.locator('.ss-out', { hasText: 'alpha said this' }).count(), 10_000);
  ok('LIVETASK-11 a tool line opens to what it gave back');
  await until('a to-do ticked', async () => (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length > planBefore, 15_000);
  await until('the tick shown', () => pane.locator('.agent-pane-todos li.is-done').count(), 15_000);
  assert.equal(await pane.locator('.ss-tool.is-open').count(), 1);
  assert.equal(await pane.locator('.ss-tool.is-open', { hasText: 'echo alpha' }).count(), 1, 'the same call stays open');
  await shot('3-line-open');
  ok('LIVETASK-15 an opened line stays that call’s as the to-dos change');

  // LIVETASK-8: Esc closes it; beside the chat, and over it when narrow; the box under the chat
  await page.keyboard.press('Escape');
  await until('closed', async () => (await pane.count()) === 0);
  assert.equal(await page.locator('.chat-layer').count(), under, 'Esc closed only that');
  await card('The list').locator('.msg-task.is-link').click();
  await until('open again', () => pane.count());
  await size(1800, 800);
  const [column, composer, wide] = await Promise.all([box(page.locator('.chat-column').last()), box(page.locator('.chat-layer > .chat-composer').last()), box(pane)]);
  assert.ok(wide.width <= 460 && wide.right === 1800, `beside the chat: ${JSON.stringify(wide)}`);
  assert.ok(Math.abs((composer.left + composer.right) / 2 - (column.left + column.right) / 2) < 2, `the box under the chat: ${JSON.stringify({ column, composer })}`);
  // what Oby asks over the chat (.asks-over, Home's) leaves the pane and its bar in view
  const over = await page.evaluate(() => {
    const d = document.createElement('div');
    d.className = 'asks-over';
    d.style.height = '40px';
    document.querySelector('.home').append(d);
    const r = d.getBoundingClientRect(), bar = document.querySelector('.agent-pane-bar').getBoundingClientRect();
    d.remove();
    return { right: r.right, top: r.top, pane: bar.left, bar: bar.bottom };
  });
  assert.ok(over.right <= over.pane + 1 && over.top >= over.bar - 1, `asks clear of the pane: ${JSON.stringify(over)}`);
  await shot('4-wide');
  await size(620);
  const narrow = await box(pane);
  assert.equal(narrow.width, 620, 'over the chat');
  await shot('5-narrow');
  // the side panel open in a window too narrow for both: all of the pane's bar shows
  await page.getByRole('button', { name: 'Show sidebar' }).click();
  await sleep(500);
  const stopOr = await box(pane.getByRole('button', { name: 'Open the task’s page' }));
  const send = await box(pane.locator('.composer'));
  const edge = await page.evaluate(() => innerWidth);
  assert.ok(stopOr.right <= edge && send.right <= edge, `in view: ${JSON.stringify({ stopOr, send, edge })}`);
  await shot('6-sidebar-narrow');
  await page.getByRole('button', { name: 'Hide sidebar' }).click();
  await size(1180);
  ok('LIVETASK-8 beside the chat, over it when narrow, and in view with the side panel open');

  // LIVETASK-4: ended, one line
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${job.id}`)), 60_000);
  await until('the card folds', async () => (await card('The list').locator('.lt-now').count()) === 0, 15_000);
  await shot('7-ended');
  ok('LIVETASK-4 ended, the card folds');

  // LIVETASK-7: the box tells it, and the pane follows the follow-up
  assert.equal(await pane.getByRole('button', { name: 'Stop' }).count(), 0, 'nothing to stop');
  await type('.agent-pane [contenteditable="true"]', 'ECHO one more thing');
  const next = await until('the follow-up', async () => (await s.call('GET', '/v1/jobs')).find((j) => j.followUpOf === job.id), 30_000);
  await until('the pane follows it', () => pane.locator('.agent-pane-title').filter({ hasText: next.title }).count(), 15_000);
  await until('your words in it', () => pane.locator('.ss-you', { hasText: 'one more thing' }).count(), 15_000);
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${next.id}`)), 60_000);
  ok('LIVETASK-7 the box tells it, and a finished one’s follow-up is what it then follows');

  // Stop stops it, and the card says so
  await page.keyboard.press('Escape');
  await until('closed', async () => (await pane.count()) === 0);
  const stopping = await dispatch('The long one', 'SLEEP', '.chat-layer > .composer [contenteditable="true"]');
  await until('at work', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'running', 30_000);
  await card('The long one').locator('.msg-task.is-link').click();
  await until('its conversation', () => pane.locator('.agent-pane-title').filter({ hasText: 'The long one' }).count());
  await pane.getByRole('button', { name: 'Stop' }).click();
  await until('stopped', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'stopped', 30_000);
  await until('the card says so', () => card('The long one').locator('.msg-task-meta', { hasText: 'Stopped ·' }).count(), 15_000);
  assert.equal(await card('The long one').locator('[aria-label="Stopped"]').count(), 1);
  await until('no Stop any more', async () => (await pane.getByRole('button', { name: 'Stop' }).count()) === 0);
  await shot('8-stopped');
  ok('LIVETASK-5 Stop stops it, and the card says so');

  // LIVETASK-15: deleted, its conversation closes
  await s.call('POST', `/v1/jobs/${stopping.id}/delete`).catch(() => s.call('DELETE', `/v1/jobs/${stopping.id}`));
  await until('it closes', async () => (await pane.count()) === 0, 15_000);
  assert.equal(await card('The long one').count(), 0);
  ok('LIVETASK-15 a deleted job’s conversation closes');

  // LIVETASK-15, again where every list is in the session: the page's button, then Full session
  const second = await dispatch('The second list', 'TODOS', '.chat-layer > .composer [contenteditable="true"]');
  await card('The second list').locator('.msg-task.is-link').click();
  await until('its conversation', () => pane.locator('.agent-pane-title').filter({ hasText: 'The second list' }).count());
  await pane.getByRole('button', { name: 'Open the task’s page' }).click();
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
