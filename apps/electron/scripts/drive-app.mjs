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
import { dirname, isAbsolute, resolve } from 'node:path';
import { launch, skipFirstRun } from './drive-lib.mjs';

let app = null;
let page = null;
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

async function describe(el, attributes) {
  const attrs = {};
  for (const a of attributes ?? []) attrs[a] = await el.getAttribute(a);
  return { text: (await el.innerText()).trim(), visible: await el.isVisible(), attributes: attrs };
}

const box = async (el) => {
  const b = await el.boundingBox();
  if (!b) throw new Error('it has no box: not laid out (display: none, or detached)');
  return { x: Math.round(b.x * 10) / 10, y: Math.round(b.y * 10) / 10, width: Math.round(b.width * 10) / 10, height: Math.round(b.height * 10) / 10 };
};
// The area two boxes share, 0 when they only touch or are apart
const shared = (a, b) => Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y));

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
      const url = new URL(page.url());
      url.search = '?everything&chat';
      await page.goto(url.href);
      await page.waitForLoadState('domcontentloaded');
    }
    // invisible still, but able to take the keyboard while a desktop.* step brings it forward to
    // press keys through macOS (agent-desktop), as a person's keys reach the menu bar
    if (r.focusable) await app.evaluate(({ BrowserWindow }) => { for (const w of BrowserWindow.getAllWindows()) w.setFocusable(true); });
    if (r.size) await app.evaluate(({ BrowserWindow }, s) => { const w = BrowserWindow.getAllWindows()[0]; w?.setResizable(true); w?.setSize(s.width, s.height); }, r.size);
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
    for (let i = 0; i < Math.min(count, r.max ?? 20); i++) items.push(await describe(found.nth(i), r.attributes));
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
