// An artifact in the real window (docs/contracts/artifacts-app.md), end to end: a real obid in a
// scratch home, a stand-in agent's task that makes a page (index.html and its stylesheet, under
// .obi/artifacts/), and the built app. Nothing reaches a real agent, account or the network.
//   ARTIFACTSAPP-4: the page's card shows in the message that reports its task's end, in the task's
//                   conversation, and a click opens it beside the conversation.
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
//   ARTIFACTSAPP-16: a card stays in the first message that tells of its artifact (the page's in its
//                   task's report, the site's in the turn that published it), on its latest version.
//   ARTIFACTSAPP-17: a version made while another artifact is open puts a New dot on the page's row
//                   under its conversation; the row's click opens it, lit, the dot gone. At 1280 by 800,
//                   the boards' size (1a, 1b).
//   ARTIFACTSAPP-3: the preview reaches nothing off its origin (another site, obid, the relay by
//                   127.0.0.1), by fetch, image, script or navigation, while its own files load; and
//                   it is sandboxed in its own partition, with no preload, no Node and no bridge.
//   ARTIFACTSAPP-18: the page's card shows its first screen, a 1280x370 PNG main took in the preview's
//                   second, never-shown view (preview.html?thumb), the page drawn 640x185 at scale 2 and
//                   the pane's at zoom 1; taking it sent nothing but to its host page and the pane's own
//                   relay, and refused nothing the pane's view didn't; three fresh cards all have their
//                   pictures within 6 s, and one picture's view at most is left; a React page drawn
//                   1.5 s after its frame loads still fills its card's picture, the picture's view told
//                   by React (artifactsapp_18_a_react_cards_picture_shows_the_page).
//   ARTIFACTSAPP-13: a React folder (App.tsx importing its Label.tsx, useState, lucide's Check and
//                   Tailwind's classes) is a Page: it draws in the preview on the vendored set main's
//                   relay serves at /__csb_vendor/, every request on loopback.
//   ARTIFACTSAPP-26: a dashboard on recharts, its class names joined by its own lib/utils (clsx and
//                   tailwind-merge), draws its teal line from loopback only, and its card's picture
//                   holds the chart (artifactsapp_26_a_dashboard_draws_with_recharts).
//   ARTIFACTSAPP-27: a dashboard on shadcn's Button, Card and Dialog and its cn, none of them its own
//                   files, draws from loopback only, shadcn/'s files and chunks among the requests: its
//                   Button is gray-900, its card's picture holds the teal Card, and a click opens the
//                   dialog (artifactsapp_27_a_dashboard_draws_with_shadcn).
//   ARTIFACTSAPP-14 and 28: one importing a package outside that set (three) shows, in the frame and
//                   the pane's error strip, the line naming it and what the set has; nothing is fetched
//                   for it (artifactsapp_28_an_import_outside_the_set_says_what_is_there_in_the_window).
//   ARTIFACTSAPP-19: a page whose script declares `scope`, the console hook's own name, draws
//                   (artifactsapp_19_a_page_declaring_scope_draws).
//   ARTIFACTSAPP-20: the pane's Reload runs the page on view again: a page counting its loads in
//                   sessionStorage shows 2 after it, every request on loopback
//                   (artifactsapp_20_reload_runs_the_page_again).
//   ARTIFACTSAPP-21: a page written for Tailwind's CDN (its script tag and an inline tailwind.config)
//                   draws styled on the bundled Tailwind main's relay serves; nothing leaves loopback
//                   and nothing, main or the frame's CSP, refuses the CDN (artifactsapp_21_a_page_written_for_tailwinds_cdn_draws_styled).
//   ARTIFACTSAPP-24: a page that throws and asks for a CDN script draws what it can, and the pane's strip
//                   says both; a real click on Ask to fix sends the fix words, signed by main, the error
//                   attached as a file and the artifact with them
//                   (artifactsapp_24_a_pages_error_shows_in_the_pane_and_ask_to_fix_sends_it).
//   ARTIFACTSAPP-25: three fresh cards of a Tailwind React page, drawn at once, each have a teal picture,
//                   the picture's view told drawn by the bridge (artifactsapp_25_three_tailwind_react_cards_pictures_are_drawn).
// Screenshots go to OBY_ARTIFACTS_SHOTS (a folder) when it's set: the window, with the preview laid
// where main lays it.
// Run on the Mac: npm run build, then OBY_TEST_OBID=<obid> npm run test:artifacts
// (obid: core/target/debug/obid by default).
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
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

// A card scrolled into view: the conversation re-renders its message while it updates, so the card
// can be swapped for a moment between finding it and scrolling to it ("Element is not attached to
// the DOM"); the locator finds it again, tried until it's attached, within 30 s
const inView = (card) => until('the card attached and in view', async () => { await card.scrollIntoViewIfNeeded({ timeout: 5_000 }); return true; }, 30_000);

const shot = async (page, name) => {
  if (!shots) return;
  mkdirSync(shots, { recursive: true });
  // every card in the page with its picture first (ARTIFACTSAPP-18)
  await until(`every card's picture before ${name}`, () => page.evaluate(() => [...document.querySelectorAll('button.art-card')]
    .every((c) => { const i = c.querySelector('img.art-thumb-img'); return i?.complete && i.naturalWidth > 0; })), 30_000);
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
    globalThis.__previewSentBy = [];
    globalThis.__previewErrors = [];
    const requests = session.fromPartition('oby-artifact-preview').webRequest;
    requests.onCompleted((d) => { globalThis.__previewRequests.push(d.url); });
    // what actually went out: its headers sent (main's own check cancels a request before that)
    requests.onSendHeaders((d) => { globalThis.__previewSent.push(d.url); globalThis.__previewSentBy.push({ url: d.url, id: d.webContentsId }); });
    // and what was refused or failed, by view (ARTIFACTSAPP-18)
    requests.onErrorOccurred((d) => { globalThis.__previewErrors.push({ url: d.url, id: d.webContentsId, error: d.error }); });
  });

  // ARTIFACTSAPP-4: the card, in the message that reports its task's end, opens the pane
  const row = page.locator('.convo-row', { hasText: job.title || 'ARTIFACTS' }).first();
  await until('the task’s conversation in the side panel', async () => (await row.count()) > 0);
  await row.evaluate((el) => el.click());
  const card = page.locator(`button.art-card[data-artifact="${pageArtifact.id}"]`);
  await until('the page’s card', async () => (await card.count()) > 0);
  const meta = await card.locator('.art-card-meta').innerText();
  assert.match(meta, /^Oby artifact · Page · v1( · made by Claude Code)?$/, `artifactsapp_4: the card says what it is: ${meta}`);
  // the report's words, as obid wrote them: the card is in their message, never in the task's card
  const turns = await s.call('GET', `/v1/conversation?id=${made.conversation}&limit=100`);
  const report = turns.find((t) => t.ended && t.jobs?.[0] === made.id);
  assert.ok(report, `the task's report is in its conversation: ${JSON.stringify(turns.map((t) => [t.who, t.ended ?? false, t.jobs]))}`);
  const inMessage = (artifact, text) => page.evaluate(({ artifact, text }) => {
    const c = document.querySelector(`button.art-card[data-artifact="${artifact}"]`);
    const msg = c?.closest('.msg');
    // its words' first few, as a person reads them (Markdown drawn)
    const words = (t) => (t.match(/[\p{L}\p{N}]+/gu) ?? []).slice(0, 4).join(' ');
    return { cards: document.querySelectorAll(`button.art-card[data-artifact="${artifact}"]`).length, inTask: !!c?.closest('.msg-task-work'), inEnd: c?.parentElement?.classList.contains('msg-end') ?? false,
      said: words(msg?.querySelector('.msg-bubble')?.innerText ?? ''), want: words(text) };
  }, { artifact, text });
  const placed = await inMessage(pageArtifact.id, report.text);
  assert.deepEqual(placed, { cards: 1, inTask: false, inEnd: true, said: placed.want, want: placed.want }, `artifactsapp_4: the card is in the task's report: ${JSON.stringify(placed)}`);
  // ARTIFACTSAPP-18: the card's top is the page's first screen, as main took it
  const picture = card.locator('img.art-thumb-img');
  await until('the card’s picture', async () => (await picture.count()) === 1 && picture.evaluate((i) => i.complete), 30_000);
  const pic = await picture.evaluate((img) => {
    if (!img.naturalWidth) return { width: 0, height: 0, png: img.src.startsWith('data:image/png;base64,'), drawn: 0 };
    const c = document.createElement('canvas');
    c.width = img.naturalWidth; c.height = img.naturalHeight;
    const g = c.getContext('2d');
    g.drawImage(img, 0, 0);
    const { data } = g.getImageData(0, 0, c.width, Math.floor(c.height / 2));
    let drawn = 0, first = -1, last = -1;
    // an opaque pixel that isn't the page's white; and the rows the teal heading spans
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] === 255 && (data[i] < 240 || data[i + 1] < 240 || data[i + 2] < 240)) drawn++;
      if (data[i] < 80 && data[i + 1] > 90 && data[i + 2] > 90 && Math.abs(data[i + 1] - data[i + 2]) < 40) { const row = Math.floor(i / 4 / c.width); if (first < 0) first = row; last = row; }
    }
    return { width: img.naturalWidth, height: img.naturalHeight, png: img.src.startsWith('data:image/png;base64,'), drawn, heading: first < 0 ? 0 : last - first + 1 };
  });
  assert.deepEqual([pic.width, pic.height], [1280, 370], `artifactsapp_18: the picture is 1280x370: ${JSON.stringify(pic)}`);
  assert.ok(pic.png, 'artifactsapp_18: the picture is a PNG data: URL');
  assert.ok(pic.drawn > 0, `artifactsapp_18: the picture's top half holds the page, not only its white: ${JSON.stringify(pic)}`);
  await inView(card);
  await shot(page, '1-card-picture');
  console.log(`ok artifactsapp_18_the_cards_picture_is_the_artifacts_first_screen (${pic.width}x${pic.height}, ${pic.drawn} drawn pixels in its top half)`);
  // drawn at half width and scale 2: the 32 px heading spans twice the ~12 rows a 720-wide picture of
  // a 1280-wide page gave it
  const thumbSize = await app.evaluate(({ webContents }) => webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html?thumb'))?.executeJavaScript('[innerWidth, innerHeight]'));
  assert.deepEqual(thumbSize, [640, 185], `artifactsapp_18: the picture's page is drawn 640x185: ${JSON.stringify(thumbSize)}`);
  assert.ok(pic.heading >= 24, `artifactsapp_18: the heading spans at least 24 rows: ${JSON.stringify(pic)}`);
  await shot(page, '1-card');
  await card.click();
  await until('the pane', async () => (await page.locator(`[data-artifact-view="${pageArtifact.id}"]`).count()) > 0);
  assert.equal(await page.locator('.art-strip-title b').innerText(), 'page', 'artifactsapp_4: the pane says which');
  console.log('ok artifactsapp_4: the card in its message opens the pane');

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
  // the pane stays at zoom 1 whatever the picture's view draws at (ARTIFACTSAPP-18)
  const paneWidth = await inHost('innerWidth');
  assert.equal(paneWidth, visible.bounds.width, `artifactsapp_18: the pane draws at zoom 1: ${paneWidth} CSS px in a ${visible.bounds.width} wide view`);
  console.log(`ok artifactsapp_18_the_picture_is_drawn_at_half_width (640x185 at scale 2, the heading ${pic.heading} rows; the pane ${paneWidth} wide at zoom 1)`);
  await shot(page, '2-preview');
  const requests = await app.evaluate(() => globalThis.__previewRequests);
  assert.ok(requests.length > 0, 'the preview made requests');
  const off = requests.filter((u) => !u.startsWith('file://') && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u) && !u.startsWith('data:') && !u.startsWith('devtools:'));
  assert.deepEqual(off, [], 'artifactsapp_1: every request the preview made went to its host page or the relay');
  console.log(`ok artifactsapp_1: a page artifact draws in the real window, from loopback only (${requests.length} requests)`);

  // ARTIFACTSAPP-18: the picture's view, once it took the page and the pane drew it too, sent nothing
  // but to its host page and the pane's own relay, and was refused nothing the pane wasn't
  const byView = await app.evaluate(({ webContents }) => {
    const all = webContents.getAllWebContents();
    return { pane: all.find((w) => w.getURL().endsWith('/preview.html'))?.id, thumb: all.find((w) => w.getURL().endsWith('/preview.html?thumb'))?.id,
      sent: globalThis.__previewSentBy, errors: globalThis.__previewErrors };
  });
  assert.ok(byView.thumb !== undefined && byView.thumb !== byView.pane, `artifactsapp_18: the picture's view is its own: ${JSON.stringify([byView.pane, byView.thumb])}`);
  const thumbSent = byView.sent.filter((r) => r.id === byView.thumb).map((r) => r.url);
  assert.ok(thumbSent.length > 0, 'artifactsapp_18: the picture’s view loaded the page');
  const thumbAway = thumbSent.filter((u) => !/^(file|data|blob|devtools):/.test(u) && !new RegExp(`^http://[a-z0-9]+-preview\\.localhost:${relayPort}/`).test(u));
  assert.deepEqual(thumbAway, [], `artifactsapp_18: the picture's view sent nothing but to its host page and the pane's relay: ${thumbAway.join(', ')}`);
  const blocked = (id) => new Set(byView.errors.filter((r) => r.id === id && r.error === 'net::ERR_BLOCKED_BY_CLIENT').map((r) => r.url));
  const paneBlocked = blocked(byView.pane);
  const onlyThumb = [...blocked(byView.thumb)].filter((u) => !paneBlocked.has(u));
  assert.deepEqual(onlyThumb, [], `artifactsapp_18: the picture's view was refused only what the pane's was: ${onlyThumb.join(', ')}`);
  console.log(`ok artifactsapp_18_taking_the_picture_reaches_nothing_new (${thumbSent.length} requests sent, ${blocked(byView.thumb).size} refused)`);

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
  const publish = async (h1, artifact, path = 'site') => {
    const room = join(s.home, 'conversations', made.conversation, 'room', path);
    mkdirSync(room, { recursive: true });
    writeFileSync(join(room, 'index.html'), `<!doctype html><h1>${h1}</h1>`);
    const said = await s.call('POST', '/v1/plans', { said: `OBY oby_artifact ${JSON.stringify({ title: path, path, ...(artifact ? { artifact } : {}) })}`, conversation: made.conversation });
    assert.match(said.text ?? '', /Published artifact /, `the conversation published it: ${said.text}`);
    return said;
  };
  await page.getByRole('button', { name: 'Close the artifact' }).click();
  await until('the pane closed', async () => (await page.locator('.art-strip-title').count()) === 0);
  const first = await publish('One');
  const site = /Published artifact ([0-9a-f-]{36})/.exec(first.text)[1];
  await until('the new artifact open by itself', async () => (await page.locator('.art-strip-title b').innerText().catch(() => '')) === 'site', 60_000);
  // its card, in the Oby message whose turn published it (ARTIFACTSAPP-16)
  const sitePlaced = await until('the site’s card in its message', async () => { const p = await inMessage(site, first.text); return p.cards === 1 && p.said && p; }, 60_000);
  assert.deepEqual(sitePlaced, { cards: 1, inTask: false, inEnd: true, said: sitePlaced.want, want: sitePlaced.want }, `artifactsapp_16: the site's card is in the turn that published it: ${JSON.stringify(sitePlaced)}`);
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
    if (as === 'zip') await shot(page, '6a-export-strip');
    await page.getByRole('button', { name: 'Export' }).click();
    if (as === 'zip') { await page.getByRole('menu', { name: 'Download' }).waitFor(); await shot(page, '6b-export-menu'); }
    await page.getByRole('menuitem', { name: item }).click();
    await until(`the ${as} saved`, async () => (await page.locator('.art-saved').innerText().catch(() => '')) === `Saved to ${to}`);
    if (as === 'zip') await shot(page, '6c-saved-to');
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
  await shot(page, '6d-import-bar');
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await until('the import open', async () => (await page.locator('.art-strip-title b').innerText().catch(() => '')) === 'Imported', 60_000);
  assert.equal(await until('the import drawn', async () => { const v = (await inFrame(`document.querySelector('h1')?.textContent ?? null`)).value; return v === 'Mine' && v; }, 60_000), 'Mine');
  const imported = (await s.call('GET', '/v1/artifacts')).find((a) => a.origin?.kind === 'import');
  assert.ok(imported, 'artifactsapp_12: obid keeps it as an import');
  assert.equal(imported.origin.from, 'page');
  await shot(page, '7-imported');
  console.log('ok artifactsapp_12: a page dropped and imported on Import opens beside the conversation');

  // ARTIFACTSAPP-16 and 17, at the boards' size: the page's v2, published from the conversation's room
  // while the import is open, leaves its card where it was, on v2, and puts a New dot on its row
  await until('the window at the boards’ size', () => app.evaluate(({ BrowserWindow }) => {
    const win = BrowserWindow.getAllWindows().find((w) => !w.webContents.getURL().endsWith('/preview.html'));
    win.setContentSize(1280, 800);
    const [w, h] = win.getContentSize();
    return w === 1280 && h === 800;
  }));
  const again = join(s.home, 'conversations', made.conversation, 'room', 'page-v2');
  mkdirSync(again, { recursive: true });
  writeFileSync(join(again, 'index.html'), '<!doctype html><h1>Hello again</h1>');
  const v2 = await s.call('POST', '/v1/plans', { said: `OBY oby_artifact ${JSON.stringify({ title: 'page', path: 'page-v2', artifact: pageArtifact.id })}`, conversation: made.conversation });
  assert.match(v2.text ?? '', /Published artifact /, `the conversation published the page's v2: ${v2.text}`);
  const pageRow = page.locator(`button.convo-kid.art-kid[title="page, made in this conversation"]`);
  const dot = pageRow.locator('span.status[role="img"][aria-label="New"] > span.status-dot.is-new');
  await until('the page’s card on v2', async () => (await card.locator('.art-card-meta').innerText().catch(() => '')).startsWith('Oby artifact · Page · v2'), 60_000);
  const stayed = await inMessage(pageArtifact.id, report.text);
  assert.deepEqual(stayed, { cards: 1, inTask: false, inEnd: true, said: stayed.want, want: stayed.want }, `artifactsapp_16: the page's card stays in the task's report: ${JSON.stringify(stayed)}`);
  await until('the page’s row with its New dot', async () => (await dot.count()) === 1, 60_000);
  console.log('ok artifactsapp_16: the page’s card stays in its task’s report on v2, and the site’s is in the turn that published it');
  // as board 1a: the import closed, the side panel beside the conversation (an open pane at this
  // width floats it), the card in view
  const showPanel = async () => { const b = page.getByRole('button', { name: 'Show sidebar' }); if (await b.count()) await b.click(); };
  await page.getByRole('button', { name: 'Close the artifact' }).click();
  await until('the pane closed', async () => (await page.locator('.art-strip-title').count()) === 0);
  await showPanel();
  await until('the row in view', async () => (await pageRow.isVisible()) && (await pageRow.boundingBox())?.x >= 0);
  await inView(card);
  assert.equal(await dot.count(), 1, 'artifactsapp_17: the dot stays until the page is opened');
  await shot(page, '8-card-in-message');
  await shot(page, '9-row-new');
  await pageRow.click();
  await until('the page open from its row', async () => (await page.locator('.art-strip-title b').innerText().catch(() => '')) === 'page');
  await showPanel();
  await until('the row in view beside the pane', async () => (await pageRow.boundingBox())?.x >= 0);
  assert.equal(await page.locator('.art-strip-title span').innerText(), 'Oby artifact · Page · v2', 'artifactsapp_17: the row opens the latest version');
  assert.match(await pageRow.getAttribute('class'), /\bis-on\b/, 'artifactsapp_17: the row is lit while its artifact is open');
  assert.equal(await dot.count(), 0, 'artifactsapp_17: opened, the dot goes');
  await shot(page, '10-row-open');
  console.log('ok artifactsapp_17: a new version puts a New dot on the row under its conversation, and the row opens it, lit, the dot gone');

  // ARTIFACTSAPP-18: three artifacts no card showed before, published one after the other: once their
  // cards are in the page, all three have their pictures within 6 s
  const fresh = [];
  for (const n of ['fresh-1', 'fresh-2', 'fresh-3']) fresh.push(/Published artifact ([0-9a-f-]{36})/.exec((await publish(n, undefined, n)).text)[1]);
  const freshCards = fresh.map((f) => page.locator(`button.art-card[data-artifact="${f}"]`));
  await until('the three fresh cards', async () => (await Promise.all(freshCards.map((c) => c.count()))).every((n) => n === 1), 60_000);
  const since = Date.now();
  await until('their three pictures', async () => (await Promise.all(freshCards.map((c) => c.evaluate((el) => { const i = el.querySelector('img.art-thumb-img'); return !!i?.complete && i.naturalWidth > 0; })))).every(Boolean), 6_000);
  const took = Date.now() - since;
  await inView(freshCards[2]);
  await shot(page, '11-three-fresh-cards');
  console.log(`ok artifactsapp_18_a_picture_waits_for_the_preview_not_a_fixed_time (three fresh cards' pictures in ${took} ms)`);
  // one picture's view at most is left
  const thumbViews = await app.evaluate(({ webContents }) => webContents.getAllWebContents().filter((w) => w.getURL().endsWith('/preview.html?thumb')).length);
  assert.ok(thumbViews <= 1, `artifactsapp_18: one picture's view at most: ${thumbViews}`);

  // ARTIFACTSAPP-13: a React folder (App.tsx and its Label.tsx, no index.html), published from the
  // conversation's room, draws in the preview on the vendored React, lucide-react and Tailwind that
  // main's relay serves: every request on loopback
  const publishFiles = async (path, files, open = true) => {
    const room = join(s.home, 'conversations', made.conversation, 'room', path);
    mkdirSync(room, { recursive: true });
    for (const [name, text] of Object.entries(files)) { mkdirSync(dirname(join(room, name)), { recursive: true }); writeFileSync(join(room, name), text); }
    const said = await s.call('POST', '/v1/plans', { said: `OBY oby_artifact ${JSON.stringify({ title: path, path })}`, conversation: made.conversation });
    assert.match(said.text ?? '', /Published artifact /, `the conversation published ${path}: ${said.text}`);
    const id = /Published artifact ([0-9a-f-]{36})/.exec(said.text)[1];
    // opened from its card, in the turn that published it
    const itsCard = page.locator(`button.art-card[data-artifact="${id}"]`);
    await until(`${path}’s card`, async () => (await itsCard.count()) === 1, 60_000);
    if (!open) return id;
    await inView(itsCard);
    await itsCard.click();
    await until(`${path} open`, async () => (await page.locator('.art-strip-title b').innerText().catch(() => '')) === path, 60_000);
    return id;
  };
  const react = await publishFiles('react', {
    'App.tsx': `import { useState } from 'react';\nimport { Check } from 'lucide-react';\nimport { Label } from './Label';\n\nawait new Promise((r) => setTimeout(r, 1500));\n\nexport default function App() {\n  const [n] = useState<number>(3);\n  return (\n    <main className="p-4 bg-teal-600 text-white">\n      <h1>React {n}</h1>\n      <Check />\n      <Label text="drawn" />\n    </main>\n  );\n}\n`,
    'Label.tsx': `export function Label({ text }: { text: string }) {\n  return <p className="mt-2">{text}</p>;\n}\n`,
  });
  const reactCard = page.locator(`button.art-card[data-artifact="${react}"]`);
  const reactMeta = await reactCard.locator('.art-card-meta').innerText();
  assert.match(reactMeta, /^Oby artifact · Page · v1( · made by Claude Code)?$/, `artifactsapp_13: the card says a page: ${reactMeta}`);
  const reactDrawn = await until('the React page drawn', async () => {
    const r = await inFrame(`(() => { const h = document.querySelector('h1'); const m = document.querySelector('main'); return h && m && { h1: h.textContent, lucide: !!document.querySelector('svg.lucide'), paddingTop: getComputedStyle(m).paddingTop }; })()`);
    return r.value?.paddingTop === '16px' && r.value;
  }, 60_000).catch(async (e) => { console.error('the React frame shows:', JSON.stringify((await inFrame('document.body.innerHTML.slice(0, 600)')).value)); throw e; });
  assert.deepEqual(reactDrawn, { h1: 'React 3', lucide: true, paddingTop: '16px' }, `artifactsapp_13: React, lucide and Tailwind drew it: ${JSON.stringify(reactDrawn)}`);
  const reactRequests = await app.evaluate(() => globalThis.__previewRequests);
  const reactOff = reactRequests.filter((u) => !u.startsWith('file://') && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u) && !u.startsWith('data:') && !u.startsWith('devtools:'));
  assert.deepEqual(reactOff, [], 'artifactsapp_13: every request the preview made went to its host page or the relay');
  const vendorLoaded = reactRequests.filter((u) => u.includes('/__csb_vendor/'));
  assert.ok(vendorLoaded.length > 0, 'artifactsapp_13: the vendored files came from the relay');
  // ARTIFACTSAPP-18: React draws 1.5 s after the frame loads, and the card's picture, taken once the
  // host page said drawn, still holds the teal page in its top half
  await until('the React card’s picture', async () => reactCard.evaluate((el) => { const i = el.querySelector('img.art-thumb-img'); return !!i?.complete && i.naturalWidth > 0; }), 30_000);
  // the teal pixels in a card's picture's top half
  const tealOf = (c) => c.evaluate((el) => {
    const img = el.querySelector('img.art-thumb-img');
    const c = document.createElement('canvas');
    c.width = img.naturalWidth; c.height = img.naturalHeight;
    const g = c.getContext('2d');
    g.drawImage(img, 0, 0);
    const { data } = g.getImageData(0, 0, c.width, Math.floor(c.height / 2));
    let teal = 0;
    for (let i = 0; i < data.length; i += 4) if (data[i] < 60 && data[i + 1] > 120 && data[i + 2] > 110) teal++;
    return { teal, of: data.length / 4 };
  });
  const reactPicture = await tealOf(reactCard);
  // the picture's view heard "drawn" from React, not its 4 s ceiling
  const thumbStatus = () => app.evaluate(({ webContents }) => webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html?thumb'))
    ?.executeJavaScript('Promise.race([window.obyStatus, new Promise((r) => setTimeout(() => r(null), 100))])'));
  const thumbDrawn = (await thumbStatus())?.drawn === true;
  assert.equal(thumbDrawn, true, "artifactsapp_18: the picture's view heard the React page drawn");
  assert.ok(reactPicture.teal > 0, `artifactsapp_18: the React card's picture holds the teal page in its top half: ${JSON.stringify(reactPicture)}`);
  await inView(reactCard);
  await shot(page, '12-react');
  console.log(`ok artifactsapp_13_a_react_artifact_draws_with_lucide_and_tailwind_from_loopback_only (${reactRequests.length} requests, ${vendorLoaded.length} to /__csb_vendor/)`);
  console.log(`ok artifactsapp_18_a_react_cards_picture_shows_the_page (${reactPicture.teal} of ${reactPicture.of} pixels in its top half teal; the picture's view drawn)`);

  // ARTIFACTSAPP-26: a dashboard on recharts, its classes joined by its own lib/utils on clsx and
  // tailwind-merge (`p-4 p-2` merged to p-2), draws its teal line from loopback only, and its card's
  // picture holds the chart
  const dashboard = await publishFiles('dashboard', {
    'App.tsx': `import { LineChart, Line, XAxis, ResponsiveContainer } from 'recharts';\nimport { cn } from '/lib/utils';\n\nconst data = [{ m: 'Jan', v: 4 }, { m: 'Feb', v: 9 }, { m: 'Mar', v: 6 }, { m: 'Apr', v: 12 }];\n\nexport default function App() {\n  return (\n    <main className={cn('p-4', 'p-2')}>\n      <div style={{ width: '100%', height: 160 }}>\n        <ResponsiveContainer width="100%" height="100%">\n          <LineChart data={data}>\n            <XAxis dataKey="m" />\n            <Line type="monotone" dataKey="v" stroke="#0d9488" strokeWidth={6} dot={false} isAnimationActive={false} />\n          </LineChart>\n        </ResponsiveContainer>\n      </div>\n    </main>\n  );\n}\n`,
    'lib/utils.ts': `import { clsx, type ClassValue } from 'clsx';\nimport { twMerge } from 'tailwind-merge';\n\nexport function cn(...inputs: ClassValue[]) {\n  return twMerge(clsx(inputs));\n}\n`,
  });
  const chart = await until('the dashboard drawn', async () => {
    const r = await inFrame(`(() => { const c = document.querySelector('svg.recharts-surface path.recharts-curve'); const m = document.querySelector('main'); return c && m && { curve: true, stroke: c.getAttribute('stroke'), className: m.className, paddingTop: getComputedStyle(m).paddingTop }; })()`);
    return r.value?.paddingTop === '8px' && r.value;
  }, 60_000).catch(async (e) => { console.error('the dashboard frame shows:', JSON.stringify((await inFrame('document.body.innerHTML.slice(0, 600)')).value)); throw e; });
  assert.deepEqual(chart, { curve: true, stroke: '#0d9488', className: 'p-2', paddingTop: '8px' }, `artifactsapp_26: recharts drew its line and cn merged the classes: ${JSON.stringify(chart)}`);
  const dashboardCard = page.locator(`button.art-card[data-artifact="${dashboard}"]`);
  await until('the dashboard card’s picture', async () => dashboardCard.evaluate((el) => { const i = el.querySelector('img.art-thumb-img'); return !!i?.complete && i.naturalWidth > 0; }), 30_000);
  const dashboardPicture = await tealOf(dashboardCard);
  assert.ok(dashboardPicture.teal > 0, `artifactsapp_26: the dashboard card's picture holds the chart: ${JSON.stringify(dashboardPicture)}`);
  const dashboardRequests = await app.evaluate(() => globalThis.__previewRequests);
  const dashboardOff = dashboardRequests.filter((u) => !u.startsWith('file://') && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u) && !u.startsWith('data:') && !u.startsWith('devtools:'));
  assert.deepEqual(dashboardOff, [], 'artifactsapp_26: every request the preview made went to its host page or the relay');
  assert.ok(dashboardRequests.some((u) => u.endsWith('/__csb_vendor/recharts/recharts.mjs')), 'artifactsapp_26: recharts came from the relay');
  await inView(dashboardCard);
  await shot(page, '12b-recharts-dashboard');
  console.log(`ok artifactsapp_26_a_dashboard_draws_with_recharts (a ${chart.stroke} curve, main's classes ${JSON.stringify(chart.className)}, ${dashboardPicture.teal} of ${dashboardPicture.of} pixels in its picture's top half teal, ${dashboardRequests.length} requests, all on loopback)`);

  // ARTIFACTSAPP-27: a dashboard on shadcn's Button, Card and Dialog and its cn, with no lib/ or
  // components/ of its own, draws from loopback only: the Card teal through cn (bg-white merged away),
  // the Button gray-900, and a click on it opens the dialog
  const shadcnBoard = await publishFiles('shadcn', {
    'App.tsx': `import { Button } from '/components/ui/button';\nimport { Card, CardHeader, CardTitle, CardContent } from '/components/ui/card';\nimport { Dialog, DialogTrigger, DialogContent, DialogTitle } from '/components/ui/dialog';\nimport { cn } from '/lib/utils';\n\nexport default function App() {\n  return (\n    <main className="p-4">\n      <Card className={cn('bg-white', 'bg-teal-600')}>\n        <CardHeader>\n          <CardTitle>Revenue</CardTitle>\n        </CardHeader>\n        <CardContent>\n          <Dialog>\n            <DialogTrigger asChild>\n              <Button id="details">Details</Button>\n            </DialogTrigger>\n            <DialogContent>\n              <DialogTitle>Revenue by month</DialogTitle>\n            </DialogContent>\n          </Dialog>\n        </CardContent>\n      </Card>\n      <div id="gray" className="mt-4 h-4 w-4 bg-gray-900" />\n    </main>\n  );\n}\n`,
  });
  const board = await until('the shadcn dashboard drawn', async () => {
    const r = await inFrame(`(() => { const b = document.getElementById('details'); const g = document.getElementById('gray'); return b && g && { button: getComputedStyle(b).backgroundColor, gray: getComputedStyle(g).backgroundColor, card: getComputedStyle(b.closest('main > div')).backgroundColor }; })()`);
    // Tailwind compiles after React draws: the Button's background is gray-900's once it has, never transparent
    return r.value?.button && r.value.button === r.value.gray && !/^(transparent|rgba\(0, 0, 0, 0\))$/.test(r.value.button) && r.value;
  }, 60_000).catch(async (e) => { console.error('the shadcn frame shows:', JSON.stringify((await inFrame('document.body.innerHTML.slice(0, 600)')).value)); throw e; });
  const shadcnCard = page.locator(`button.art-card[data-artifact="${shadcnBoard}"]`);
  await until('the shadcn card’s picture', async () => shadcnCard.evaluate((el) => { const i = el.querySelector('img.art-thumb-img'); return !!i?.complete && i.naturalWidth > 0; }), 30_000);
  const shadcnPicture = await tealOf(shadcnCard);
  assert.ok(shadcnPicture.teal > 0, `artifactsapp_27: the shadcn card's picture holds the page: ${JSON.stringify(shadcnPicture)}`);
  await inView(shadcnCard);
  await shot(page, '12c-shadcn-dashboard');
  await inFrame(`document.getElementById('details').click(); true`);
  // open, and settled: its entrance run, drawn whole
  const opened = await until('the dialog open', async () => (await inFrame(`(() => { const d = [...document.querySelectorAll('[role="dialog"]')].find((e) => e.textContent.includes('Revenue by month')); return d && !d.getAnimations({ subtree: true }).some((a) => a.playState === 'running') && getComputedStyle(d).opacity === '1' && { title: d.textContent, background: getComputedStyle(d).backgroundColor, left: Math.round(d.getBoundingClientRect().left), width: Math.round(d.getBoundingClientRect().width), inner: innerWidth }; })()`)).value, 30_000)
    .catch(async (e) => { console.error('the dialog shows:', JSON.stringify((await inFrame(`(() => { const d = document.querySelector('[role="dialog"]'); return d && { opacity: getComputedStyle(d).opacity, transform: getComputedStyle(d).translate, animations: d.getAnimations({ subtree: true }).map((a) => a.playState) }; })()`)).value)); throw e; });
  assert.ok(opened.title.startsWith('Revenue by month'), `artifactsapp_27: a click on the trigger opened the dialog: ${JSON.stringify(opened)}`);
  await shot(page, '12c-shadcn-dialog');
  const shadcnRequests = await app.evaluate(() => [...globalThis.__previewRequests, ...globalThis.__previewSent]);
  const shadcnOff = shadcnRequests.filter((u) => !/^(file|data|blob|devtools):/.test(u) && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u));
  assert.deepEqual(shadcnOff, [], `artifactsapp_27: nothing left loopback: ${shadcnOff.join(', ')}`);
  for (const f of ['ui/button.mjs', 'ui/dialog.mjs']) assert.ok(shadcnRequests.some((u) => u.endsWith(`/__csb_vendor/shadcn/${f}`)), `artifactsapp_27: shadcn/${f} came from the relay`);
  const shadcnChunks = new Set(shadcnRequests.filter((u) => u.includes('/__csb_vendor/shadcn/chunks/')).map((u) => new URL(u).pathname));
  assert.ok(shadcnChunks.size > 0, 'artifactsapp_27: shadcn/chunks/ came from the relay');
  console.log(`ok artifactsapp_27_a_dashboard_draws_with_shadcn (the Button ${board.button} as a gray-900 element, the Card ${board.card}, the dialog opened on a click and drawn whole (${opened.background}, ${opened.width} px wide at ${opened.left} in ${opened.inner}), ${shadcnChunks.size} shared chunks, ${shadcnPicture.teal} of ${shadcnPicture.of} pixels in its picture's top half teal, all on loopback)`);

  // ARTIFACTSAPP-14 and 28: an import outside the vendored set is named, with what the set has, in
  // the frame and the pane's error strip; nothing is fetched for it
  await publishFiles('charts', { 'App.tsx': `import * as THREE from 'three';\n\nexport default function App() {\n  return <p>{String(THREE.REVISION)}</p>;\n}\n` });
  const refused = await until('the import’s error shown', async () => (await inFrame(`document.querySelector('pre')?.textContent ?? null`)).value, 60_000);
  const NO_THREE = 'This preview has no `three`. It has: react, react-dom, lucide-react, recharts, clsx, tailwind-merge, class-variance-authority and shadcn/ui at /components/ui/<name>.';
  assert.equal(refused, NO_THREE, `artifactsapp_28: the frame names the import and the set: ${refused}`);
  const stripSays = await until('the import’s error in the pane', async () => { const t = await page.locator('.art-page-error pre').innerText().catch(() => ''); return t.includes(NO_THREE) && t; }, 30_000);
  const chartsSent = await app.evaluate(() => [...globalThis.__previewRequests, ...globalThis.__previewSent]);
  assert.deepEqual(chartsSent.filter((u) => u.includes('three')), [], 'artifactsapp_14: nothing was fetched for three');
  const chartsAway = chartsSent.filter((u) => !/^(file|data|blob|devtools):/.test(u) && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u));
  assert.deepEqual(chartsAway, [], `artifactsapp_14: nothing left loopback: ${chartsAway.join(', ')}`);
  await shot(page, '13-react-import-refused');
  console.log(`ok artifactsapp_14_an_import_outside_the_vendored_set_shows_its_error_and_fetches_nothing (${JSON.stringify(refused)})`);
  console.log(`ok artifactsapp_28_an_import_outside_the_set_says_what_is_there_in_the_window (the pane's strip: ${JSON.stringify(stripSays)})`);

  // ARTIFACTSAPP-19: a page that declares `scope` draws beside Sandpack's console hook; the frame's
  // console, said when it doesn't
  await app.evaluate(({ webContents }) => {
    globalThis.__previewConsole = [];
    webContents.getAllWebContents().find((w) => w.getURL().endsWith('/preview.html'))
      .on('console-message', (e) => { globalThis.__previewConsole.push(e.message); });
  });
  await publishFiles('scope', { 'index.html': `<!doctype html><html><body><script>const scope = 'artifact'; document.body.dataset.scope = scope;</script></body></html>` });
  const scope = await until('the page declaring scope drawn', async () => (await inFrame('document.body.dataset.scope')).value, 30_000).catch(async (e) => {
    console.error('artifactsapp_19: document.body.dataset.scope is', (await inFrame('String(document.body.dataset.scope)')).value, '; the frame logged:', JSON.stringify(await app.evaluate(() => globalThis.__previewConsole)));
    throw e;
  });
  assert.equal(scope, 'artifact', `artifactsapp_19: the page's own scope: ${scope}`);
  await shot(page, '14-scope');
  console.log(`ok artifactsapp_19_a_page_declaring_scope_draws (document.body.dataset.scope is ${JSON.stringify(scope)})`);

  // ARTIFACTSAPP-20: the pane's Reload runs the page on view again, its frame's session kept
  await publishFiles('reload', { 'index.html': `<!doctype html><html><body><p id="loads"></p><script>const loads = Number(sessionStorage.getItem('loads') || 0) + 1; sessionStorage.setItem('loads', String(loads)); document.getElementById('loads').textContent = String(loads);</script></body></html>` });
  const loads = () => inFrame(`document.getElementById('loads')?.textContent ?? null`).then((r) => r.value);
  assert.equal(await until('the page’s first load', loads, 30_000), '1', 'artifactsapp_20: the page loaded once');
  await page.locator('[data-artifact-strip] button[aria-label="Reload"]').click();
  assert.equal(await until('the page loaded again', async () => { const n = await loads(); return n === '2' && n; }, 30_000), '2', 'artifactsapp_20: Reload ran the page again');
  const reloadRequests = await app.evaluate(() => globalThis.__previewRequests);
  const reloadOff = reloadRequests.filter((u) => !u.startsWith('file://') && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u) && !u.startsWith('data:') && !u.startsWith('devtools:'));
  assert.deepEqual(reloadOff, [], 'artifactsapp_20: every request the preview made went to its host page or the relay');
  await shot(page, '15-reloaded');
  console.log(`ok artifactsapp_20_reload_runs_the_page_again (the frame shows 2 after Reload, ${reloadRequests.length} requests, all on loopback)`);

  // ARTIFACTSAPP-21: a page written for Tailwind's CDN draws styled, its tag sent to the bundled
  // Tailwind: nothing leaves loopback, and nothing is refused for the CDN
  const sinceTailwind = await app.evaluate(() => globalThis.__previewRequests.length);
  await publishFiles('tailwind', { 'index.html': `<!doctype html><html><head><script src="https://cdn.tailwindcss.com"></script><script>tailwind.config = { theme: {} }</script></head><body><main class="p-4 bg-teal-600"><h1>Tailwind's CDN</h1></main></body></html>` });
  const styled = () => inFrame(`(() => { const m = document.querySelector('main'); return m && { paddingTop: getComputedStyle(m).paddingTop, backgroundColor: getComputedStyle(m).backgroundColor }; })()`).then((r) => r.value);
  await until('the Tailwind page styled', async () => (await styled())?.paddingTop === '16px', 30_000).catch(() => {});
  const tailwind = await styled();
  await shot(page, '16-tailwind-cdn');
  // refused by the session (main's lock), or by the frame's CSP before any request (its console)
  const cdnRefused = [...(await app.evaluate(() => globalThis.__previewErrors)).map((e) => e.url), ...mainLogs.join('').split('\n').filter((l) => l.includes('artifact preview: refused')),
    ...(await app.evaluate(() => globalThis.__previewConsole)).filter((l) => /refused|violates|blocked/i.test(l))].filter((u) => u.includes('cdn.tailwindcss.com'));
  assert.equal(tailwind?.paddingTop, '16px', `artifactsapp_21: Tailwind's p-4 applied: ${JSON.stringify(tailwind)}; refused for the CDN: ${JSON.stringify(cdnRefused)}`);
  assert.notEqual(tailwind.backgroundColor, 'rgba(0, 0, 0, 0)', `artifactsapp_21: bg-teal-600 applied: ${JSON.stringify(tailwind)}`);
  const tailwindRequests = await app.evaluate(() => globalThis.__previewRequests);
  assert.ok(tailwindRequests.slice(sinceTailwind).some((u) => u.endsWith('/__csb_vendor/tailwindcss-browser/index.global.js')), 'artifactsapp_21: the page had the bundled Tailwind from the relay');
  const tailwindOff = tailwindRequests.filter((u) => !u.startsWith('file://') && !new RegExp(`^http://([a-z0-9]+-)?preview\\.localhost:${relayPort}/`).test(u) && !u.startsWith('data:') && !u.startsWith('devtools:'));
  assert.deepEqual(tailwindOff, [], 'artifactsapp_21: every request the preview made went to its host page or the relay');
  assert.deepEqual(cdnRefused, [], 'artifactsapp_21: nothing was refused for the CDN');
  console.log(`ok artifactsapp_21_a_page_written_for_tailwinds_cdn_draws_styled (paddingTop ${tailwind.paddingTop}, background ${tailwind.backgroundColor}, ${tailwindRequests.length} requests, all on loopback)`);

  // ARTIFACTSAPP-24: a page that throws, then asks for a CDN script: the frame draws what it can, the
  // pane's strip says both, and a real click on Ask to fix sends the fix words, signed by main, the
  // error attached as a file and the artifact with them
  const broken = await publishFiles('broken', { 'index.html': `<!doctype html><html><body><h1>Half</h1><script>throw new Error('boom')</script><script src="https://cdn.example.com/x.js"></script></body></html>` });
  const strip = page.locator('.art-page-error');
  const said = await until('the page’s error in the pane', async () => { const t = await strip.locator('pre').innerText().catch(() => ''); return t.includes('boom') && t.includes('cdn.example.com') && t; }, 30_000).catch(async (e) => {
    console.error('artifactsapp_24: the pane shows', JSON.stringify(await page.locator('.art-br').innerText().catch(() => '')), '; the picture view’s status', JSON.stringify(await inHost('Promise.race([window.obyStatus, new Promise((r) => setTimeout(() => r(null), 100))])')));
    throw e;
  });
  assert.equal(await strip.locator('p').innerText(), 'This page stopped with an error.', 'artifactsapp_24: the strip’s lead');
  assert.equal((await inFrame(`document.querySelector('h1')?.textContent ?? null`)).value, 'Half', 'artifactsapp_24: the frame shows what drew');
  // its card's picture is its own, never the teal page shown before it in the picture's view
  const brokenCard = page.locator(`button.art-card[data-artifact="${broken}"]`);
  await until('the broken card’s picture', async () => brokenCard.evaluate((el) => { const i = el.querySelector('img.art-thumb-img'); return !!i?.complete && i.naturalWidth > 0; }), 30_000);
  assert.equal((await tealOf(brokenCard)).teal, 0, 'artifactsapp_18: the broken page’s picture is its own, not the Tailwind page’s before it');
  await shot(page, '17-page-error');
  // what main sends obid for the click, its X-Oby-Words included: main's own fetch, watched from main
  await app.evaluate(() => {
    globalThis.__plansSent = [];
    const f = globalThis.fetch;
    globalThis.fetch = (url, init) => {
      if (String(url).endsWith('/v1/plans')) globalThis.__plansSent.push({ words: init?.headers?.['X-Oby-Words'] ?? null, body: JSON.parse(init?.body ?? 'null') });
      return f(url, init);
    };
  });
  const FIX = 'The preview shows an error, in the attached file. Fix the artifact so it draws.';
  await page.getByRole('button', { name: 'Ask to fix', exact: true }).click();
  const fixTurn = await until('the owner’s fix words in the conversation', async () => (await s.call('GET', `/v1/conversation?id=${made.conversation}&limit=100`)).find((t) => t.who === 'user' && t.text === FIX), 60_000);
  assert.deepEqual(fixTurn.attachments?.map((a) => a.split('/').pop()), ['preview-error.txt'], `artifactsapp_24: the error went as a file: ${JSON.stringify(fixTurn.attachments)}`);
  const fixSent = (await app.evaluate(() => globalThis.__plansSent)).filter((r) => r.body?.said === FIX);
  assert.equal(fixSent.length, 1, `artifactsapp_24: one send: ${JSON.stringify(fixSent)}`);
  assert.deepEqual(fixSent[0].body.artifact, { id: broken, version: 1 }, 'artifactsapp_24: the artifact went with the words');
  assert.ok(!fixSent[0].body.said.includes('boom'), 'artifactsapp_24: the error is not in the words');
  assert.match(fixSent[0].words ?? '', /^\S+ \S+ \S+$/, `artifactsapp_24: main signed it (X-Oby-Words): ${fixSent[0].words}`);
  await shot(page, '18-asked-to-fix');
  console.log(`ok artifactsapp_24_a_pages_error_shows_in_the_pane_and_ask_to_fix_sends_it (${JSON.stringify(said)}; sent with X-Oby-Words ${fixSent[0].words.split(' ')[0]} …, attachments ${JSON.stringify(fixTurn.attachments)})`);

  // ARTIFACTSAPP-25: three fresh cards of a Tailwind React page, drawn at once, one after another: each
  // picture's top half is teal, the picture's view told drawn by the bridge, not its ceiling
  const teal = { 'App.tsx': `export default function App() {\n  return <main className="p-4 bg-teal-600 text-white"><h1>Teal</h1></main>;\n}\n` };
  const fresh25 = [];
  let tealCard;
  for (const n of ['teal-1', 'teal-2', 'teal-3']) {
    const tealId = await publishFiles(n, teal, false);
    tealCard = page.locator(`button.art-card[data-artifact="${tealId}"]`);
    await until(`${n}’s picture`, async () => tealCard.evaluate((el) => { const i = el.querySelector('img.art-thumb-img'); return !!i?.complete && i.naturalWidth > 0; }), 30_000);
    fresh25.push({ n, ...(await tealOf(tealCard)), status: await thumbStatus() });
  }
  await inView(tealCard);
  await shot(page, '19-teal-cards');
  for (const f of fresh25) {
    assert.ok(f.teal > 0, `artifactsapp_25: ${f.n}'s picture is drawn and styled: ${JSON.stringify(fresh25)}`);
    assert.deepEqual(f.status, { drawn: true }, `artifactsapp_25: ${f.n}'s picture view heard drawn: ${JSON.stringify(fresh25)}`);
  }
  console.log(`ok artifactsapp_25_three_tailwind_react_cards_pictures_are_drawn (${fresh25.map((f) => `${f.n}: ${f.teal} of ${f.of} teal`).join(', ')})`);

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
