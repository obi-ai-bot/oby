// A Claude Design design in the real window (docs/contracts/artifacts-app.md, ARTIFACTSAPP-29 and 30;
// obid's ARTIFACTS-13 to 15), end to end: a real obid in a scratch home, the built app, and the Oby
// Artifacts canvas (tools/obid-spec/lab-ui/artifacts/design/project/, a Claude Design project) zipped
// as Claude Design exports it. Nothing reaches a real agent, account or the network.
//   ARTIFACTSAPP-29: the zip dropped on a conversation's thread and imported on the owner's Import opens
//                   in the pane, drawn by obid's canvas host: every board of each page in a sandboxed
//                   frame of its own, each drawn at its size with no mistake, on Claude Design's launch
//                   (Prototype, focused); Reload draws it again; Code hides it and Preview brings it back.
//   ARTIFACTSAPP-30: it is drawn on its version's own origin, `<id>-v1.localhost` on main's relay: every
//                   request the view made went there, each board's origin is opaque (sandboxed), and a
//                   board can fetch nothing, not even its own version's files.
// Screenshots go to OBY_ARTIFACTS_SHOTS (a folder) when it's set: the window with the design's view laid
// where main lays it, the view alone, and each board drawn alone at its own size.
// Run on the Mac: npm run build, then OBY_TEST_OBID=<obid> npm run test:artifacts-design
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { appRoot, launch, until, skipFirstRun } from './drive-lib.mjs';
import { startObid, stopObid } from './artifacts-obid.mjs';

const repoRoot = resolve(appRoot, '../..');
const binary = process.env.OBY_TEST_OBID || join(repoRoot, 'core/target/debug/obid');
const shots = process.env.OBY_ARTIFACTS_SHOTS;
const design = join(repoRoot, 'tools/obid-spec/lab-ui/artifacts/design');
const scratch = mkdtempSync(join(tmpdir(), 'oby-artifacts-design-'));
let daemon, app;
const mainLogs = [];

// the design's view: its canvas host on <id>-v<n>.localhost, asked through main
const DESIGN_URL = (id, v) => new RegExp(`^http://${id}-v${v}\\.localhost:(\\d+)/__canvas\\.html`);
const inDesign = (id, v, js) => app.evaluate(async ({ webContents }, { source, js }) => {
  const wc = webContents.getAllWebContents().find((w) => new RegExp(source).test(w.getURL()));
  return wc ? Promise.race([wc.executeJavaScript(js), new Promise((done) => setTimeout(() => done(null), 5_000))]) : null;
}, { source: DESIGN_URL(id, v).source, js });

const shot = async (name, id, v) => {
  if (!shots) return;
  mkdirSync(shots, { recursive: true });
  const png = await app.evaluate(async ({ BrowserWindow, webContents }, source) => {
    const win = BrowserWindow.getAllWindows()[0];
    const wc = webContents.getAllWebContents().find((w) => new RegExp(source).test(w.getURL()));
    const view = win?.contentView.children.find((c) => c.webContents === wc);
    const page = (await win.webContents.capturePage()).toPNG().toString('base64');
    if (!view || !view.getVisible()) return { page };
    return { page, view: (await wc.capturePage()).toPNG().toString('base64'), at: view.getBounds() };
  }, DESIGN_URL(id, v).source);
  writeFileSync(join(shots, `${name}.png`), Buffer.from(png.page, 'base64'));
  if (png.view) {
    writeFileSync(join(shots, `${name}-view.png`), Buffer.from(png.view, 'base64'));
    writeFileSync(join(shots, `${name}-view-at.json`), JSON.stringify(png.at));
  }
};

async function main() {
  // the project, zipped as Claude Design exports it: project/ and every file in it
  const zip = join(scratch, 'Oby Artifacts.zip');
  execFileSync('/usr/bin/zip', ['-q', '-r', '-X', zip, 'project'], { cwd: design });
  const canvas = JSON.parse(readFileSync(join(design, 'project/canvas.json'), 'utf8'));

  const s = await startObid(scratch, binary);
  daemon = s.daemon;
  // a conversation to drop it on: a task's (the stand-in's ARTIFACTS)
  const made = await s.call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'build', task: 'ARTIFACTS' } });
  const job = await until('the task to end', async () => { const j = await s.call('GET', `/v1/jobs/${made.id}`); return ['done', 'failed'].includes(j.status) && j; }, 120_000);

  app = await launch({ dir: s.root, userData: join(s.root, 'electron-data'), env: { ...process.env, OBID_HOME: s.home, OBID_PORT: String(s.port) } });
  const page = await app.firstWindow();
  app.process().stdout?.on('data', (b) => mainLogs.push(b.toString()));
  app.process().stderr?.on('data', (b) => mainLogs.push(b.toString()));
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.waitForLoadState('domcontentloaded');
  await skipFirstRun(page);
  await until('Home', async () => (await page.locator('.app').count()) > 0);
  // the boards' size, 1280 by 800
  await until('the window at the boards’ size', () => app.evaluate(({ BrowserWindow }) => {
    const win = BrowserWindow.getAllWindows()[0];
    win.setResizable(true);
    win.setContentSize(1280, 800);
    return win.getContentSize()[0] >= 1280;
  }));
  // every request the preview's session sends, and what it refused
  await app.evaluate(({ session }) => {
    globalThis.__designSent = [];
    globalThis.__designFailed = [];
    const requests = session.fromPartition('oby-artifact-preview').webRequest;
    requests.onSendHeaders((d) => { globalThis.__designSent.push(d.url); });
    requests.onErrorOccurred((d) => { globalThis.__designFailed.push({ url: d.url, error: d.error }); });
  });

  const row = page.locator('.convo-row', { hasText: job.title || 'ARTIFACTS' }).first();
  await until('the task’s conversation in the side panel', async () => (await row.count()) > 0);
  await row.evaluate((el) => el.click());
  await until('the thread', async () => (await page.locator('.chat-scroll').count()) > 0);

  // ARTIFACTSAPP-29: the zip dropped on the thread, imported on the owner's click
  await page.evaluate(({ name, base64 }) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const data = new DataTransfer();
    data.items.add(new File([bytes], name, { type: 'application/zip' }));
    const thread = document.querySelector('.chat-scroll');
    for (const type of ['dragover', 'drop']) thread.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: data }));
  }, { name: 'Oby Artifacts.zip', base64: readFileSync(zip).toString('base64') });
  await until('the import offered', async () => (await page.locator('[data-artifact-import]').innerText().catch(() => '')).includes('Oby Artifacts.zip'));
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await until('the design open', async () => (await page.locator('.art-strip-title b').innerText().catch(() => '')) === canvas.title, 60_000);
  const imported = (await s.call('GET', '/v1/artifacts')).find((a) => a.origin?.kind === 'import');
  assert.ok(imported, 'artifactsapp_29: obid keeps it as an import');
  assert.equal(imported.kind, 'design', `artifactsapp_29: obid keeps it as a design: ${imported.kind}`);
  assert.deepEqual(imported.origin, { kind: 'import', from: 'claude-design-zip' });
  assert.equal(await page.locator('.art-strip-title span').innerText(), 'Oby artifact · Canvas · v1', 'artifactsapp_29: the strip names it');
  const id = imported.id;

  // drawn: the canvas host on the version's own origin, every board of its first page in a frame
  const first = canvas.pages[0].id;
  const want = Object.entries(canvas.boards).filter(([, b]) => (b.page || first) === first).map(([f]) => f).sort();
  const STATE = `JSON.parse(JSON.stringify({ ready: window.__OBY_CANVAS.ready, page: window.__OBY_CANVAS.page, playing: window.__OBY_CANVAS.playing, errors: window.__OBY_CANVAS.errors, frames: window.__OBY_CANVAS.frames, sandboxes: [...document.querySelectorAll('#stage iframe')].map((f) => f.getAttribute('sandbox')) }))`;
  const drawn = await until('every board drawn', async () => { const st = await inDesign(id, 1, STATE); return st?.ready && st; }, 60_000).catch(async (e) => {
    console.error('the canvas says', JSON.stringify(await inDesign(id, 1, STATE)), '; the pane says', JSON.stringify(await page.locator('.art-br').innerText().catch(() => '')));
    throw e;
  });
  assert.deepEqual(Object.keys(drawn.frames).sort(), want, 'artifactsapp_29: a frame for every board of the page');
  assert.deepEqual(drawn.errors, [], `artifactsapp_29: no board threw: ${JSON.stringify(drawn.errors)}`);
  assert.ok(drawn.sandboxes.every((x) => x === 'allow-scripts'), `artifactsapp_29: each board sandboxed: ${JSON.stringify(drawn.sandboxes)}`);
  for (const f of want) {
    const { w, h } = canvas.boards[f];
    const size = drawn.frames[f].size;
    assert.ok(size && size.width <= w + 1 && size.height <= h + 1 && size.width > 0 && size.height > 0, `artifactsapp_29: ${f} drawn within its ${w}x${h}: ${JSON.stringify(size)}`);
  }
  assert.equal(drawn.page, first);
  // Claude Design's launch: its board focused, played
  assert.equal(drawn.playing, canvas.launch.file, `artifactsapp_29: it opens as Claude Design launches it: ${drawn.playing}`);
  const shown = await until('the design’s view shown over the pane', () => app.evaluate(({ BrowserWindow, webContents }, source) => {
    const win = BrowserWindow.getAllWindows()[0];
    const wc = webContents.getAllWebContents().find((w) => new RegExp(source).test(w.getURL()));
    const view = win.contentView.children.find((c) => c.webContents === wc);
    return view?.getVisible() && view.getBounds();
  }, DESIGN_URL(id, 1).source));
  const box = await page.locator(`[data-artifact-view="${id}"]`).boundingBox();
  assert.ok(Math.abs(shown.x - box.x) <= 1 && Math.abs(shown.width - box.width) <= 1, `artifactsapp_29: the view lies over the pane's box: ${JSON.stringify({ shown, box })}`);
  await shot('d1-launch', id, 1);
  console.log(`ok artifactsapp_29_a_claude_design_project_opens_drawn_in_the_pane (${want.length} boards drawn, ${canvas.launch.file} focused)`);

  // the canvas itself: back from Play, every board laid out; then the second page
  await inDesign(id, 1, `document.getElementById('close').click(); true`);
  await shot('d2-canvas', id, 1);
  const second = canvas.pages[1];
  await inDesign(id, 1, `document.querySelector('[data-page="${second.id}"]').click(); true`);
  const research = Object.entries(canvas.boards).filter(([, b]) => b.page === second.id).map(([f]) => f).sort();
  const page2 = await until(`the ${second.name} page drawn`, async () => { const st = await inDesign(id, 1, STATE); return st?.ready && st.page === second.id && st; }, 30_000);
  assert.deepEqual(Object.keys(page2.frames).sort(), research, 'artifactsapp_29: its pages switch');
  assert.deepEqual(page2.errors, [], `artifactsapp_29: no board of ${second.name} threw: ${JSON.stringify(page2.errors)}`);
  await shot('d3-research', id, 1);
  console.log(`ok artifactsapp_29_its_pages_switch (${research.join(', ')})`);

  // ARTIFACTSAPP-30: every request went to the version's own origin on main's relay; a board is held in
  const sent = await app.evaluate(() => globalThis.__designSent);
  const origin = new RegExp(`^http://${id}-v1\\.localhost:\\d+/`);
  const designSent = sent.filter((u) => origin.test(u));
  const away = sent.filter((u) => !origin.test(u) && !/^(file|data|blob|devtools):/.test(u) && !/^http:\/\/([a-z0-9]+-)?preview\.localhost:\d+\//.test(u));
  assert.deepEqual(away, [], `artifactsapp_30: nothing left loopback: ${JSON.stringify(away.slice(0, 10))}`);
  for (const p of ['/__canvas.html', '/support.js', '/__dc/boards.js', '/Main.dc.html']) assert.ok(designSent.some((u) => new URL(u).pathname === p), `artifactsapp_30: ${p} came from the version's origin`);
  const held = await app.evaluate(async ({ webContents }, source) => {
    const wc = webContents.getAllWebContents().find((w) => new RegExp(source).test(w.getURL()));
    const board = wc.mainFrame.framesInSubtree.find((f) => f !== wc.mainFrame && f.url.endsWith('/Research.dc.html'));
    if (!board) return null;
    return {
      origin: await board.executeJavaScript('self.origin'),
      own: await board.executeJavaScript(`fetch('/canvas.json').then(() => 'loaded', () => 'blocked')`),
      away: await board.executeJavaScript(`fetch('https://example.com/').then(() => 'loaded', () => 'blocked')`),
      top: await board.executeJavaScript(`(() => { try { return String(top.location.href); } catch { return 'blocked'; } })()`),
    };
  }, DESIGN_URL(id, 1).source);
  assert.deepEqual(held, { origin: 'null', own: 'blocked', away: 'blocked', top: 'blocked' }, `artifactsapp_30: a board is held in: ${JSON.stringify(held)}`);
  console.log(`ok artifactsapp_30_a_design_is_drawn_on_its_versions_own_origin_from_loopback_only (${designSent.length} requests to it, none away; a board's origin ${held.origin})`);

  // Reload draws it again; Code hides it and Preview brings it back
  const loads = await inDesign(id, 1, 'performance.timeOrigin');
  await page.getByRole('button', { name: 'Reload' }).click();
  await until('the design drawn again', async () => { const t = await inDesign(id, 1, 'window.__OBY_CANVAS?.ready && performance.timeOrigin'); return t && t !== loads; }, 30_000);
  const viewShown = () => app.evaluate(({ BrowserWindow, webContents }, source) => {
    const wc = webContents.getAllWebContents().find((w) => new RegExp(source).test(w.getURL()));
    return !!BrowserWindow.getAllWindows()[0].contentView.children.find((c) => c.webContents === wc)?.getVisible();
  }, DESIGN_URL(id, 1).source);
  await page.getByRole('tab', { name: 'Code' }).click();
  await until('the design’s view gone while Code shows', async () => !(await viewShown()));
  await until('the canvas.json in Code', async () => (await page.locator('.cm-content').first().innerText().catch(() => '')).length > 0);
  await page.getByRole('tab', { name: 'Preview' }).click();
  await until('the design’s view back', viewShown);
  console.log('ok artifactsapp_29_reload_and_code_keep_the_design');

  // each board alone, at its size, as Claude Design draws a board: for comparing with its own render
  if (shots) {
    for (const f of Object.keys(canvas.boards)) {
      const { w, h } = canvas.boards[f];
      const png = await app.evaluate(async ({ webContents, WebContentsView }, { source, f, w, h }) => {
        const wc = webContents.getAllWebContents().find((x) => new RegExp(source).test(x.getURL()));
        const base = new URL(wc.getURL()).origin;
        // a board's own file, drawn at its own size in a view of its own of the same session, never shown
        const v = new WebContentsView({ webPreferences: { partition: 'oby-artifact-preview', sandbox: true, contextIsolation: true, backgroundThrottling: false } });
        v.setBounds({ x: 0, y: 0, width: w, height: h });
        v.webContents.enableDeviceEmulation({ screenPosition: 'desktop', screenSize: { width: w, height: h }, viewSize: { width: w, height: h }, viewPosition: { x: 0, y: 0 }, deviceScaleFactor: 1, scale: 1 });
        await v.webContents.loadURL(`${base}/${f}`);
        await new Promise((done) => setTimeout(done, 2_500));
        const image = await v.webContents.capturePage({ x: 0, y: 0, width: w, height: h });
        v.webContents.close();
        return image.toPNG().toString('base64');
      }, { source: DESIGN_URL(id, 1).source, f, w, h }).catch((e) => { console.error(`${f}: ${e.message}`); return null; });
      if (png) writeFileSync(join(shots, `board-${f.replace('.dc.html', '')}.png`), Buffer.from(png, 'base64'));
    }
  }

  // Close takes it away
  await page.getByRole('button', { name: 'Close the artifact' }).click();
  await until('the design’s view hidden', async () => !(await viewShown()));
  assert.deepEqual(errors, [], 'the page threw nothing');
}

main().then(() => { console.log('test-artifacts-design: all passed'); }, (e) => { console.error(e); console.error('main said:', mainLogs.join('').slice(-3000)); process.exitCode = 1; }).finally(async () => {
  await app?.close().catch(() => {});
  await stopObid(daemon);
  if (!process.env.OBY_KEEP) rmSync(scratch, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
});
