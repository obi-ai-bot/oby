// An artifact in the real window (docs/contracts/artifacts-app.md), end to end: a real obid in a
// scratch home, a stand-in agent's task that makes a page (index.html and its stylesheet, under
// .obi/artifacts/), and the built app. Nothing reaches a real agent, account or the network.
//   ARTIFACTSAPP-4: the page's card shows under its task in the task's conversation, and a click
//                   opens it beside the conversation.
//   ARTIFACTSAPP-1: Sandpack draws it in main's preview view, its stylesheet applied, from
//                   <id>-preview.localhost on main's relay; every request the preview made went to
//                   its host page or that relay.
//   ARTIFACTSAPP-5: its Code tab shows each file of the version, index.html first.
//   ARTIFACTSAPP-6: the conversation gives it version 2, and the open pane moves there by itself.
//   ARTIFACTSAPP-8 to 10: an edit typed in Code shows in Preview unsaved; Save makes the next version
//                   (on an obid with the edit route; an older one's refusal shows, the edit kept);
//                   the version menu opens any version, an older one read-only.
//   ARTIFACTSAPP-11: Download saves the zip and the HTML, each byte for byte as obid exports it.
//   ARTIFACTSAPP-12: a page dropped on the thread is imported on the owner's Import, and opens.
//   ARTIFACTSAPP-3: the preview reaches nothing off its origin (another site, obid, the relay by
//                   127.0.0.1), by fetch, image, script or navigation, while its own files load; and
//                   it is sandboxed in its own partition, with no preload, no Node and no bridge.
// Screenshots go to OBY_ARTIFACTS_SHOTS (a folder) when it's set: the window, with the preview laid
// where main lays it.
// Run on the Mac: npm run build, then OBY_TEST_OBID=<obid> npm run test:artifacts
// (obid: core/target/debug/obid by default).
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { buildFixture } from '../../../e2e/lib/fixture.mjs';
import { appRoot, launch, until, skipFirstRun } from './drive-lib.mjs';

const repoRoot = resolve(appRoot, '../..');
const agents = join(repoRoot, 'core/crates/obid/tests/agents');
const binary = process.env.OBY_TEST_OBID || join(repoRoot, 'core/target/debug/obid');
const shots = process.env.OBY_ARTIFACTS_SHOTS;
const scratch = mkdtempSync(join(tmpdir(), 'oby-artifacts-'));
const json = (path, value) => writeFileSync(path, JSON.stringify(value), { mode: 0o600 });
let daemon, app;
const mainLogs = [];

async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  const { port } = server.address();
  await new Promise((done) => server.close(done));
  return port;
}

// A scratch obid: one stand-in Claude account, the fixture repository
async function startObid() {
  const root = scratch, home = join(root, 'home');
  for (const dir of ['home/accounts/stub', 'env-home', 'tmp', 'electron-data']) mkdirSync(join(root, dir), { recursive: true });
  const fixture = buildFixture(join(root, 'fixture'));
  json(join(home, 'accounts/stub/.credentials.json'), { claudeAiOauth: { accessToken: 'fake-claude', expiresAt: (Math.floor(Date.now() / 1000) + 86400) * 1000 } });
  const port = await freePort();
  json(join(root, 'config.json'), {
    listen: `127.0.0.1:${port}`, home, owner: 'alex',
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
    assert.ok(response.ok, `${method} ${path}: ${response.status}: ${text}`);
    return text ? JSON.parse(text) : null;
  };
  await until('the scratch obid', () => call('GET', '/v1/status'));
  return { root, home, port, call, logs };
}

// Main's preview view, and the frame Sandpack shows the artifact in, asked through main
async function inFrame(js) {
  return app.evaluate(async ({ webContents }, js) => {
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    const frame = wc?.mainFrame.framesInSubtree.find((f) => /^http:\/\/[a-z0-9]+-preview\.localhost:\d+$/.test(f.origin) && !f.url.includes('/__csb_relay/'));
    if (!frame) return { none: true };
    // a frame being replaced (the next version drawn) may never answer: asked again, never waited on
    const stalled = new Promise((done) => setTimeout(() => done({ none: true, stalled: true }), 5_000));
    return Promise.race([frame.executeJavaScript(js).then((value) => ({ value })), stalled]);
  }, js);
}
const inHost = (js) => app.evaluate(async ({ webContents }, js) => {
  const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
  return wc ? Promise.race([wc.executeJavaScript(js), new Promise((done) => setTimeout(() => done(null), 5_000))]) : null;
}, js);

// what a request from the preview's frame came to: loaded, or blocked before it got anything
const TRIES = (obidPort) => `(async () => {
  const fetched = (u) => fetch(u, { mode: 'no-cors' }).then(() => 'loaded', () => 'blocked');
  const image = (u) => new Promise((done) => { const i = new Image(); i.onload = () => done('loaded'); i.onerror = () => done('blocked'); i.src = u; });
  const script = (u) => new Promise((done) => { const s = document.createElement('script'); s.onload = () => done('loaded'); s.onerror = () => done('blocked'); s.src = u; document.head.append(s); });
  const relay = 'http://127.0.0.1:' + location.port + '/__csb_relay/';
  return {
    ownFile: await fetched('/style.css'),
    site: await fetched('https://example.com/'),
    obid: await fetched('http://127.0.0.1:${obidPort}/v1/status'),
    relayByAddress: await fetched(relay),
    otherPreview: await fetched('http://elsewhere-preview.localhost:' + location.port + '/index.html'),
    siteImage: await image('https://example.com/a.png'),
    siteScript: await script('https://example.com/a.js'),
    beacon: (navigator.sendBeacon('https://example.com/b'), 'tried'),
  };
})()`;

const shot = async (page, name) => {
  if (!shots) return;
  mkdirSync(shots, { recursive: true });
  // the window's page, with main's preview view drawn where main lays it (a page screenshot holds only the page)
  const png = await app.evaluate(async ({ BrowserWindow, webContents }) => {
    const win = BrowserWindow.getAllWindows().find((w) => !w.webContents.getURL().endsWith('/preview.html'));
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    const view = win?.contentView.children.find((c) => c.webContents === wc);
    const page = await win.webContents.capturePage();
    if (!view || !view.getVisible()) return { page: page.toPNG().toString('base64') };
    return { page: page.toPNG().toString('base64'), preview: (await wc.capturePage()).toPNG().toString('base64'), at: view.getBounds() };
  });
  writeFileSync(join(shots, `${name}.png`), Buffer.from(png.page, 'base64'));
  if (png.preview) {
    writeFileSync(join(shots, `${name}-preview.png`), Buffer.from(png.preview, 'base64'));
    writeFileSync(join(shots, `${name}-preview-at.json`), JSON.stringify(png.at));
  }
};

async function main() {
  const s = await startObid();
  // a task that makes a page, its stylesheet beside it (the stand-in's ARTIFACTS)
  const made = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'build', task: 'ARTIFACTS' } });
  const job = await until('the task to end', async () => { const j = await s.call('GET', `/v1/jobs/${made.id}`); return ['done', 'failed'].includes(j.status) && j; }, 120_000);
  const artifacts = await s.call('GET', `/v1/artifacts?conversation=${made.conversation}`);
  const pageArtifact = artifacts.find((a) => a.title === 'page');
  assert.ok(pageArtifact, `the task made its page: ${JSON.stringify(artifacts.map((a) => a.title))} (${job.status})`);
  assert.equal(pageArtifact.kind, 'html');

  app = await launch({ dir: s.root, userData: join(s.root, 'electron-data'), env: { ...process.env, OBID_HOME: s.home, OBID_PORT: String(s.port) } });
  const page = await app.firstWindow();
  // main's own output, said when the run fails
  app.process().stdout?.on('data', (b) => mainLogs.push(b.toString()));
  app.process().stderr?.on('data', (b) => mainLogs.push(b.toString()));
  app.process().on('exit', (code, signal) => mainLogs.push(`main exited: ${code} ${signal}`));
  // everything that ends the app or a window, each with where it came from (a quit nobody asked
  // for is a bug, never noise)
  await app.evaluate(({ app: a, BrowserWindow }) => {
    const said = (what) => console.log(`[ends] ${what} at ${new Date().toISOString()}\n${new Error().stack?.split('\n').slice(2, 9).join('\n')}`);
    for (const ev of ['before-quit', 'will-quit', 'quit', 'window-all-closed']) a.on(ev, () => said(`app ${ev}`));
    a.on('render-process-gone', (_e, wc, d) => said(`render-process-gone ${wc.getURL()} ${JSON.stringify(d)}`));
    a.on('child-process-gone', (_e, d) => said(`child-process-gone ${JSON.stringify(d)}`));
    const watch = (w) => w.on('closed', () => said(`window closed`));
    BrowserWindow.getAllWindows().forEach(watch);
    a.on('browser-window-created', (_e, w) => watch(w));
    process.on('SIGTERM', () => said('SIGTERM'));
    process.on('SIGINT', () => said('SIGINT'));
  });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.waitForLoadState('domcontentloaded');
  await skipFirstRun(page);
  await until('Home', async () => (await page.locator('.app').count()) > 0).catch(async (e) => {
    console.error('the window shows:', (await page.evaluate(() => document.body.innerText).catch(() => '')).slice(0, 600));
    console.error('obid says:', s.logs.join('').slice(-1500));
    throw e;
  });
  // the window at its own size (main/index.ts, 994 by 670), as the owner has it once set up
  await until('the window at its size', () => app.evaluate(({ BrowserWindow }) => {
    const win = BrowserWindow.getAllWindows()[0];
    win.setResizable(true);
    win.setSize(994, 670);
    return win.getContentSize()[0] >= 994;
  }));
  // every request the preview's session sees, from before it first loads
  await app.evaluate(({ session }) => {
    globalThis.__previewRequests = [];
    globalThis.__previewSent = [];
    const requests = session.fromPartition('oby-artifact-preview').webRequest;
    requests.onCompleted((d) => { globalThis.__previewRequests.push(d.url); });
    // what actually went out: its headers sent (main's own check cancels a request before that)
    requests.onSendHeaders((d) => { globalThis.__previewSent.push(d.url); });
  });

  // ARTIFACTSAPP-4: the card, under its task in the task's conversation, opens the pane
  const row = page.locator('.convo-row', { hasText: job.title || 'ARTIFACTS' }).first();
  await until('the task’s conversation in the side panel', async () => (await row.count()) > 0);
  await row.evaluate((el) => el.click());
  const card = page.locator('button.art-card', { hasText: 'page' });
  await until('the page’s card', async () => (await card.count()) > 0);
  const meta = await card.locator('.art-card-meta').innerText();
  assert.match(meta, /^Oby artifact · Page · v1( · made by Claude Code)?$/, `artifactsapp_4: the card says what it is: ${meta}`);
  await shot(page, '1-card');
  await card.click();
  await until('the pane', async () => (await page.locator(`[data-artifact-view="${pageArtifact.id}"]`).count()) > 0);
  assert.equal(await page.locator('.art-strip-title b').innerText(), 'page', 'artifactsapp_4: the pane says which');
  console.log('ok artifactsapp_4: the card under its task opens the pane');

  // ARTIFACTSAPP-1: Sandpack draws the page from <id>-preview.localhost, its stylesheet applied
  const drawn = await until('the page drawn in the preview', async () => {
    const r = await inFrame(`(() => { const h = document.querySelector('h1'); return h && { text: h.textContent, color: getComputedStyle(h).color, origin: location.origin }; })()`);
    // drawn once its stylesheet has loaded too (a link loads after the heading is there)
    return r.value?.color === 'rgb(0, 128, 128)' && r.value;
  }, 60_000).catch(async (e) => { console.error('drawn so far:', JSON.stringify((await inFrame(`(() => { const h = document.querySelector('h1'); return h && { text: h.textContent, color: getComputedStyle(h).color }; })()`)).value)); throw e; });
  assert.equal(drawn.text, 'Hello');
  assert.equal(drawn.color, 'rgb(0, 128, 128)', 'artifactsapp_1: its stylesheet (h1 teal) applies');
  assert.match(drawn.origin, /^http:\/\/[a-z0-9]+-preview\.localhost:\d+$/, `artifactsapp_1: from a preview origin: ${drawn.origin}`);
  const relayPort = Number(new URL(drawn.origin).port);
  const visible = await app.evaluate(({ BrowserWindow, webContents }) => {
    const win = BrowserWindow.getAllWindows().find((w) => !w.webContents.getURL().endsWith('/preview.html'));
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    const view = win.contentView.children.find((c) => c.webContents === wc);
    return view && { visible: view.getVisible(), bounds: view.getBounds() };
  });
  const box = await page.locator(`[data-artifact-view="${pageArtifact.id}"]`).boundingBox();
  assert.ok(visible?.visible, 'artifactsapp_1: main’s view shows');
  for (const k of ['x', 'y', 'width', 'height']) assert.ok(Math.abs(visible.bounds[k] - box[k]) <= 1, `artifactsapp_1: the view lies on the pane’s box (${k}: ${visible.bounds[k]} vs ${box[k]})`);
  await shot(page, '2-preview');
  const requests = await app.evaluate(() => globalThis.__previewRequests);
  assert.ok(requests.length > 0, 'the preview made requests');
  const off = requests.filter((u) => !u.startsWith('file://') && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u) && !u.startsWith('data:') && !u.startsWith('devtools:'));
  assert.deepEqual(off, [], 'artifactsapp_1: every request the preview made went to its host page or the relay');
  console.log(`ok artifactsapp_1: a page artifact draws in the real window, from loopback only (${requests.length} requests)`);

  // ARTIFACTSAPP-3 (1): nothing off its origin, while its own files load
  const tries = (await inFrame(TRIES(s.port))).value;
  assert.deepEqual(tries, {
    ownFile: 'loaded', site: 'blocked', obid: 'blocked', relayByAddress: 'blocked', otherPreview: 'blocked',
    siteImage: 'blocked', siteScript: 'blocked', beacon: 'tried',
  }, `artifactsapp_3: the preview reaches only its own origin: ${JSON.stringify(tries)}`);
  // and nothing of it left: no request the preview's session sent went anywhere but its own origin
  // or its host page (a beacon says it's queued whatever the CSP then does with it)
  await new Promise((r) => setTimeout(r, 1000));
  const sent = await app.evaluate(() => globalThis.__previewSent);
  const away = sent.filter((u) => !u.startsWith('file://') && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u));
  assert.deepEqual(away, [], `artifactsapp_3: nothing the preview tried left for another origin: ${away.join(', ')}`);
  // the host page reaches nothing either
  const hostTries = await inHost(`fetch('https://example.com/', { mode: 'no-cors' }).then(() => 'loaded', () => 'blocked')`);
  assert.equal(hostTries, 'blocked', 'artifactsapp_3: the host page fetches nothing outside');
  // a navigation, which no CSP stops, is stopped by the session: the frame never gets there
  console.log('ok artifactsapp_3: the preview reaches nothing off its origin');

  // ARTIFACTSAPP-3 (2): sandboxed, its own partition, no preload, no Node, no bridge
  const isolation = await app.evaluate(async ({ app: a, BrowserWindow, session, webContents }) => {
    const win = BrowserWindow.getAllWindows().find((w) => !w.webContents.getURL().endsWith('/preview.html'));
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    const prefs = wc.getLastWebPreferences();
    const pid = wc.getOSProcessId();
    const metric = a.getAppMetrics().find((m) => m.pid === pid);
    // a cookie the window's session holds for a preview host never reaches the preview's
    await session.defaultSession.cookies.set({ url: 'http://probe-preview.localhost/', name: 'oby_probe', value: '1' });
    const theirs = await wc.session.cookies.get({ name: 'oby_probe' });
    return {
      ownSession: wc.session !== win.webContents.session && wc.session === session.fromPartition('oby-artifact-preview'),
      persistent: wc.session.isPersistent(),
      sandbox: prefs.sandbox, contextIsolation: prefs.contextIsolation, nodeIntegration: prefs.nodeIntegration,
      nodeIntegrationInSubFrames: prefs.nodeIntegrationInSubFrames, preload: prefs.preload ?? null, webviewTag: prefs.webviewTag,
      sandboxed: metric?.sandboxed, cookieCrossed: theirs.length > 0,
      samePid: pid === win.webContents.getOSProcessId(),
    };
  });
  assert.deepEqual(isolation, {
    ownSession: true, persistent: false, sandbox: true, contextIsolation: true, nodeIntegration: false,
    nodeIntegrationInSubFrames: false, preload: null, webviewTag: false, sandboxed: true, cookieCrossed: false, samePid: false,
  }, `artifactsapp_3: the preview's view: ${JSON.stringify(isolation)}`);
  const reach = `({ process: typeof process, require: typeof require, bridge: typeof window.app, parentBridge: (() => { try { return typeof window.parent.app; } catch { return 'cross-origin'; } })() })`;
  const frameReach = (await inFrame(reach)).value;
  assert.deepEqual(frameReach, { process: 'undefined', require: 'undefined', bridge: 'undefined', parentBridge: 'cross-origin' }, `artifactsapp_3: the frame reaches no Node and no bridge: ${JSON.stringify(frameReach)}`);
  const hostReach = await inHost(reach);
  assert.deepEqual({ ...hostReach, parentBridge: undefined }, { process: 'undefined', require: 'undefined', bridge: 'undefined', parentBridge: undefined }, `artifactsapp_3: the host page reaches no Node and no bridge: ${JSON.stringify(hostReach)}`);
  console.log('ok artifactsapp_3: the preview is sandboxed in its own partition with no way into the app');

  // ARTIFACTSAPP-3 (1): a navigation of the frame (it leaves the frame on an error page; the next
  // version shown makes a new one):
  // a navigation of the frame: the host page's CSP (frame-src) stops it before any request, and
  // the frame is left on Chromium's error page, an opaque origin, with nothing of the site
  await app.evaluate(({ webContents }) => {
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    globalThis.__failedLoads = [];
    wc.on('did-fail-provisional-load', (_e, code, desc, url) => { globalThis.__failedLoads.push({ url, desc }); });
  });
  await inFrame(`location.href = 'https://example.com/'; true`);
  const failed = await until('the navigation refused', () => app.evaluate(() => globalThis.__failedLoads.find((f) => f.url === 'https://example.com/'))).catch(async (e) => {
    console.error('failed loads:', JSON.stringify(await app.evaluate(() => globalThis.__failedLoads)));
    console.error('frames:', JSON.stringify(await app.evaluate(({ webContents }) => webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html')).mainFrame.framesInSubtree.map((f) => [f.url, f.origin]))));
    throw e;
  });
  assert.match(failed.desc, /^ERR_BLOCKED_BY_(CSP|CLIENT)$/, `artifactsapp_3: the navigation was refused: ${failed.desc}`);
  const frames = await app.evaluate(({ webContents }) => {
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    return wc.mainFrame.framesInSubtree.map((f) => ({ url: f.url, origin: f.origin }));
  });
  assert.ok(frames.every((f) => !f.origin.includes('example.com')), `artifactsapp_3: no frame holds the site: ${JSON.stringify(frames)}`);
  const sentAfter = await app.evaluate(() => globalThis.__previewSent);
  assert.ok(!sentAfter.some((u) => u.includes('example.com')), 'artifactsapp_3: the navigation sent nothing');
  console.log('ok artifactsapp_3: a navigation of the preview goes nowhere');

  // ARTIFACTSAPP-5: Code shows the version's files, index.html first, and Preview's view goes meanwhile
  const viewShown = () => app.evaluate(({ BrowserWindow, webContents }) => {
    const win = BrowserWindow.getAllWindows().find((w) => !w.webContents.getURL().endsWith('/preview.html'));
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    return !!win.contentView.children.find((c) => c.webContents === wc)?.getVisible();
  });
  await page.getByRole('tab', { name: 'Code' }).click();
  await until('the page’s code', async () => (await page.locator('[data-artifact-code="index.html"] .cm-content').innerText().catch(() => '')).includes('<h1>Hello</h1>'));
  assert.deepEqual(await page.locator('.art-file').allInnerTexts(), ['index.html', 'style.css'], 'artifactsapp_5: a tab per file, index.html first');
  await until('Preview’s view gone while Code shows', async () => !(await viewShown()));
  await shot(page, '3-code');
  await page.getByRole('tab', { name: 'style.css' }).click();
  await until('the stylesheet’s code', async () => (await page.locator('[data-artifact-code="style.css"] .cm-content').innerText().catch(() => '')).includes('h1 { color: teal }'));
  await page.getByRole('tab', { name: 'Preview' }).click();
  await until('Preview’s view back', viewShown);
  console.log('ok artifactsapp_5: Code shows each file of the version');

  // ARTIFACTSAPP-6: what the conversation's agent makes opens by itself, and its next version moves
  // the pane there. Published with the conversation's own oby_artifact from its room, which any obid
  // with artifacts takes (a version across a conversation's jobs, ARTIFACTS-7, is newer than the
  // installed one this may run on)
  const room = join(s.home, 'conversations', made.conversation, 'room', 'site');
  const publish = async (h1, artifact) => {
    mkdirSync(room, { recursive: true });
    writeFileSync(join(room, 'index.html'), `<!doctype html><h1>${h1}</h1>`);
    const said = await s.call('POST', '/v1/plans', { said: `OBY oby_artifact ${JSON.stringify({ title: 'site', path: 'site', ...(artifact ? { artifact } : {}) })}`, conversation: made.conversation });
    assert.match(said.text ?? '', /Published artifact /, `the conversation published it: ${said.text}`);
    return said;
  };
  await page.getByRole('button', { name: 'Close the artifact' }).click();
  await until('the pane closed', async () => (await page.locator('.art-strip-title').count()) === 0);
  const first = await publish('One');
  const site = /Published artifact ([0-9a-f-]{36})/.exec(first.text)[1];
  await until('the new artifact open by itself', async () => (await page.locator('.art-strip-title b').innerText().catch(() => '')) === 'site', 60_000);
  assert.equal(await until('its version 1 drawn', async () => (await inFrame(`document.querySelector('h1')?.textContent ?? null`)).value, 60_000), 'One');
  await publish('Two', site);
  await until('the pane on version 2', async () => (await page.locator('.art-strip-title span').innerText()) === 'Oby artifact · Page · v2', 60_000);
  assert.equal(await until('version 2 drawn', async () => { const v = (await inFrame(`document.querySelector('h1')?.textContent ?? null`)).value; return v === 'Two' && v; }, 60_000), 'Two', 'artifactsapp_6: the preview draws version 2');
  await shot(page, '4-followed');
  console.log('ok artifactsapp_6: a new version from the agent moves the open pane to it');

  // ARTIFACTSAPP-8 and 10: the owner types in Code; Preview shows the edit unsaved; Save sends it,
  // signed by main on the click of "Save as v3". An obid with the edit route (ARTIFACTS-16, it answers
  // an unsigned edit 401) makes version 3; an older one answers 404, which the pane says, the edit kept
  const token = readFileSync(join(s.home, 'api-token'), 'utf8').trim();
  const probe = await fetch(`http://127.0.0.1:${s.port}/v1/artifacts/${site}/versions`, { method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: '{}' });
  const edits = probe.status === 401;
  await page.getByRole('tab', { name: 'Code' }).click();
  const editor = page.locator('[data-artifact-code="index.html"] .cm-content');
  await until('the editor', async () => (await editor.innerText().catch(() => '')).includes('<h1>Two</h1>'));
  await editor.click();
  await page.keyboard.press('Meta+A');
  await page.keyboard.insertText('<!doctype html><h1>Three</h1>');
  const saveButton = page.getByRole('button', { name: 'Save as v3', exact: true });
  await until('Save as v3', async () => (await saveButton.count()) === 1);
  await page.getByRole('tab', { name: 'Preview' }).click();
  assert.equal(await until('the unsaved edit drawn', async () => { const v = (await inFrame(`document.querySelector('h1')?.textContent ?? null`)).value; return v === 'Three' && v; }, 60_000), 'Three', 'artifactsapp_8: Preview shows the edit before it is saved');
  await shot(page, '5-edited');
  await saveButton.click();
  if (edits) {
    await until('the pane on version 3', async () => (await page.locator('.art-strip-title span').innerText()) === 'Oby artifact · Page · v3', 60_000);
    const v3 = await s.call('GET', `/v1/artifacts/${site}`);
    assert.equal(v3.versions.at(-1).by, 'owner', 'artifactsapp_8: version 3 is the owner’s');
    assert.equal(await (await fetch(`http://127.0.0.1:${s.port}/v1/artifacts/${site}/3/index.html`, { headers: { Authorization: `Bearer ${token}` } })).text(), '<!doctype html><h1>Three</h1>');
    console.log('ok artifactsapp_8: an edit shows in Preview unsaved, and Save makes version 3, the owner’s');
  } else {
    await until('obid’s refusal shown', async () => (await page.locator('.art-refused').count()) === 1);
    assert.equal(await saveButton.count(), 1, 'artifactsapp_10: the edit stays to save again');
    console.log(`ok artifactsapp_8 (Preview of the unsaved edit) and artifactsapp_10 (refused: ${await page.locator('.art-refused').innerText()}); this obid has no edit route (${probe.status}), so Save isn't checked here`);
  }
  await shot(page, '6-saved');

  // ARTIFACTSAPP-9: the version menu, the latest marked; an older version is read-only
  const menu = page.getByRole('combobox', { name: 'Version' });
  const options = await menu.locator('option').allInnerTexts();
  assert.equal(options[0], `v${options.length}, the latest`, `artifactsapp_9: the latest first and marked: ${options.join(', ')}`);
  await menu.selectOption('1');
  await until('version 1 on view', async () => (await page.locator('.art-strip-title span').innerText()) === 'Oby artifact · Page · v1');
  await page.getByRole('tab', { name: 'Code' }).click();
  await until('version 1’s code', async () => (await editor.innerText().catch(() => '')).includes('<h1>One</h1>'));
  assert.equal(await editor.getAttribute('contenteditable'), 'false', 'artifactsapp_9: an older version is read-only');
  await page.getByRole('tab', { name: 'Preview' }).click();
  await until('version 1 drawn again', async () => (await inFrame(`document.querySelector('h1')?.textContent ?? null`)).value === 'One', 60_000);
  console.log('ok artifactsapp_9: the version menu opens any version, an older one read-only');

  // ARTIFACTSAPP-11: Download saves obid's export of the version on view, byte for byte, where main's
  // own dialog answers (the test answers it for the owner, as no one can click a hidden window's sheet)
  const exportOf = async (as) => new Uint8Array(await (await fetch(`http://127.0.0.1:${s.port}/v1/artifacts/${site}/export/1?as=${as}`, { headers: { Authorization: `Bearer ${token}` } })).arrayBuffer());
  for (const [as, item] of [['zip', 'Project archive (.zip)'], ['html', 'Standalone HTML']]) {
    const to = join(s.root, `saved.${as}`);
    await app.evaluate(({ dialog }, to) => { dialog.showSaveDialog = async () => ({ canceled: false, filePath: to }); }, to);
    await page.getByRole('button', { name: 'Export' }).click();
    await page.getByRole('menuitem', { name: item }).click();
    await until(`the ${as} saved`, async () => (await page.locator('.art-saved').innerText().catch(() => '')) === `Saved to ${to}`);
    assert.deepEqual([...readFileSync(to)], [...await exportOf(as)], `artifactsapp_11: the ${as} saved is obid's export, byte for byte`);
  }
  console.log('ok artifactsapp_11: Download saves the version as obid exports it, zip and HTML');

  // ARTIFACTSAPP-12: a page dropped on the thread is offered; the owner's click on Import sends it,
  // signed by main (obid refuses an unsigned import), and it opens beside the conversation, drawn
  await page.evaluate((html) => {
    const data = new DataTransfer();
    data.items.add(new File([html], 'mine.html', { type: 'text/html' }));
    const thread = document.querySelector('.chat-scroll');
    for (const type of ['dragover', 'drop']) thread.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: data }));
  }, '<!doctype html><h1>Mine</h1>');
  await until('the import offered', async () => (await page.locator('[data-artifact-import]').innerText().catch(() => '')).includes('mine.html'));
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await until('the import open', async () => (await page.locator('.art-strip-title b').innerText().catch(() => '')) === 'Imported', 60_000);
  assert.equal(await until('the import drawn', async () => { const v = (await inFrame(`document.querySelector('h1')?.textContent ?? null`)).value; return v === 'Mine' && v; }, 60_000), 'Mine');
  const imported = (await s.call('GET', '/v1/artifacts')).find((a) => a.origin?.kind === 'import');
  assert.ok(imported, 'artifactsapp_12: obid keeps it as an import');
  assert.equal(imported.origin.from, 'page');
  await shot(page, '7-imported');
  console.log('ok artifactsapp_12: a page dropped and imported on Import opens beside the conversation');


  // Close takes it away
  await page.getByRole('button', { name: 'Close the artifact' }).click();
  await until('the preview hidden', () => app.evaluate(({ BrowserWindow, webContents }) => {
    const win = BrowserWindow.getAllWindows().find((w) => !w.webContents.getURL().endsWith('/preview.html'));
    const wc = webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'));
    return !win.contentView.children.find((c) => c.webContents === wc)?.getVisible();
  }));
  assert.deepEqual(errors, [], 'the page threw nothing');
}

main().then(() => { console.log('test-artifacts: all passed'); }, (e) => { console.error(e); console.error('main said:', mainLogs.join('').slice(-3000)); process.exitCode = 1; }).finally(async () => {
  await app?.close().catch(() => {});
  if (daemon && daemon.exitCode === null) { const gone = new Promise((done) => daemon.once('exit', done)); daemon.kill(); await gone; }
  if (!process.env.OBY_KEEP) rmSync(scratch, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
});
