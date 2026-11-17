// LIVETASK (docs/contracts/live-task.md), through the built app with real input: Electron → a real
// obid in a scratch home, with stand-in agents and a local fixture repository. Nothing reaches a real
// agent, account or GitHub. What only this proves: a task shows as a row nested under its conversation,
// its card and its row open its own thread over the conversation (never in place) and back returns, the
// thread's box tells it and its follow-up opens as its own thread, Stop in the box stops it, a tool line
// fetches its detail and stays that call's as the to-dos change, a deleted job's thread and card go, a
// real permission waits in the drawer over the message box with the card lean and Later only taking it off
// that drawer (LIVETASK-24 to 27), and the card's menu opens the task's page.
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
  // the side panel open, where the tasks' rows are
  if (await page.locator('button[aria-label="Show sidebar"]').count()) await page.locator('button[aria-label="Show sidebar"]').click();
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
  // a task's own thread, over the conversation (TaskThread), and its row nested under the conversation
  const thread = () => page.locator('.agent-layer:not(.is-leaving) .lt-thread');
  const kid = (title) => page.locator('.convo-kid').filter({ hasText: title });
  const back = async () => {
    await page.locator('.crumbs button.crumb').first().click();
    await until('back in the conversation', async () => (await page.locator('.agent-layer').count()) === 0);
  };
  const talkBox = '.chat-layer:not(.agent-layer) > .composer [contenteditable="true"]';

  // LIVETASK-19, -21: the task a row under its conversation; LIVETASK-1: the card says what it's on
  const job = await dispatch('The list', 'TODOS', '[contenteditable="true"]');
  await until('its row under the conversation', () => kid('The list').count(), 15_000);
  await until('working, its row says so', () => kid('The list').locator('.status-dot.is-working').count(), 30_000);
  assert.match(await kid('The list').getAttribute('title'), /^The list on Stub$/);
  await until('the conversation says an agent works', () => page.locator('.convo-row .connector-about', { hasText: '1 agent working' }).count(), 15_000);
  ok('LIVETASK-19, -21 the task is a row under its conversation, and the conversation says one agent works');
  await until('what it is on, on the card', () => card('The list').locator('.msg-task-now').count(), 30_000);
  assert.equal(await card('The list').locator('.lt-todos').count(), 0, 'no list on the card');
  await shot('1-card');
  ok('LIVETASK-1 the card says what it is on, and holds no list');

  // LIVETASK-5, -22: a click on the card opens its own thread over the conversation, never in place
  await card('The list').locator('.msg-task.is-link').click();
  await until('its own thread', () => thread().count());
  assert.equal(await card('The list').locator('.lt-thread').count(), 0, 'nothing opened in place');
  assert.equal(await card('The list').locator('[aria-expanded]').count(), 0);
  assert.match(await page.locator('.crumbs').innerText(), /The list/);
  await until('its row lit', () => kid('The list').and(page.locator('.is-active')).count());
  await until('its to-dos on top', async () => (await thread().locator('.lt-thread-todos li').count()) === 2, 30_000);
  assert.equal(await thread().locator('.ss-plan').count(), 0, 'and not in the conversation as well');
  await shot('2-thread');
  ok('LIVETASK-5, -22 a click on the card opens its own thread, its row lit');

  // LIVETASK-11, -15: a line fetches what it ran, and stays that call's as the to-dos change
  const alpha = thread().locator('.ss-tool', { hasText: 'echo alpha' });
  const planBefore = (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length;
  await alpha.locator('.ss-tool-row').click();
  await until('its output', () => alpha.locator('.ss-out', { hasText: 'alpha said this' }).count(), 10_000);
  ok('LIVETASK-11 a tool line opens to what it gave back');
  await until('a to-do ticked', async () => (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length > planBefore, 15_000);
  await until('the tick shown', () => thread().locator('.lt-thread-todos li.is-done').count(), 15_000);
  assert.equal(await thread().locator('.ss-tool.is-open').count(), 1);
  assert.equal(await thread().locator('.ss-tool.is-open', { hasText: 'echo alpha' }).count(), 1, 'the same call stays open');
  await shot('3-line-open');
  ok('LIVETASK-15 an opened line stays that call’s as the to-dos change');

  // LIVETASK-22: back returns to the conversation; its row opens the thread again
  await back();
  await kid('The list').click();
  await until('its thread again', () => thread().count());
  ok('LIVETASK-22 back returns to the conversation, and its row opens its thread again');

  // LIVETASK-4, -19: ended, one line on the card, Done on its row
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${job.id}`)), 60_000);
  await until('the card says so', () => card('The list').locator('.msg-task-meta', { hasText: /^(Done|Nothing done) ·/ }).count(), 15_000);
  await until('its row says so', () => kid('The list').locator('.convo-kid-text', { hasText: /· (Done|Nothing done)$/ }).count(), 15_000);
  await shot('7-ended');
  ok('LIVETASK-4, -19 ended, the card and its row say so');

  // LIVETASK-7: the thread's box tells it, and the follow-up opens as its own thread
  assert.equal(await thread().getByRole('button', { name: 'Stop' }).count(), 0, 'nothing to stop');
  await type(thread().locator('[contenteditable="true"]'), 'ECHO one more thing');
  const next = await until('the follow-up', async () => (await s.call('GET', '/v1/jobs')).find((j) => j.followUpOf === job.id), 30_000);
  await until('your words in its own thread', () => thread().locator('.ss-you', { hasText: 'one more thing' }).count(), 15_000);
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${next.id}`)), 60_000);
  ok('LIVETASK-7 the thread’s box tells it, and a finished one’s follow-up opens as its own thread');

  // LIVETASK-5: Stop in the thread's box stops it, and its card and row say so
  await back();
  const stopping = await dispatch('The long one', 'SLEEP', talkBox);
  await until('at work', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'running', 30_000);
  await until('its row', () => kid('The long one').count(), 15_000);
  await kid('The long one').click();
  await until('its thread', () => thread().count());
  await thread().getByRole('button', { name: 'Stop' }).click();
  await until('stopped', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'stopped', 30_000);
  await until('the card says so', () => card('The long one').locator('.msg-task-meta', { hasText: 'Stopped ·' }).count(), 15_000);
  await until('its row says so', () => kid('The long one').locator('[aria-label="Stopped"] .status-dot.is-failed').count(), 15_000);
  await until('no Stop any more', async () => (await thread().getByRole('button', { name: 'Stop' }).count()) === 0);
  await shot('8-stopped');
  ok('LIVETASK-5 Stop in the thread’s box stops it, and its card and row say so');

  // LIVETASK-15: deleted, its thread, card and row go with it
  await s.call('POST', `/v1/jobs/${stopping.id}/delete`).catch(() => s.call('DELETE', `/v1/jobs/${stopping.id}`));
  await until('it goes', async () => (await card('The long one').count()) === 0 && (await kid('The long one').count()) === 0 && (await page.locator('.agent-layer').count()) === 0, 15_000);
  ok('LIVETASK-15 a deleted job’s thread, card and row go with it');


  // LIVETASK-18: the card's menu (right-click), View details: the task's page; then, LIVETASK-15 again
  // where every list is in the session, Full session
  const second = await dispatch('The second list', 'TODOS', talkBox);
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

  // back from the task's page to the conversation
  await page.locator('.crumbs button.crumb').first().click();
  await until('the conversation', () => page.locator(talkBox).evaluate((el) => el.checkVisibility()).catch(() => false), 10_000);

  // LIVETASK-24 to 27: a real permission waits in the drawer over the message box, the card lean; Later
  // takes it off that drawer only, and the row, Waiting for you and the task's thread still have it
  // (a mode that asks: a new install runs in Bypass, #1602; last, since going back to Bypass takes a
  // signed click)
  await s.call('PUT', '/v1/settings/permissions', { mode: 'ask' });
  const asking = await dispatch('The ask', 'ASK: {"tool":"Bash","input":{"command":"curl https://example.invalid"}}', talkBox);
  await until('it asks', async () => (await s.call('GET', `/v1/jobs/${asking.id}`)).permission, 30_000);
  const drawer = page.locator('.chat-layer:not(.agent-layer):not(.is-leaving) .decision-bar');
  await until('the drawer over the box', () => drawer.count(), 15_000);
  assert.match(await drawer.locator('.tc-ask-line').innerText(), /^Permission\s+The ask/);
  assert.equal(await drawer.locator('.tc-ask-command', { hasText: 'curl https://example.invalid' }).count(), 1, 'its command on one line');
  assert.equal(await card('The ask').locator('.tc-ask, .tc-option').count(), 0, 'the card stays lean');
  assert.match(await card('The ask').locator('.msg-task-meta').innerText(), /^Needs you · /);
  await until('its row needs you', () => kid('The ask').locator('.status-dot.is-needs').count(), 15_000);
  await shot('9-drawer');
  ok('LIVETASK-24, -25 the decision waits on one line over the message box; the card is its head alone');
  await sleep(600);
  await drawer.getByRole('button', { name: 'Later', exact: true }).click();
  await until('off the drawer', async () => (await drawer.count()) === 0);
  await sleep(500);
  assert.ok((await s.call('GET', `/v1/jobs/${asking.id}`)).permission, 'it still asks');
  const inbox = await s.call('GET', '/v1/inbox');
  const items = Array.isArray(inbox) ? inbox : inbox.items ?? [];
  assert.ok(items.filter((i) => i.job === asking.id).every((i) => !i.later), 'nothing put off in obid');
  assert.equal(await kid('The ask').locator('.status-dot.is-needs').count(), 1, 'its row still needs you');
  assert.match(await card('The ask').locator('.msg-task-meta').innerText(), /^Needs you · /);
  await shot('10-later');
  await kid('The ask').click();
  await until('its thread', () => thread().count());
  const own = page.locator('.agent-layer:not(.is-leaving) .decision-bar');
  await until('the decision again, over its box', () => own.count(), 15_000);
  ok('LIVETASK-26 Later takes it off that drawer only: the row still needs you, and the task shows it again');
  await sleep(600);
  await own.locator('.tc-option', { hasText: /^Allow( once)?$/ }).first().click();
  await until('answered', async () => !(await s.call('GET', `/v1/jobs/${asking.id}`)).permission, 30_000);
  await until('one line where it was asked', () => thread().locator('.lt-answered', { hasText: /^Permission · Allow/ }).count(), 15_000);
  assert.equal(await own.count(), 0, 'the drawer goes');
  await shot('11-answered');
  ok('LIVETASK-27 answered, one line stays in its thread');

  assert.deepEqual(errors, [], 'no errors on the page');
}

main().then(
  async () => { await app.evaluate(({ app: a }) => a.exit(0)).catch(() => {}); daemon.kill(); await sleep(500); if (!process.env.OBY_TEST_KEEP) rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 }); console.log('passed'); process.exit(0); },
  async (e) => { console.error(e); try { const w = app?.windows()[0]; if (shots && w) await w.screenshot({ path: join(shots, 'failed.png') }); } catch {} await app?.evaluate(({ app: a }) => a.exit(0)).catch(() => {}); daemon?.kill(); process.exit(1); },
);
