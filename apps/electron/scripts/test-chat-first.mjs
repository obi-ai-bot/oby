// CHATFIRST (docs/contracts/chat-first.md), through the built app with real input: Electron → a real
// obid in a scratch home, with stand-in agents and a local fixture repository, then the mock for what
// a scratch obid has none of (an Obysphere's teammates). Nothing reaches a real agent, account or GitHub.
// Screenshots go to OBY_CHAT_SHOTS (a folder), when it's set.
// Run: OBY_TEST_OBID=<obid> (else core/target/debug/obid), npm run build, then npm run test:chat-first.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { buildFixture } from '../../../e2e/lib/fixture.mjs';
import { appRoot, launch, sleep, until } from './drive-lib.mjs';

const repoRoot = resolve(appRoot, '../..');
const agents = join(repoRoot, 'core/crates/obid/tests/agents');
const binary = process.env.OBY_TEST_OBID || join(repoRoot, 'core/target/debug/obid');
const shots = process.env.OBY_CHAT_SHOTS;
const root = join(homedir(), 'Library/Application Support/oby-live-chat-first');
const home = join(root, 'home');
const json = (path, value) => writeFileSync(path, JSON.stringify(value), { mode: 0o600 });
const ok = (line) => console.log(`ok  ${line}`);
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
    listen: `127.0.0.1:${port}`, home, owner: 'wellington', telemetry: false,
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
  await until('the scratch obid', () => call('GET', '/v1/status'));
  return { port, call };
}

async function open(s) {
  app = await launch({ dir: root, userData: join(root, 'electron-data'), env: { ...process.env, OBID_HOME: home, OBID_PORT: String(s.port) } });
  const page = await app.firstWindow();
  page.on('pageerror', (e) => { throw new Error(`the page: ${e.message}`); });
  await page.waitForLoadState('domcontentloaded');
  return page;
}
const close = async () => { await app.evaluate(({ app: a }) => a.exit(0)).catch(() => {}); await sleep(800); };
// what's on top seen, its way in over: the box and the words truly at full opacity
const seen = (page) => until('what’s on top in full view', () => page.evaluate(() => {
  const els = [...document.querySelectorAll('.chat-layer:not(.is-leaving):not(.is-covered), .chat-layer:not(.is-leaving):not(.is-covered) .chat-thread, .chat-layer:not(.is-leaving):not(.is-covered) .composer')];
  return !els.length || els.every((el) => el.checkVisibility({ checkOpacity: true }) && getComputedStyle(el).opacity === '1');
}), 20_000);
const shot = async (page, name) => { await seen(page); await page.waitForTimeout(400); if (shots) { mkdirSync(shots, { recursive: true }); await page.screenshot({ path: join(shots, `${name}.png`) }); } };
const button = (page, name) => page.getByRole('button', { name, exact: true });
const visible = (page, selector) => page.locator(selector).filter({ visible: true });
const focusedBox = (page) => page.evaluate(() => {
  const el = document.activeElement;
  return !!el?.closest('.composer') && !el.closest('[inert]');
});
// the conversation on top, as its layer's own label says
const onTop = (page) => page.evaluate(() => [...document.querySelectorAll('.chat-layer:not(.is-leaving):not(.is-covered) section.chat')].at(-1)?.getAttribute('aria-label') ?? null);

async function main() {
  const s = await startObid();

  // CHATFIRST-10: first run, walked through with clicks, lands on the empty chat
  let page = await open(s);
  await button(page, 'Let’s connect your tools').click({ timeout: 30_000 });
  await until('the stand-in agent found', async () => {
    await page.locator('.onb-go').click();
    return (await page.locator('.onb-skip').count()) > 0;
  }, 30_000, 800);
  await shot(page, '10a-first-run-knowledge');
  await page.locator('.onb-skip').click();
  await button(page, 'Let’s go').click();
  await until('the hand over ends', async () => (await page.locator('.onb-go').count()) === 0, 15_000);
  await sleep(2500);
  const landed = visible(page, '.composer [contenteditable="true"]');
  await until('the box', async () => (await landed.count()) === 1);
  assert.equal(await page.locator('.oby-frame').count() > 0, true, 'CHATFIRST-10: Oby in place');
  assert.equal(await page.locator('.chip').count(), 0, 'CHATFIRST-10: no chips');
  assert.equal(await button(page, 'Show sidebar').count(), 1, 'CHATFIRST-10: its side panel closed, in setting up’s narrow window');
  await shot(page, '10b-first-run-landed');
  ok('CHATFIRST-10 first run lands on the empty chat');

  // CHATFIRST-2: a conversation that arrives meanwhile, as from the iPhone, doesn't move you off it
  await landed.click();
  const late = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', title: 'Came in later', task: 'Look around' } });
  await until('it’s listed', () => page.locator('.convo-row', { hasText: 'Came in later' }).count());
  await until('it ends', async () => ['done', 'failed', 'interrupted', 'stopped'].includes((await s.call('GET', `/v1/jobs/${late.id}`)).status), 120_000);
  await sleep(500);
  assert.equal(await onTop(page), null, 'CHATFIRST-2: still on the empty chat');
  assert.equal(await focusedBox(page), true, 'CHATFIRST-2: still in its box');
  ok('CHATFIRST-2 a conversation that arrives later doesn’t move you');
  // and words sent on it open their conversation
  await page.keyboard.insertText('OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"research","title":"Core flow","task":"Look around"}');
  await page.keyboard.press('Enter');
  await until('its conversation opens', async () => (await onTop(page)) !== null);
  const job = await until('a task from it', async () => (await s.call('GET', '/v1/jobs')).find((j) => j.title === 'Core flow'));
  await until('its card', () => page.locator('.msg-task.is-link', { hasText: 'Core flow' }).count());
  ok('CHATFIRST-2 words on the empty chat open their conversation');

  // CHATFIRST-6: a task from its card in the conversation, and back to it
  const talk = await onTop(page);
  await page.locator('.msg-task.is-link', { hasText: 'Core flow' }).click();
  await until('its page', () => page.locator('[aria-placeholder="Tell Oby about this task…"]').count());
  await shot(page, '6-task-from-its-card');
  await button(page, 'Back').click();
  await until('back in the conversation', async () => (await onTop(page)) === talk);
  ok('CHATFIRST-6 a task opens from its card, back returns to its conversation');
  await until('the task ends', async () => ['done', 'failed', 'interrupted', 'stopped'].includes((await s.call('GET', `/v1/jobs/${job.id}`)).status), 120_000);
  await close();

  // CHATFIRST-1: the next launch opens on it, box focused, the panel open (CHATFIRST-3's first time)
  page = await open(s);
  await until('its conversation', async () => (await onTop(page)) === talk, 30_000);
  assert.equal(await page.evaluate(() => !!document.querySelector('.stage.is-hidden')), true, 'CHATFIRST-1: no Home behind it');
  await until('its box focused', () => focusedBox(page));
  const t0 = Date.now();
  await seen(page);
  console.log(`    (its way in, in a hidden window: ${Date.now() - t0} ms)`);
  assert.equal(await button(page, 'Back').count(), 0, 'nothing to go back to from your conversation');
  assert.equal(await button(page, 'Hide sidebar').count(), 1, 'CHATFIRST-3: the side panel open');
  await shot(page, '1-opens-on-your-conversation');
  ok('CHATFIRST-1 opens on the latest conversation with its box focused');

  // CHATFIRST-3: Oby first, its usage rows at the foot, a click to the accounts
  const order = await page.locator('.sidebar-content').innerText();
  assert.ok(order.indexOf('Oby') < order.indexOf('New conversation'), order);
  assert.ok(order.indexOf('New conversation') < order.indexOf('Claude Code'), order);
  assert.ok(order.includes('–'), `no usage reported, no number: ${order}`);
  ok('CHATFIRST-3 the side panel lists Oby first and its usage at the foot');

  // CHATFIRST-9: Esc stays; a page from the side panel comes back to the conversation you were in
  await page.keyboard.press('Escape');
  await sleep(400);
  assert.equal(await onTop(page), talk, 'CHATFIRST-9: Esc stays');
  await button(page, 'Accounts and how much of each is used').click();
  await until('the accounts', async () => (await page.locator('.settings-layer, .set-page, [class*="settings"]').count()) > 0);
  await button(page, 'Back').click();
  await until('back in the conversation', async () => (await onTop(page)) === talk);
  await until('the panel back on the chats', async () => (await page.locator('.rail-button[aria-label="Oby"]').getAttribute('aria-pressed')) === 'true');
  ok('CHATFIRST-9 Esc stays, and back from a page returns to the conversation and the chats');

  // CHATFIRST-7: the kinds of work under Tasks
  await page.locator('.rail-button[aria-label="Tasks"]').click();
  await page.locator('.chat-kind', { hasText: 'Review PR' }).waitFor();
  await page.locator('.chat-kind', { hasText: 'Issues' }).waitFor();
  await shot(page, '7a-tasks-section');
  await page.locator('.chat-kind', { hasText: /^Tasks/ }).click();
  await until('its cards', () => page.locator('.group-layer:not(.is-leaving)').count());
  await shot(page, '7b-tasks-cards');
  ok('CHATFIRST-7 Review PR, Issues and Tasks open from the side panel');

  // CHATFIRST-8: what Oby asks shows over your conversation, answered there in a click
  const draft = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', title: 'Draft task', task: 'DRAFT: notes.md' } });
  await until('the draft task ends', async () => ['done', 'failed', 'stopped'].includes((await s.call('GET', `/v1/jobs/${draft.id}`)).status), 120_000);
  await button(page, 'Back').click();
  await until('back in the conversation', async () => (await onTop(page)) === talk);
  const row = page.locator('.task-row', { hasText: 'Draft task' });
  await row.waitFor();
  await row.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Delete task' }).click();
  await until('obid’s Confirm', async () => (await s.call('GET', '/v1/inbox')).some((i) => i.job === draft.id && i.state === 'open'));
  // (deleting from the side panel opened nothing: the conversation stays on top)
  const confirm = page.locator('.asks-over .ask-card.is-up', { hasText: 'Delete Draft task' }).getByRole('button', { name: 'Confirm', exact: true });
  await confirm.waitFor();
  await shot(page, '8-asks-over-the-chat');
  assert.equal(await onTop(page), talk, 'CHATFIRST-8: over your conversation');
  await sleep(600);
  await confirm.click();
  await until('obid deleted it', async () => !(await s.call('GET', '/v1/jobs')).some((j) => j.id === draft.id));
  ok('CHATFIRST-8 what Oby asks is answered over the conversation');

  // CHATFIRST-4: New conversation, one click from the panel, the box focused
  await page.locator('.rail-button[aria-label="Oby"]').click();
  await button(page, 'New conversation').click();
  await until('the empty chat', async () => (await page.evaluate(() => !document.querySelector('.stage.is-hidden'))));
  await until('its box focused', () => focusedBox(page));
  await shot(page, '4-new-conversation');
  // CHATFIRST-9: words half typed there, a page, and back: the empty chat, with them still in its box
  await page.keyboard.insertText('half a thought');
  await button(page, 'Accounts and how much of each is used').click();
  await button(page, 'Back').waitFor();
  await button(page, 'Back').click();
  await until('the empty chat again', async () => (await page.evaluate(() => !document.querySelector('.stage.is-hidden'))));
  await sleep(400);
  assert.equal(await onTop(page), null, 'CHATFIRST-9: not the conversation you’d left');
  assert.ok((await visible(page, '.composer [contenteditable="true"]').innerText()).includes('half a thought'), 'CHATFIRST-9: your words kept');
  await visible(page, '.composer [contenteditable="true"]').click();
  await page.keyboard.press('Meta+A');
  await page.keyboard.press('Backspace');
  ok('CHATFIRST-9 back from a page returns to the empty chat, your words kept');
  // and Oby's row, your latest conversation
  await page.locator('.chat-own').click();
  await until('your latest', async () => (await onTop(page)) !== null);
  ok('CHATFIRST-4 New conversation is one click, and Oby’s row goes back');

  // CHATFIRST-12: deleting the one you're in goes to your latest, and the last one to the empty chat
  const deleteOnTop = async () => {
    await button(page, 'Conversation options').click();
    await page.getByRole('menuitem', { name: 'Delete conversation' }).click();
  };
  const before = await onTop(page);
  await deleteOnTop();
  await until('your latest', async () => { const t = await onTop(page); return t !== null && t !== before; });
  await shot(page, '12a-deleted-goes-to-latest');
  // (the rest, one by one, each landing on the next)
  for (let left = (await s.call('GET', '/v1/conversations')).length; left > 0; left--) {
    const on = await onTop(page);
    await deleteOnTop();
    await until('it left', async () => (await onTop(page)) !== on);
  }
  await until('none left: the empty chat', async () => (await onTop(page)) === null && page.evaluate(() => !document.querySelector('.stage.is-hidden')));
  await visible(page, '.composer [contenteditable="true"]').waitFor();
  await shot(page, '12b-deleted-the-last');
  ok('CHATFIRST-12 deleting the conversation you’re in goes to your latest, then the empty chat');

  // CHATFIRST-11: Sangam's Home, as he left it, next to this
  const built = pathToFileURL(join(appRoot, 'out/renderer/index.html')).href;
  await page.goto(`${built}?home`);
  await page.locator('button.chip', { hasText: 'Review PR' }).waitFor();
  await sleep(600);
  assert.equal(await onTop(page), null, 'CHATFIRST-11: opens on no conversation');
  await shot(page, '11-home-kept');
  ok('CHATFIRST-11 ?home is Sangam’s Home, with its chips');

  // CHATFIRST-3 and CHATFIRST-5: the Obysphere and its teammates, on the mock's demo data
  await page.evaluate(() => localStorage.setItem('oby.welcomed', '1'));
  await page.goto(`${built}?everything&chat`);
  await page.locator('.chat-sphere', { hasText: 'Deskree' }).waitFor();
  await until('its conversation', async () => (await onTop(page)) !== null);
  await shot(page, '3-panel-with-teammates');
  const mine = await onTop(page);
  const text = await page.locator('.sidebar-content').innerText();
  assert.ok(text.indexOf('Oby') < text.indexOf('Deskree') && text.indexOf('Deskree') < text.indexOf('Dima’s Oby') && text.indexOf('Dima’s Oby') < text.indexOf('Claude Code'), text);
  await page.getByRole('button', { name: /^Chat with Dima’s Oby/ }).click();
  await page.locator('.crumb', { hasText: 'Dima’s Oby' }).waitFor();
  await shot(page, '5-a-teammates-oby');
  await button(page, 'Back').click();
  await until('back in your conversation', async () => (await onTop(page)) === mine);
  ok('CHATFIRST-3 and 5 the Obysphere and teammates list under Oby, and a teammate’s Oby opens and comes back');

  // CHATFIRST-8: at the window's smallest height a tall ask scrolls, so a real click reaches Later
  await button(page, 'Hide sidebar').click();
  await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(420, 520));
  await until('the small window', () => page.evaluate(() => innerWidth <= 420 && innerHeight <= 520));
  const over = page.locator('.asks-over');
  await sleep(500);
  const room = await over.evaluate((el) => ({ bottom: el.getBoundingClientRect().bottom, scroll: el.scrollHeight, client: el.clientHeight, window: innerHeight }));
  console.log(`    (the stack at 420×520: ${JSON.stringify(room)})`);
  assert.ok(room.bottom <= room.window, 'CHATFIRST-8: it stays in the window');
  await shot(page, '8b-ask-at-the-smallest-window');
  // the mock's tallest ask fits at 420×520 (271px: its text is clamped). One with more choices wrapping
  // wouldn't, and no card here is that tall: stretched, its choices below the fold, a real click reaches them
  await page.locator('.asks-over .ask-card.is-up').evaluate((el) => { el.style.paddingTop = '500px'; });
  await sleep(600);
  const tall = await over.evaluate((el) => ({ bottom: el.getBoundingClientRect().bottom, scroll: el.scrollHeight, client: el.clientHeight, window: innerHeight }));
  console.log(`    (stretched: ${JSON.stringify(tall)})`);
  assert.ok(tall.bottom <= tall.window, 'CHATFIRST-8: it stays in the window');
  assert.ok(tall.scroll > tall.client, 'CHATFIRST-8: taller than the window, it scrolls');
  const first = await page.locator('.asks-over .ask-card.is-up .tc-title').innerText();
  await sleep(600);
  await page.locator('.asks-over .ask-card.is-up').getByRole('button', { name: 'Later', exact: true }).click({ timeout: 10_000 });
  await until('the next card', async () => (await page.locator('.asks-over .ask-card.is-up .tc-title').innerText()) !== first);
  ok('CHATFIRST-8 an ask taller than the smallest window scrolls, its Later reachable');

  await close();
}

main().then(() => { console.log('test-chat-first: all passed'); }, (e) => { console.error(e); process.exitCode = 1; }).finally(() => {
  daemon?.kill();
  void app?.close().catch(() => {});
  // (once the app and obid have let go of their folders)
  setTimeout(() => {
    if (!process.env.OBY_TEST_KEEP) rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });
    process.exit();
  }, 1500);
});
