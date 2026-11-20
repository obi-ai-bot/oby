// The lab's browser screens for obid-spec's `lab.ui.*` nodes (tools/obid-spec/src/lab/ui.rs), as
// apps/electron/scripts/drive-app.mjs is for `app.*`: one helper per opened screen, started by
// `lab.ui.open` and kept until `lab.ui.close`. One JSON request per line on stdin, one JSON line back
// on stdout, `{"ok": true, "output": …}` or `{"ok": false, "error": "…"}`; the Rust node only
// orchestrates.
//
// The screen runs in chrome-headless-shell (the path the node gives, from obid-spec's own cache), with
// a profile of its own in the run's folder: never the person's Chrome, its profile, or their screen.
// Every picture is of the page this helper opened, never of the screen.
//
// A target names one element as drive-app.mjs's do: {"role", "name"}, {"label"}, {"text"} or
// {"css"}, with "exact", "hasText", "visible", "nth"/"last" and "within".
import { mkdirSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { dirname, isAbsolute } from 'node:path';
import { chromium } from 'playwright-core';

let server = null;
let browser = null;
let context = null;
let page = null;
let base = null;
let size = null;
const errors = [];
// the screen's own sends to the browser API (take, give back, a secret, your keys) still on their way:
// a click, typing or a key press answers once they are answered, so the next step sees their effect
let sending = 0;
async function delivered(timeout = 10_000) {
  const until = Date.now() + timeout;
  await new Promise((done) => setTimeout(done, 50));
  while (sending > 0 && Date.now() < until) await new Promise((done) => setTimeout(done, 25));
  return sending === 0;
}

function locate(target, root = page) {
  if (!target || typeof target !== 'object') throw new Error('a target is an object: role and name, label, text or css');
  const scope = target.within ? locate(target.within, root) : root;
  const exact = target.exact ?? true;
  let found;
  if (target.role) found = scope.getByRole(target.role, { ...(target.name !== undefined ? { name: target.name, exact } : {}) });
  else if (target.label) found = scope.getByLabel(target.label, { exact });
  else if (target.text) found = scope.getByText(target.text, { exact });
  else if (target.css) found = scope.locator(target.css);
  else throw new Error(`a target names role, label, text or css: ${JSON.stringify(target)}`);
  if (target.hasText) found = found.filter({ hasText: target.hasText });
  if (target.visible) found = found.filter({ visible: true });
  if (target.last) found = found.last();
  else if (Number.isInteger(target.nth)) found = found.nth(target.nth);
  return found;
}

async function one(target, timeout = 10_000) {
  const found = locate(target);
  const deadline = Date.now() + timeout;
  let count = 0;
  while (Date.now() < deadline) {
    count = await found.count();
    if (count === 1) return found;
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error(`${JSON.stringify(target)}: ${count} found, expected exactly one`);
}

const round = (n) => Math.round(n * 10) / 10;
const box = async (el) => {
  const b = await el.boundingBox();
  if (!b) throw new Error('it has no box: not laid out (display: none, or detached)');
  return { x: round(b.x), y: round(b.y), width: round(b.width), height: round(b.height) };
};
const shared = (a, b) => Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y));

// A token's value as the page computes a colour from it, so `rgb(…)` compares to `rgb(…)`.
const tokenColor = (p, token) => p.evaluate((t) => {
  const probe = document.createElement('span');
  probe.style.color = `var(${t})`;
  document.body.appendChild(probe);
  const c = getComputedStyle(probe).color;
  probe.remove();
  return c;
}, token);

async function describe(el, r) {
  const attrs = {};
  for (const a of r.attributes ?? []) attrs[a] = await el.getAttribute(a);
  // the words as a person reads them: each run of white space one space
  const out = { text: (await el.innerText({ timeout: 2000 })).replace(/\s+/g, ' ').trim(), visible: await el.isVisible(), attributes: attrs };
  // styles: {"background-color": "--held"} → its computed value, the token's, and whether they match
  if (r.styles) {
    out.styles = {};
    for (const [prop, token] of Object.entries(r.styles)) {
      const value = await el.evaluate((e, p) => getComputedStyle(e).getPropertyValue(p), prop);
      const want = token ? await tokenColor(page, token) : null;
      out.styles[prop] = { value, ...(token ? { token, tokenValue: want, matches: value.trim() === want } : {}) };
    }
  }
  return out;
}

// The pieces a board and its screen both have, by the app's and the design's own class names: the
// boxes compared one by one, the words too.
const PIECES = [
  '.bar', '.crumb', '.project-chip', '.bar-end .icon-button', '.panel-toggle',
  '.msg-bubble', '.msg-time', '.msg-task', '.msg-task-title', '.msg-task-meta', '.ss-tool-row', '.lt-answered', '.chat-live',
  '.chat-composer', '.composer-field', '.decision-card', '.tc-ask-line', '.tc-ask-text', '.secret-row', '.secret-note',
  '.br', '.br-tabs', '.br-tab', '.br-tabs-end .icon-button', '.br-nav', '.br-address', '.br-control', '.br-control-text', '.br-control .tc-option',
  '.br-view', '.agent-pointer', '.agent-label', '.you-pointer', '.br-shelf', '.br-file', '.br-tip', '.empty-words', '.where-bar', '.peek', '.ran',
  '.br-state', '.br-state-title', '.br-over', '.mini', '.cell', '.cell-name', '.sheet-title', '.sheet-sub', '.strip-box', '.told',
  '.sidebar-slot', '.rail-button', '.sidebar-item', '.sidebar-title', '.connector', '.convo-row', '.convo-kid', '.sidebar-archived',
  '.set-card', '.set-row', '.set-switch', '.set-head', '.set-foot', '.group-title',
  '.site-card', '.site-input', '.site-button', '.site-row', '.plan', '.kpi',
  '.control-pill', '.mac-app', '.modal', '.modal-title', '.modal-list li', '.steps', '.modal-actions button',
  '.msg-quote', '.tc-code',
];

// Today's app, as the app's own CSS draws it: compared and reported, never held to the board (the
// boards restate a few of the app's sizes slightly differently: README, "Not matched within 4px").
const TODAY = new Set([
  '.bar', '.crumb', '.project-chip', '.bar-end .icon-button', '.panel-toggle',
  '.msg-bubble', '.msg-time', '.msg-task', '.msg-task-title', '.msg-task-meta', '.ss-tool-row', '.lt-answered', '.chat-live',
  '.chat-composer', '.composer-field', '.msg-quote', '.tc-code',
  '.sidebar-slot', '.rail-button', '.sidebar-item', '.sidebar-title', '.connector', '.convo-row', '.convo-kid', '.sidebar-archived',
  '.group-title',
]);

async function pieces(p) {
  return p.evaluate((sels) => {
    const out = {};
    for (const s of sels) {
      out[s] = [...document.querySelectorAll(s)].filter((e) => e.getClientRects().length).map((e) => {
        const b = e.getBoundingClientRect();
        return { x: b.x, y: b.y, width: b.width, height: b.height, text: e.innerText?.replace(/\s+/g, ' ').trim() ?? '' };
      });
    }
    return out;
  }, PIECES);
}

async function boardPage(file, state) {
  const p = await context.newPage();
  await p.setViewportSize(size);
  await p.goto(`${base}/board/${file}.html${state ? `?state=${encodeURIComponent(state)}` : ''}`);
  await p.waitForLoadState('load');
  await p.evaluate(() => document.fonts.ready);
  // the boards animate the working dots and the sheen: still, for the picture
  await p.addStyleTag({ content: '*,*::before,*::after{animation:none!important;transition:none!important}' });
  return p;
}

async function still(p) {
  await p.addStyleTag({ content: '*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}' });
}

const ops = {
  async open(r) {
    if (!browser) {
      if (!r.chrome || !isAbsolute(r.chrome)) throw new Error('chrome: the browser’s absolute path');
      // headless, its temporary profile under TMPDIR (the node points it into the run's folder):
      // nothing of the person's, nothing on their screen
      server = await chromium.launchServer({ executablePath: r.chrome, headless: r.headless ?? true, args: ['--no-first-run', '--no-default-browser-check', '--disable-extensions'] });
      browser = await chromium.connect(server.wsEndpoint());
      context = await browser.newContext({ viewport: r.size ?? { width: 1600, height: 1000 }, deviceScaleFactor: r.scale ?? 1 });
      page = await context.newPage();
      page.on('pageerror', (e) => errors.push(e.message));
      page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
      const sends = (q) => q.method() !== 'GET' && new URL(q.url()).pathname.startsWith('/v1/');
      page.on('request', (q) => { if (sends(q)) sending++; });
      page.on('requestfinished', (q) => { if (sends(q)) sending--; });
      page.on('requestfailed', (q) => { if (sends(q)) sending--; });
    }
    if (!r.url || !/^http:\/\/127\.0\.0\.1:\d+\//.test(r.url)) throw new Error('url: the lab app’s, on 127.0.0.1');
    base = new URL(r.url).origin;
    await page.setViewportSize(r.size ?? { width: 1600, height: 1000 });
    await page.goto(r.url);
    await page.waitForSelector('[data-lab-window], .lab-index', { timeout: 15_000 });
    // the board's own size, from its window, unless the step names one
    size = r.size ?? await page.evaluate(() => {
      const w = document.querySelector('[data-lab-window]');
      const b = w?.getBoundingClientRect();
      return b ? { width: Math.round(b.width), height: Math.round(b.height) } : { width: 1280, height: 800 };
    });
    await page.setViewportSize(size);
    await page.reload();
    await page.waitForSelector('[data-lab-window], .lab-index', { timeout: 15_000 });
    await page.evaluate(() => document.fonts.ready);
    return { pid: server.process()?.pid ?? null, url: page.url(), size, title: await page.title() };
  },
  async click(r) {
    const el = await one(r.target, r.timeout);
    // position: where in the element, in its own px (a spot on the live page)
    const at = r.position ? { position: r.position } : {};
    if (r.hover) await el.hover({ timeout: r.timeout ?? 10_000, ...at });
    else await el.click({ timeout: r.timeout ?? 10_000, ...at });
    return { clicked: !r.hover, hovered: !!r.hover, delivered: await delivered() };
  },
  async type(r) {
    if (typeof r.text !== 'string') throw new Error('text: the words to type');
    if (r.target) await (await one(r.target, r.timeout)).click({ timeout: r.timeout ?? 10_000 });
    await page.keyboard.type(r.text);
    // a secret is typed, never answered back: its length only
    return { typed: r.text.length, delivered: await delivered() };
  },
  async press(r) {
    if (typeof r.keys !== 'string') throw new Error('keys: what to press, as Playwright names it (Enter, Escape)');
    if (r.target) await (await one(r.target, r.timeout)).focus();
    for (let i = 0; i < (r.times ?? 1); i++) await page.keyboard.press(r.keys);
    return { pressed: r.keys, times: r.times ?? 1, delivered: await delivered() };
  },
  // an element that goes while it's read (a state changing under the read) is read again
  async read(r) {
    for (let attempt = 0; ; attempt++) {
      try {
        const found = locate(r.target);
        const count = await found.count();
        const items = [];
        for (let i = 0; i < Math.min(count, r.max ?? 20); i++) items.push(await describe(found.nth(i), r));
        return { count, items, ...(items[0] ?? { text: null, visible: false, attributes: {} }) };
      } catch (e) {
        if (attempt >= 2) throw e;
        await new Promise((done) => setTimeout(done, 150));
      }
    }
  },
  async measure(r) {
    const a = await box(await one(r.target, r.timeout));
    const out = { box: a };
    for (const [name, target] of Object.entries(r.against ?? {})) {
      const b = await box(await one(target, r.timeout));
      const area = shared(a, b);
      out[name] = { box: b, overlap: area > 0, sharedArea: round(area), gap: round(Math.max(b.y - (a.y + a.height), a.y - (b.y + b.height), b.x - (a.x + a.width), a.x - (b.x + b.width))) };
    }
    return out;
  },
  async reload() {
    await page.reload();
    await page.waitForSelector('[data-lab-window]', { timeout: 15_000 });
    return { url: page.url() };
  },
  async goto(r) {
    if (!r.url || new URL(r.url).origin !== base) throw new Error('url: a page of the lab app’s own');
    await page.goto(r.url);
    await page.waitForSelector('[data-lab-window], .lab-index', { timeout: 15_000 });
    return { url: page.url() };
  },
  async screenshot(r) {
    if (!r.path || !isAbsolute(r.path)) throw new Error('path: an absolute file in the run’s folder');
    mkdirSync(dirname(r.path), { recursive: true });
    await still(page);
    await page.screenshot({ path: r.path, clip: { x: 0, y: 0, ...size } });
    return { path: r.path };
  },
  // The screen beside its design board, box by box: every piece both have (PIECES), matched by its
  // class and its place in the page, must sit within `tolerance` px, and its words must be the same.
  async compare(r) {
    if (!r.board) throw new Error('board: the board’s file name (design/boards/<board>.dc.html)');
    const tolerance = r.tolerance ?? 4;
    await still(page);
    await page.waitForTimeout(50);
    const b = await boardPage(r.board, r.state);
    const [mine, theirs] = [await pieces(page), await pieces(b)];
    await b.close();
    const skip = new Set(r.skip ?? []);
    const off = [];
    const today = [];
    const words = [];
    const counts = [];
    let worst = 0;
    let compared = 0;
    for (const sel of PIECES) {
      if (skip.has(sel)) continue;
      const m = mine[sel], t = theirs[sel];
      if (m.length !== t.length) { if (m.length || t.length) (TODAY.has(sel) ? today : counts).push({ piece: sel, screen: m.length, board: t.length }); continue; }
      t.forEach((tb, i) => {
        const mb = m[i];
        const d = Math.max(Math.abs(mb.x - tb.x), Math.abs(mb.y - tb.y), Math.abs(mb.width - tb.width), Math.abs(mb.height - tb.height));
        const entry = { piece: `${sel}[${i}]`, by: round(d), screen: { x: round(mb.x), y: round(mb.y), w: round(mb.width), h: round(mb.height) }, board: { x: round(tb.x), y: round(tb.y), w: round(tb.width), h: round(tb.height) } };
        if (TODAY.has(sel)) { if (d > tolerance) today.push(entry); if (mb.text !== tb.text) words.push({ piece: entry.piece, screen: mb.text.slice(0, 160), board: tb.text.slice(0, 160) }); return; }
        compared++;
        worst = Math.max(worst, d);
        if (d > tolerance) off.push({ piece: `${sel}[${i}]`, by: round(d), screen: { x: round(mb.x), y: round(mb.y), w: round(mb.width), h: round(mb.height) }, board: { x: round(tb.x), y: round(tb.y), w: round(tb.width), h: round(tb.height) } });
        if (mb.text !== tb.text) words.push({ piece: `${sel}[${i}]`, screen: mb.text.slice(0, 160), board: tb.text.slice(0, 160) });
      });
    }
    return { board: r.board, tolerance, compared, worst: round(worst), within: off.length === 0 && counts.length === 0, off: off.slice(0, 40), offCount: off.length, sameWords: words.length === 0, words: words.slice(0, 40), counts, today: today.slice(0, 40), todayCount: today.length };
  },
  // The screen and its board side by side in one PNG (the board left, the screen right), for the
  // owner's look; made in a page of this helper's own.
  async sidebyside(r) {
    if (!r.path || !isAbsolute(r.path)) throw new Error('path: an absolute file in the run’s folder');
    if (!r.board) throw new Error('board: the board’s file name');
    await still(page);
    const mine = await page.screenshot({ clip: { x: 0, y: 0, ...size } });
    const b = await boardPage(r.board, r.state);
    const theirs = await b.screenshot({ clip: { x: 0, y: 0, ...size } });
    await b.close();
    const gap = 24, head = 36;
    const w = size.width * 2 + gap * 3, h = size.height + head + gap;
    const c = await context.newPage();
    await c.setViewportSize({ width: w, height: h });
    const img = (buf) => `data:image/png;base64,${buf.toString('base64')}`;
    await c.setContent(`<body style="margin:0;background:#1c1c1e;font:600 14px -apple-system,system-ui,sans-serif;color:#d0d0d4">
      <div style="position:absolute;left:${gap}px;top:10px">Design board · ${r.board}${r.state ? ` (${r.state})` : ''}</div>
      <div style="position:absolute;left:${gap * 2 + size.width}px;top:10px">Lab screen · ${r.label ?? ''}</div>
      <img src="${img(theirs)}" style="position:absolute;left:${gap}px;top:${head}px;width:${size.width}px;height:${size.height}px">
      <img src="${img(mine)}" style="position:absolute;left:${gap * 2 + size.width}px;top:${head}px;width:${size.width}px;height:${size.height}px"></body>`);
    mkdirSync(dirname(r.path), { recursive: true });
    await c.screenshot({ path: r.path });
    await c.close();
    return { path: r.path, width: w, height: h };
  },
  async close() {
    const pid = server?.process()?.pid ?? null;
    await context?.close().catch(() => {});
    await browser?.close().catch(() => {});
    await server?.close().catch(() => {});
    context = null; browser = null; page = null; server = null;
    return { closed: pid, pageErrors: errors };
  },
};

// for a quick look by hand: node scripts/drive-lab.mjs < requests.jsonl
const lines = createInterface({ input: process.stdin });
for await (const line of lines) {
  if (!line.trim()) continue;
  let reply;
  try {
    const r = JSON.parse(line);
    const op = ops[r.op];
    if (!op) throw new Error(`no op ${r.op}: ${Object.keys(ops).join(', ')}`);
    reply = { ok: true, output: await op(r) };
  } catch (e) {
    reply = { ok: false, error: e?.message ?? String(e) };
  }
  process.stdout.write(`${JSON.stringify(reply)}\n`);
  if (reply.ok && line.includes('"close"')) break;
}
process.exit(0);
