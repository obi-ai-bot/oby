// The real app for obid-spec's `app.*` nodes (tools/obid-spec, docs/contracts/obid-spec.md): one
// helper per launched app, started by `app.launch` and kept until `app.close`. It reads one JSON
// request per line on stdin and answers one JSON line on stdout, `{"ok": true, "output": …}` or
// `{"ok": false, "error": "…"}`; the Rust node only orchestrates. The app is launched through
// drive-lib.mjs's launcher (Playwright's _electron.launch), hidden, with its own user-data folder,
// on the workflow's scratch obid (OBID_PORT, OBID_HOME), never the owner's on 7777.
//
// A target names one element the way a person finds it: {"role": "button", "name": "Copy"},
// {"label": "…"}, {"text": "…"}, or {"css": "…"} where nothing a person reads names it; with
// "exact" (default true), "hasText", "nth" (or "last": true), and "within" (another target the
// element is inside).
import { mkdirSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { launch, skipFirstRun } from './drive-lib.mjs';
import { boardFile, judge, pieces } from './board-compare.mjs';

let app = null;
let page = null;
let scratch = null;
const errors = [];

function locate(target, root = page) {
  if (!target || typeof target !== 'object') throw new Error('a target is an object: role and name, label, text or css');
  let scope = target.within ? locate(target.within, root) : root;
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

// One element: exactly one found (or the one `nth`/`last` picked), else an error saying how many.
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

// styles: {"background-color": "--held"} → the computed value, the app's token's, and whether they
// match (a token's colour as the page computes it, so `rgb(…)` compares to `rgb(…)`)
async function describe(el, attributes, styles) {
  const attrs = {};
  for (const a of attributes ?? []) attrs[a] = await el.getAttribute(a);
  const out = { text: (await el.innerText()).trim(), visible: await el.isVisible(), attributes: attrs };
  if (styles) {
    out.styles = {};
    for (const [prop, token] of Object.entries(styles)) {
      const value = (await el.evaluate((e, p) => getComputedStyle(e).getPropertyValue(p), prop)).trim();
      const want = token ? await page.evaluate((t) => {
        const probe = document.createElement('span');
        probe.style.color = `var(${t})`;
        document.body.appendChild(probe);
        const c = getComputedStyle(probe).color;
        probe.remove();
        return c;
      }, token) : null;
      out.styles[prop] = { value, ...(token ? { token, tokenValue: want, matches: value === want } : {}) };
    }
  }
  return out;
}

const box = async (el) => {
  const b = await el.boundingBox();
  if (!b) throw new Error('it has no box: not laid out (display: none, or detached)');
  return { x: Math.round(b.x * 10) / 10, y: Math.round(b.y * 10) / 10, width: Math.round(b.width * 10) / 10, height: Math.round(b.height * 10) / 10 };
};
// The area two boxes share, 0 when they only touch or are apart
const shared = (a, b) => Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y));

// Still, for a measure or a picture: no animation, no transition, no caret
async function still() {
  await page.addStyleTag({ content: '*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}' });
  await page.waitForTimeout(50);
}

// A design board in a hidden window of the app's own, at the window's size: its page, and its close
async function boardPage(board, state) {
  if (!scratch) throw new Error('launch first');
  const url = boardFile(board, state, join(scratch, 'boards'));
  const size = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }));
  const opened = app.waitForEvent('window');
  const id = await app.evaluate(({ BrowserWindow }, a) => {
    const w = new BrowserWindow({ show: false, width: a.size.width, height: a.size.height, useContentSize: true, webPreferences: { sandbox: true, contextIsolation: true } });
    void w.loadURL(a.url);
    return w.id;
  }, { url, size });
  const p = await opened;
  await p.waitForLoadState('load');
  await p.evaluate(() => document.fonts.ready);
  return { page: p, id, size, close: () => app.evaluate(({ BrowserWindow }, i) => BrowserWindow.fromId(i)?.destroy(), id).catch(() => {}) };
}

const ops = {
  async launch(r) {
    if (app) throw new Error('this helper already launched its app');
    for (const k of ['scratch', 'userData']) if (!r[k] || !isAbsolute(r[k])) throw new Error(`${k} is an absolute folder`);
    if (!r.obid?.port || !r.obid?.home) throw new Error('obid: the scratch obid’s port and home (never the owner’s on 7777)');
    if (Number(r.obid.port) === 7777) throw new Error('7777 is the owner’s obid: a workflow runs its own');
    mkdirSync(r.userData, { recursive: true, mode: 0o700 });
    // the person's environment but for where obid is, and nothing that points the page at a dev server
    const { ELECTRON_RENDERER_URL, OBID_PORT, OBID_HOME, ...rest } = process.env;
    const env = { ...rest, OBID_PORT: String(r.obid.port), OBID_HOME: r.obid.home };
    scratch = r.scratch;
    app = await launch({ dir: r.scratch, userData: r.userData, env, hidden: r.hidden ?? true, ...(r.main ? { main: resolve(r.main) } : {}) });
    page = await app.firstWindow();
    page.on('pageerror', (e) => errors.push(e.message));
    await page.waitForLoadState('domcontentloaded');
    if (r.skipFirstRun ?? true) {
      await skipFirstRun(page);
      await page.waitForLoadState('domcontentloaded');
    }
    // the app's demo data (src/renderer/src/mock.ts) in the chat-first window, for what needs no obid
    if (r.mock) {
      // storage: the window's own remembered choices for the demo (the side panel kept open or closed)
      if (r.storage) {
        if (typeof r.storage !== 'object' || Object.entries(r.storage).some(([k, v]) => !/^oby\.[A-Za-z.]+$/.test(k) || typeof v !== 'string')) throw new Error('storage: oby.* keys with string values');
        await page.evaluate((kept) => { for (const [k, v] of Object.entries(kept)) localStorage.setItem(k, v); }, r.storage);
      }
      const url = new URL(page.url());
      // query: more of the demo's words after ?everything&chat (browser=<n>: a board of the browser's design)
      if (r.query !== undefined && !/^[a-z0-9=&_-]*$/i.test(r.query)) throw new Error('query: letters, digits, =, & and dashes');
      url.search = `?everything&chat${r.query ? `&${r.query}` : ''}`;
      await page.goto(url.href);
      await page.waitForLoadState('domcontentloaded');
    }
    // invisible still, but able to take the keyboard while a desktop.* step brings it forward to
    // press keys through macOS (agent-desktop), as a person's keys reach the menu bar
    if (r.size) await app.evaluate(({ BrowserWindow }, s) => { const w = BrowserWindow.getAllWindows()[0]; w?.setResizable(true); w?.setSize(s.width, s.height); }, r.size);
    // the demo with choices of its own (storage) opens again at its size, so they apply as at that
    // width (the side panel docks where it fits)
    if (r.mock && r.storage && r.size) {
      await page.reload();
      await page.waitForLoadState('domcontentloaded');
    }
    if (r.focusable) await app.evaluate(({ BrowserWindow }) => { for (const w of BrowserWindow.getAllWindows()) w.setFocusable(true); });
    return { pid: app.process().pid, title: await page.title() };
  },
  async click(r) {
    const el = await one(r.target, r.timeout);
    if (r.hover) await el.hover({ timeout: r.timeout ?? 10_000 });
    else await el.click({ timeout: r.timeout ?? 10_000 });
    return { clicked: !r.hover, hovered: !!r.hover };
  },
  async type(r) {
    if (typeof r.text !== 'string') throw new Error('text: the words to type');
    if (r.target) await (await one(r.target, r.timeout)).click({ timeout: r.timeout ?? 10_000 });
    // a message box is contenteditable: its words go in as the keyboard inserts them
    await page.keyboard.insertText(r.text);
    return { typed: r.text.length };
  },
  async press(r) {
    if (typeof r.keys !== 'string') throw new Error('keys: what to press, as Playwright names it (Enter, Meta+B)');
    if (r.target) await (await one(r.target, r.timeout)).focus();
    await page.keyboard.press(r.keys);
    return { pressed: r.keys };
  },
  async read(r) {
    const found = locate(r.target);
    const count = await found.count();
    const items = [];
    for (let i = 0; i < Math.min(count, r.max ?? 20); i++) items.push(await describe(found.nth(i), r.attributes, r.styles));
    return { count, items, ...(items[0] ?? { text: null, visible: false, attributes: {} }) };
  },
  async measure(r) {
    const a = await box(await one(r.target, r.timeout));
    const out = { box: a };
    for (const [name, target] of Object.entries(r.against ?? {})) {
      const b = await box(await one(target, r.timeout));
      const area = shared(a, b);
      out[name] = { box: b, overlap: area > 0, sharedArea: Math.round(area * 10) / 10, gap: Math.round(Math.max(b.y - (a.y + a.height), a.y - (b.y + b.height), b.x - (a.x + a.width), a.x - (b.x + b.width)) * 10) / 10 };
    }
    return out;
  },
  async screenshot(r) {
    if (!r.path || !isAbsolute(r.path)) throw new Error('path: an absolute file in the run’s folder');
    mkdirSync(dirname(r.path), { recursive: true });
    await page.screenshot({ path: r.path });
    return { path: r.path };
  },
  // The window beside a design board (board-compare.mjs), box by box: every piece both have, matched
  // by its class and its place in the page, within `tolerance` px and the same words.
  async compare(r) {
    if (!r.board) throw new Error('board: the board’s file name (design/boards/<board>.dc.html)');
    await still();
    const b = await boardPage(r.board, r.state);
    try {
      const [mine, theirs] = [await pieces(page), await pieces(b.page)];
      return judge(r.board, mine, theirs, { tolerance: r.tolerance ?? 4, skip: r.skip ?? [] });
    } finally { await b.close(); }
  },
  // The board and the window side by side in one PNG (the board left, the app right), for the owner's look
  async sidebyside(r) {
    if (!r.path || !isAbsolute(r.path)) throw new Error('path: an absolute file in the run’s folder');
    if (!r.board) throw new Error('board: the board’s file name');
    await still();
    const mine = await page.screenshot();
    const b = await boardPage(r.board, r.state);
    try {
      const theirs = await b.page.screenshot();
      const { width, height } = b.size;
      const gap = 24, head = 36, w = width * 2 + gap * 3, h = height + head + gap;
      await app.evaluate(({ BrowserWindow }, a) => BrowserWindow.fromId(a.id)?.setContentSize(a.w, a.h), { id: b.id, w, h });
      const img = (buf) => `data:image/png;base64,${buf.toString('base64')}`;
      await b.page.setContent(`<body style="margin:0;background:#1c1c1e;font:600 14px -apple-system,system-ui,sans-serif;color:#d0d0d4">
        <div style="position:absolute;left:${gap}px;top:10px">Design board · ${r.board}${r.state ? ` (${r.state})` : ''}</div>
        <div style="position:absolute;left:${gap * 2 + width}px;top:10px">The app · ${r.label ?? ''}</div>
        <img src="${img(theirs)}" style="position:absolute;left:${gap}px;top:${head}px;width:${width}px;height:${height}px">
        <img src="${img(mine)}" style="position:absolute;left:${gap * 2 + width}px;top:${head}px;width:${width}px;height:${height}px"></body>`);
      await b.page.waitForTimeout(100);
      mkdirSync(dirname(r.path), { recursive: true });
      await b.page.screenshot({ path: r.path, clip: { x: 0, y: 0, width: w, height: h } });
      return { path: r.path, width: w, height: h };
    } finally { await b.close(); }
  },
  async close() {
    const pid = app?.process().pid;
    await app?.evaluate(({ app: a }) => a.exit(0)).catch(() => {});
    await app?.close().catch(() => {});
    app = null;
    return { closed: pid ?? null, pageErrors: errors };
  },
};

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
