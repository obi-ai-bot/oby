// LIVETASK (docs/contracts/live-task.md), through the built app with real input: Electron → a real
// obid in a scratch home, with stand-in agents and a local fixture repository. Nothing reaches a real
// agent, account or GitHub. What only this proves: a task shows as a row nested under its conversation;
// its card is progress only and what it did shows under it, in the conversation; a tool line there fetches
// its detail and stays that call's as the to-dos change; a nested row and a click on its card open its own
// thread over the conversation, its row lit (LIVETASK-22); an alert's click opens the conversation at its
// card, lit; the card's ⋯ tells its agent through the message box (its chip) and its
// follow-up lands in the conversation; Stop from the card's menu stops it; a deleted job's card and row go;
// a real permission waits in the drawer over the message box with the card lean, Later only taking it off
// that drawer until the conversation is opened at the task again, and its answer leaves one line under
// the card (LIVETASK-24 to 31).
// Screenshots go to OBY_LIVE_SHOTS (a folder), when it's set.
// Run: OBY_TEST_OBID=<obid> (else core/target/debug/obid), npm run build, then npm run test:live-task.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { buildFixture } from '../../../e2e/lib/fixture.mjs';
import { appRoot, launch, sleep, until, skipFirstRun } from './drive-lib.mjs';

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
  await skipFirstRun(page);
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
    await until(`the card of ${title}`, () => page.locator('.msg-task', { hasText: title }).count());
    return job;
  };
  // a task in the conversation: its card (progress only), and what it did under it
  const entry = (title) => page.locator('.chat-layer:not(.is-leaving) .msg-task-work').filter({ hasText: title });
  const card = (title) => entry(title).locator('> .msg-task');
  const log = (title) => entry(title).locator('> .lt-log');
  const kid = (title) => page.locator('.convo-kid').filter({ hasText: title });
  const talkBox = '.chat-layer:not(.is-leaving) > .composer [contenteditable="true"]';
  const menu = async (title, item) => {
    await card(title).getByRole('button', { name: `More for ${title}` }).click();
    await page.getByRole('menuitem', { name: item }).click();
  };
  // what main does when an alert is clicked (notify.ts, open): the place it's for, asked for by the page.
  // The macOS banner itself isn't driven here.
  const alertClicked = (place) => app.evaluate(({ ipcMain, BrowserWindow }, p) => {
    ipcMain.removeHandler('opened');
    let once = p;
    ipcMain.handle('opened', () => { const at = once; once = null; return at; });
    BrowserWindow.getAllWindows()[0].webContents.send('open');
  }, place);

  // LIVETASK-19, -21: the task a row under its conversation
  const job = await dispatch('The list', 'TODOS', '[contenteditable="true"]');
  await until('its row under the conversation', () => kid('The list').count(), 15_000);
  await until('working, its row says so', () => kid('The list').locator('.status-dot.is-working').count(), 30_000);
  assert.match(await kid('The list').getAttribute('title'), /^The list on Stub$/);
  await until('the conversation says an agent works', () => page.locator('.convo-row .connector-about', { hasText: '1 agent working' }).count(), 15_000);
  ok('LIVETASK-19, -21 the task is a row under its conversation, and the conversation says one agent works');

  // LIVETASK-1, -29: the card is progress only; its list and lines show under it, in the conversation
  await until('its to-dos under the card', async () => (await log('The list').locator('.ss-plan li').count()) === 2, 30_000);
  assert.equal(await card('The list').locator('.ss-entries, .ss-plan, .lt-log, .lt-todos').count(), 0, 'nothing of what it did in the card');
  assert.match(await card('The list').locator('.msg-task-meta').first().innerText(), /^(Running|Queued) · /);
  assert.equal(await card('The list').locator('.msg-task-stages').count(), 1, 'its stages as a bar');
  await shot('1-card-and-log');
  ok('LIVETASK-1, -29 the card is its head and stages; its list and lines are under it');

  // LIVETASK-11, -15: a line under the card fetches what it ran, and stays that call's as the to-dos change
  const more = log('The list').locator('.lt-log-more', { hasText: /^Show \d+ more$/ });
  if (await more.count()) await more.click();
  const alpha = log('The list').locator('.ss-tool', { hasText: 'echo alpha' });
  await until('the echo line', () => alpha.count(), 30_000);
  const planBefore = (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length;
  await alpha.locator('.ss-tool-row').click();
  await until('its output', () => alpha.locator('.ss-out', { hasText: 'alpha said this' }).count(), 10_000);
  ok('LIVETASK-11 a tool line under the card opens to what it gave back');
  await until('a to-do ticked', async () => (await s.call('GET', `/v1/jobs/${job.id}/transcript`)).entries.filter((e) => e.kind === 'plan').length > planBefore, 15_000);
  await until('the tick shown', () => log('The list').locator('.ss-plan li.is-done').count(), 15_000);
  assert.equal(await log('The list').locator('.ss-tool.is-open', { hasText: 'echo alpha' }).count(), 1, 'the same call stays open');
  await shot('2-line-open');
  ok('LIVETASK-15 an opened line stays that call’s as the to-dos change');

  // LIVETASK-22: from elsewhere (a new conversation), its nested row opens the task's own thread over the
  // conversation that started it, its row lit and all it did shown; back returns to the conversation, where
  // a click on its card opens that thread again
  const talk = await page.locator('.convo-row.is-active .connector-name').innerText();
  const own = page.locator(`.chat-layer:not(.is-leaving):not(.is-covered) [data-task="${job.id}"]`);
  const ownBox = page.locator('.chat-layer:not(.is-leaving):not(.is-covered) [aria-label="Tell the agent about The list"]');
  const back = async () => {
    await page.locator('.convo-row', { has: page.locator('.connector-name', { hasText: talk }) }).first().click();
    await until('back in its conversation', async () => (await page.locator('.convo-row.is-active .connector-name').count()) && (await page.locator('.convo-row.is-active .connector-name').innerText()) === talk, 10_000);
    await sleep(300);
  };
  await page.getByRole('button', { name: 'New conversation' }).first().click();
  await until('the conversation left', async () => (await page.locator('.convo-row.is-active').count()) === 0);
  if (!(await kid('The list').count())) await page.getByRole('button', { name: `Show ${talk}’s agents` }).click();
  await kid('The list').click();
  await until('its own thread, its box telling its agent', async () => (await own.count()) === 1 && (await ownBox.count()) === 1, 10_000);
  await until('its row lit', () => page.locator('.convo-kid.is-active', { hasText: 'The list' }).count(), 10_000);
  assert.equal(await own.locator('.lt-log-more', { hasText: /^Show \d+ more$/ }).count(), 0, 'all it did, nothing folded');
  await sleep(400);  // its way in over
  await shot('3-row-opens-own-thread');
  ok('LIVETASK-22 a nested row opens the task’s own thread, its row lit, all it did shown');
  await back();
  // a part of the card that is no button of its own (its title is one: below, from the keyboard)
  await card('The list').locator('.msg-task-meta').first().click();
  await until('its own thread again', async () => (await own.count()) === 1 && (await ownBox.count()) === 1, 10_000);
  await sleep(400);  // its way in over
  await shot('3b-card-opens-own-thread');
  ok('LIVETASK-22 a click on its card in the conversation opens its own thread');
  await back();
  // from the keyboard: its title is a button
  await card('The list').getByRole('button', { name: 'Open The list' }).focus();
  await page.keyboard.press('Enter');
  await until('its own thread from the keyboard', async () => (await own.count()) === 1 && (await ownBox.count()) === 1, 10_000);
  ok('LIVETASK-22 Enter on its card’s title opens its own thread');
  // LIVETASK-20 with the row lit: the twist still folds the conversation's rows
  const kids = page.locator('.convo-nest', { has: page.locator('.convo-row .connector-name', { hasText: talk }) }).locator('.convo-kids');
  await until('its rows shown, the lit one among them', async () => (await kids.count()) === 1, 10_000);
  await page.getByRole('button', { name: `Hide ${talk}’s agents` }).click();
  await until('folded while lit', async () => (await kids.count()) === 0, 10_000);
  await sleep(300);
  await shot('3c-twist-folds-while-lit');
  ok('LIVETASK-20 the twist folds the rows while its task’s own thread shows');
  // opening the thread again (its card) shows them once, the lit row among them
  await back();
  await card('The list').locator('.msg-task-meta').first().click();
  await until('its own thread again, its rows shown again', async () => (await own.count()) === 1 && (await page.locator('.convo-kid.is-active', { hasText: 'The list' }).count()) === 1, 10_000);
  await sleep(400);
  await shot('3d-card-reveals-rows');
  ok('LIVETASK-22 opening its own thread from its card shows its folded rows once, its row lit');
  await back();
  // from the keyboard: its title focused lights the card, as hovered
  const title = card('The list').getByRole('button', { name: 'Open The list' });
  await title.focus();
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Tab');
  await until('its title focused from the keyboard', () => title.evaluate((el) => el.matches(':focus-visible')), 5_000);
  await sleep(400);  // the card's own fade
  const lit = await card('The list').evaluate((el) => { const c = getComputedStyle(el); return { border: c.borderColor, background: c.backgroundColor }; });
  await page.mouse.move(2, 2);
  await page.keyboard.press('Tab');
  await sleep(400);
  const away = await card('The list').evaluate((el) => { const c = getComputedStyle(el); return { border: c.borderColor, background: c.backgroundColor }; });
  assert.notDeepEqual(lit, away, `the card lit while its title has focus (${JSON.stringify(lit)} vs ${JSON.stringify(away)})`);
  await title.focus();
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Tab');
  await sleep(300);
  await shot('3e-title-focused');
  ok('LIVETASK-22 its title focused from the keyboard lights the card');

  // LIVETASK-4, -31: ended, the card says so; under it, its last words with its lines folded
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${job.id}`)), 60_000);
  await until('the card says so', () => card('The list').locator('.msg-task-meta', { hasText: /^(Done|Nothing done) ·/ }).count(), 15_000);
  await until('its row says so', () => kid('The list').locator('.convo-kid-text', { hasText: /· (Done|Nothing done)$/ }).count(), 15_000);
  await until('its lines folded', async () => (await log('The list').locator('.ss-tool').count()) === 0 && (await log('The list').locator('.lt-log-more').count()) === 1, 15_000);
  assert.equal(await log('The list').locator('.ss-plan').count(), 0, 'no list once it ended');
  await shot('4-ended');
  ok('LIVETASK-4, -31 ended, the card says so, and its lines fold under it');

  // LIVETASK-30: the card's ⋯ tells its agent through the message box, its chip on it; the follow-up
  // lands in the conversation, and the box talks to Oby again
  await menu('The list', 'Ask for a follow-up…');
  await until('its chip over the box', () => page.locator('.composer-to', { hasText: 'The list' }).count());
  await shot('5-chip');
  await type(talkBox, 'ECHO one more thing');
  const next = await until('the follow-up', async () => (await s.call('GET', '/v1/jobs')).find((j) => j.followUpOf === job.id), 30_000);
  assert.equal(next.conversation, job.conversation, 'in the same conversation');
  await until('the chip gone', async () => (await page.locator('.composer-to').count()) === 0);
  await until('the follow-up’s card in the conversation', () => page.locator(`.chat-layer:not(.is-leaving) [data-task="${next.id}"]`).count(), 30_000);
  await until('it ends', async () => ended(await s.call('GET', `/v1/jobs/${next.id}`)), 60_000);
  ok('LIVETASK-30 the card’s menu tells its agent from the message box, and its follow-up lands in the conversation');

  // LIVETASK-30: Stop from the card's menu stops it, and its card and row say so
  const stopping = await dispatch('The long one', 'SLEEP', talkBox);
  await until('at work', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'running', 30_000);
  await until('its row', () => kid('The long one').count(), 15_000);
  await menu('The long one', 'Stop task');
  await until('stopped', async () => (await s.call('GET', `/v1/jobs/${stopping.id}`)).status === 'stopped', 30_000);
  await until('the card says so', () => card('The long one').locator('.msg-task-meta', { hasText: 'Stopped ·' }).count(), 15_000);
  await until('its row says so', () => kid('The long one').locator('[aria-label="Stopped"] .status-dot.is-failed').count(), 15_000);
  await shot('6-stopped');
  ok('LIVETASK-30 Stop in the card’s menu stops it, and its card and row say so');

  // LIVETASK-15: deleted, its card and row go with it
  await s.call('POST', `/v1/jobs/${stopping.id}/delete`).catch(() => s.call('DELETE', `/v1/jobs/${stopping.id}`));
  await until('it goes', async () => (await entry('The long one').count()) === 0 && (await kid('The long one').count()) === 0, 15_000);
  ok('LIVETASK-15 a deleted job’s card and row go with it');

  // LIVETASK-24 to 27: a real permission waits in the drawer over the message box, the card lean; Later
  // takes it off that drawer only, and an alert's click (LIVETASK-28) opens the conversation at the card
  // with it back (a mode that asks: a new install runs in Bypass, #1602; last, since going back to Bypass
  // takes a signed click)
  await s.call('PUT', '/v1/settings/permissions', { mode: 'ask' });
  const asking = await dispatch('The ask', 'ASK: {"tool":"Bash","input":{"command":"curl https://example.invalid"}}', talkBox);
  await until('it asks', async () => (await s.call('GET', `/v1/jobs/${asking.id}`)).permission, 30_000);
  const drawer = page.locator('.chat-layer:not(.is-leaving) .decision-bar');
  await until('the drawer over the box', () => drawer.count(), 15_000);
  // Sangam's card: a plain question naming its agent, the command once, on its own line under it
  assert.match(await drawer.locator('.tc-ask-line').innerText(), /^Permission\s+Let Claude Code run a command\?\s+·\s+The ask/);
  assert.equal(await drawer.locator('.tc-ask-command', { hasText: 'curl https://example.invalid' }).count(), 1, 'its command on one line');
  assert.equal(await entry('The ask').locator('.tc-ask, .tc-option').count(), 0, 'the card and what is under it stay lean');
  assert.match(await card('The ask').locator('.msg-task-meta').first().innerText(), /^Needs you · /);
  await until('its row needs you', () => kid('The ask').locator('.status-dot.is-needs').count(), 15_000);
  await shot('7-drawer');
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
  await shot('8-later');
  // an alert's click for it: the conversation at its card, lit, the decision back over the box
  await page.getByRole('button', { name: /Waiting for you/ }).first().click();
  await until('Waiting for you', () => page.locator('.group-layer:not(.is-leaving) .waiting-head').count());
  await alertClicked({ job: asking.id, conversation: asking.conversation });
  await until('its conversation at its card', () => page.locator(`.chat-layer:not(.is-leaving) .msg-task-work.is-lit[data-task="${asking.id}"]`).count(), 10_000);
  await until('the decision again, over the box', () => drawer.count(), 15_000);
  await shot('9-alert-opens-at-card');
  ok('LIVETASK-26, -28 an alert’s click opens the conversation at the card, and what Later put off is back');
  await sleep(600);
  await drawer.locator('.tc-option', { hasText: /^Allow( once)?$/ }).first().click();
  await until('answered', async () => !(await s.call('GET', `/v1/jobs/${asking.id}`)).permission, 30_000);
  await until('one line under the card where it was asked', () => log('The ask').locator('.lt-answered', { hasText: /^Permission · Allow/ }).count(), 15_000);
  assert.equal(await drawer.count(), 0, 'the drawer goes');
  await shot('10-answered');
  ok('LIVETASK-27 answered, one line stays under its card');

  // LIVETASK-19, -21, -24: an edit is asked as an edit, never as a command
  const editing = await dispatch('The edit', 'ASK: {"tool":"Edit","input":{"file_path":"notes.md","old_string":"a","new_string":"b"}}', talkBox);
  await until('it asks', async () => (await s.call('GET', `/v1/jobs/${editing.id}`)).permission, 30_000);
  await until('the drawer over the box', () => drawer.count(), 15_000);
  assert.match(await drawer.locator('.tc-ask-line').innerText(), /^Permission\s+Let Claude Code edit notes\.md\?\s+·\s+The edit/);
  await until('its row says it asks to edit a file', () => kid('The edit').filter({ hasText: 'asks to edit a file' }).count(), 15_000);
  await until('its conversation says so', () => page.locator('.convo-row .connector-about', { hasText: 'Claude Code asks to edit a file' }).count(), 15_000);
  await shot('11-edit');
  await drawer.locator('.tc-option', { hasText: /^Deny$/ }).first().click();
  await until('answered', async () => !(await s.call('GET', `/v1/jobs/${editing.id}`)).permission, 30_000);
  ok('LIVETASK-21, -24 an edit is asked as an edit, in a plain question');

  assert.deepEqual(errors, [], 'no errors on the page');
}

main().then(
  async () => { await app.evaluate(({ app: a }) => a.exit(0)).catch(() => {}); daemon.kill(); await sleep(500); if (!process.env.OBY_TEST_KEEP) rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 }); console.log('passed'); process.exit(0); },
  async (e) => { console.error(e); try { const w = app?.windows()[0]; if (shots && w) await w.screenshot({ path: join(shots, 'failed.png') }); } catch {} await app?.evaluate(({ app: a }) => a.exit(0)).catch(() => {}); daemon?.kill(); process.exit(1); },
);
