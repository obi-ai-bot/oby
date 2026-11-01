// Oby's core, end to end, through the real app: Electron → preload → main → signed HTTP → a real
// obid in a scratch home, with stand-in agents (core/crates/obid/tests/agents) and a local fixture
// repository. Nothing reaches a real agent, account or GitHub. What it proves, through the UI:
//   1. Words in Home's box start a task that runs to its end, its card in the conversation.
//   2. A task started on its own (POST /v1/jobs, as the Tasks view does) has a conversation of its
//      own (docs/contracts/task-conversations.md, TASKCONV-1).
//   3. A follow-up asked on that task's page lands in that conversation, with its card (TASKCONV-3).
//   4. A task with no conversation gets one when its page's words reach it, and the app opens it
//      (TASKCONV-4).
//   5. The owner's words go signed by this Mac's key (docs/contracts/signed-words.md): the task
//      from Home's box is theirs (vetted, WORDS-6), and so is the follow-up their words on its page
//      start.
//   6. Words the page's script sends with no keystroke or click go unsigned (WORDS-7).
//   7. The window is sandboxed (#1641, WORDS-9): its process runs in Chromium's sandbox, the page reaches
//      neither `process` nor `require`, and the preload's bridge still works.
//   8. A task deleted from the side panel on its Confirm (work that isn't committed, TASKS-11)
//      leaves the side panel as obid deletes it, with no reload (#1707).
// Screenshots of each step go to OBY_CORE_SHOTS (a folder), when it's set.
// Run: cargo build -p obid (or OBY_TEST_OBID=<obid>), npm run build, then npm run test:core.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { buildFixture } from '../../../e2e/lib/fixture.mjs';
import { appRoot, launch, sleep, until } from './drive-lib.mjs';

const repoRoot = resolve(appRoot, '../..');
const agents = join(repoRoot, 'core/crates/obid/tests/agents');
const binary = process.env.OBY_TEST_OBID || join(repoRoot, 'core/target/debug/obid');
const shots = process.env.OBY_CORE_SHOTS;
const scratch = mkdtempSync(join(tmpdir(), 'oby-core-'));
const json = (path, value) => writeFileSync(path, JSON.stringify(value), { mode: 0o600 });
let daemon, app;

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
  app = await launch({ dir: s.root, userData: join(s.root, 'electron-data'), env: { ...process.env, OBID_HOME: s.home, OBID_PORT: String(s.port) } });
  const page = await app.firstWindow();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  // the page's Content-Security-Policy (vite.config.ts) blocks nothing the app itself loads
  page.on('console', (m) => { if (m.type() === 'error' && /Content.Security.Policy/i.test(m.text())) errors.push(m.text()); });
  await page.waitForLoadState('domcontentloaded');
  // set up already (FirstRun.tsx), so Home shows rather than the first run's screens
  await page.evaluate(() => { for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1'); });
  await page.reload();
  await page.waitForLoadState('domcontentloaded');
  return { page, errors };
}

const shot = async (page, name) => { if (shots) { mkdirSync(shots, { recursive: true }); await page.screenshot({ path: join(shots, `${name}.png`) }); } };
// what the app shows now, as text
const shown = (page) => page.evaluate(() => document.body.innerText);
// types words into the message box with this placeholder, and sends them
async function send(page, placeholder, words) {
  const box = page.locator(`[contenteditable="true"][aria-placeholder="${placeholder}"]`).last();
  // shown, as a person sees it (a page fades in): only then do its words count as typed (the preload's check)
  await until('the box shown', () => box.evaluate((el) => el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })));
  await box.click();
  await page.keyboard.insertText(words);
  await page.keyboard.press('Enter');
}
const jobs = async (s) => s.call('GET', '/v1/jobs');
// the job as obid keeps it, `vetted` and all
const kept = (s, id) => JSON.parse(readFileSync(join(s.home, 'jobs', `${id}.json`), 'utf8'));
const ended = (j) => ['done', 'failed', 'interrupted', 'stopped'].includes(j.status);

async function main() {
  const s = await startObid();
  const { page, errors } = await openApp(s);
  await until('Home', async () => (await page.locator('.app').count()) > 0);
  await shot(page, '0-home');

  // 7. Sandboxed (#1641, WORDS-9): the page's process is in Chromium's sandbox, and the preload's
  // bridge (a sandboxed preload's, CommonJS) answers through main. getAppMetrics()'s `sandboxed` is
  // what proves the sandbox: the page lacks `process` and `require` with contextIsolation alone too
  const sandboxed = await app.evaluate(({ app: a, BrowserWindow }) => {
    const pid = BrowserWindow.getAllWindows()[0].webContents.getOSProcessId();
    return a.getAppMetrics().find((m) => m.pid === pid)?.sandboxed;
  });
  assert.equal(sandboxed, true, 'the window’s process runs sandboxed');
  const { bridge, ...reach } = await page.evaluate(async () => ({
    process: typeof process, require: typeof require, module: typeof module,
    bridge: Object.keys(window.app), fullScreen: typeof window.app.fullScreen,
    status: (await window.app.obid.call('GET', '/v1/status')).status,
  }));
  assert.deepEqual(reach, { process: 'undefined', require: 'undefined', module: 'undefined', fullScreen: 'boolean', status: 200 });
  for (const key of ['chrome', 'obid', 'notifications', 'updates', 'pickFolders', 'onSphere', 'sphereLeave']) assert.ok(bridge.includes(key), `window.app.${key}`);

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
  assert.equal(kept(s, first.id).vetted, true, 'WORDS-6: the task from Home’s box is the owner’s own words');
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

  // 5. Words on the page of the task from Home's box, signed: its follow-up is the owner's too
  await page.evaluate((id) => dispatchEvent(new CustomEvent('oby:open', { detail: { kind: 'task', id }, cancelable: true })), first.id);
  await until('the first task’s page', async () => (await page.locator('[aria-placeholder="Tell Oby about this task…"]').count()) > 0);
  await send(page, 'Tell Oby about this task…', `OBY oby_tell_job {"id":"${first.id}","words":"Look around once more"}`);
  const more = await until('its follow-up', async () => (await jobs(s)).find((j) => j.followUpOf === first.id));
  assert.equal(kept(s, more.id).vetted, true, 'WORDS-6: the follow-up the owner’s words start is theirs');
  assert.ok(!s.logs.join('').includes('not words the owner typed'), 'every signature obid was given checked out');

  // 6. Words the page's script sends with no keystroke or click of the owner's go unsigned
  // (WORDS-7): the follow-up they start isn't the owner's
  await until('the follow-up ends', async () => ended(await s.call('GET', `/v1/jobs/${more.id}`)), 120_000);
  await sleep(2500);
  const told = await page.evaluate((id) => window.app.obid.call('POST', `/v1/jobs/${id}/messages`, { text: 'Look around once more' }), more.id);
  assert.equal(told.status, 201, told.text);
  const unheld = await until('the follow-up the script’s words start', async () => kept(s, JSON.parse(told.text).id));
  assert.notEqual(unheld.vetted, true, 'WORDS-7: words no keystroke or click held go unsigned');

  // 8. Delete from the side panel, on a task whose work isn't committed: obid's Confirm first, and
  // on the owner's Confirm it leaves the side panel at once, with no reload (TASKS-11)
  const draft = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', title: 'Draft task', task: 'DRAFT: notes.md' } });
  await until('the draft task ends', async () => ended(await s.call('GET', `/v1/jobs/${draft.id}`)), 120_000);
  const row = page.locator('.task-row', { hasText: 'Draft task' });
  await until('its row in the side panel', async () => (await row.count()) === 1);
  // (the side panel may be folded away: its row's own right-click and menu, as a person's)
  await row.dispatchEvent('contextmenu');
  await page.getByRole('menuitem', { name: 'Delete task' }).evaluate((el) => el.click());
  await until('obid’s Confirm', async () => (await s.call('GET', '/v1/inbox')).some((i) => i.job === draft.id && i.state === 'open'));
  assert.equal(await row.count(), 1, 'TASKS-11: nothing is deleted before the Confirm');
  // Home's stack: its Confirm, once the pages the steps above opened are closed (New task)
  await page.getByRole('button', { name: 'New task', exact: true }).first().evaluate((el) => el.click());
  const confirm = page.locator('.ask-card.is-up', { hasText: 'Delete Draft task' }).getByRole('button', { name: 'Confirm', exact: true });
  await until('its Confirm on Home', async () => (await confirm.count()) === 1 && await confirm.evaluate((el) => !el.closest('[inert]')));
  // (a card takes no click in its first half second: TaskCard's pick)
  await sleep(600);
  await confirm.evaluate((el) => el.click());
  await until('obid deleted it', async () => !(await jobs(s)).some((j) => j.id === draft.id));
  await until('it left the side panel', async () => (await row.count()) === 0, 5000);
  await sleep(1000);
  assert.equal(await row.count(), 0, 'gone for good: the Confirm’s answer doesn’t bring it back');
  await shot(page, '8-deleted-on-its-confirm');

  assert.deepEqual(errors, [], 'no page errors');
  console.log('core: Home → task → its conversation → follow-ups, the owner’s words signed: all passed');
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
