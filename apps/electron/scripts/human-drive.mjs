// Oby driven as its owner drives it: this checkout's built app, on the owner's own obid (7777) and
// their real agent logins, through the window only (docs/human-drive/research.md). Every word is
// typed into a message box and sent with a real Enter, every answer is a real click, so main signs
// them as the owner's (docs/contracts/signed-words.md); obid's job files are read for evidence only,
// never to drive. Scenarios are in human-drive.scenarios.mjs; each run writes report.md,
// report.json, the screenshots and main's log to its own folder, outside this repository.
//
// Real side effects: tasks on the owner's Claude account; an issue, pushed branches, pull requests
// and merges in OBY_DRIVE_REPO (a scratch repository the owner named, never one of obi-ai-bot's);
// a device of the driver's own registered with obid for the run. At the end the driver closes the
// issue and the pull requests this run made, deletes their `oby/` branches and revokes its device.
//
// Run: npm run build, then OBY_DRIVE_REPO=owner/name npm run drive [-- --only id,id] [-- --list].
// OBY_DRIVE_SHOW=1 shows the window while it runs; OBY_DRIVE_OUT picks the run's folder.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { appRoot, electronBinary, launch, sleep, until as waitFor } from './drive-lib.mjs';
import scenarios from './human-drive.scenarios.mjs';

const runner = join(homedir(), 'Library/Application Support/Oby/Runner');
// the driver's own app folder: its own device key, never the installed app's (this build couldn't
// open that one, and would set it aside)
const own = join(homedir(), 'Library/Application Support/oby-human-drive');
const DEVICE = 'Oby human drive (test, revoked after each run)';
const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
const out = process.env.OBY_DRIVE_OUT || join(own, 'runs', stamp);
const args = process.argv.slice(2);
const gh = (...a) => execFileSync('gh', a, { encoding: 'utf8', timeout: 60_000 }).trim();
const until = (label, f, ms) => waitFor(label, f, ms, 500);

if (args.includes('--list')) {
  for (const s of scenarios) console.log(`${s.id.padEnd(16)} ${s.title}${s.needs ? `  (after ${s.needs.join(', ')})` : ''}`);
  process.exit(0);
}
const fail = (why) => { console.error(why); process.exit(2); };
const only = args.includes('--only') ? (args[args.indexOf('--only') + 1] ?? '').split(',').filter((id) => id && !id.startsWith('-')) : null;
if (only && !only.length) fail('--only takes scenario ids, comma-separated: npm run drive -- --only open,research (-- --list lists them)');
const unknown = only?.find((id) => !scenarios.some((s) => s.id === id));
if (unknown) fail(`no scenario “${unknown}” (npm run drive -- --list)`);
// owner/name, however it's written (a URL, `.git`, any case); never one of obi-ai-bot's
const repo = (process.env.OBY_DRIVE_REPO ?? '').trim().toLowerCase()
  .replace(/^(https?:\/\/|ssh:\/\/)?(git@)?github\.com[:/]/, '').replace(/\.git$/, '').replace(/\/+$/, '');
if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) fail('Name the scratch repository the tasks may push to and merge in: OBY_DRIVE_REPO=owner/name npm run drive');
if (repo.startsWith('obi-ai-bot/')) fail(`Never drive the real tasks into ${repo}: name a scratch repository in OBY_DRIVE_REPO.`);

// The driver's own device with obid, under DEVICE: registered before the app opens, revoked at the
// end (human-drive-device.cjs, a one-shot Electron app with the same key, so it works after the
// driven app has closed). Main's own registration of that key then finds it known; a revoke a hard
// kill missed is done by the next run's, since the key, so the device, is the same.
function device(action) {
  try {
    const out = execFileSync(electronBinary, [join(appRoot, 'scripts/human-drive-device.cjs'), action, join(own, 'electron-data/device-key'), runner, DEVICE],
      { encoding: 'utf8', timeout: 60_000, stdio: ['ignore', 'pipe', 'ignore'] });
    return JSON.parse(out.split('\n').find((l) => l.startsWith('DEVICE '))?.slice(7) ?? '{"error":"no answer"}');
  } catch (e) { return { error: e.message.split('\n')[0] }; }
}

async function openApp() {
  mkdirSync(join(own, 'electron-data'), { recursive: true });
  // no OBID_PORT or OBID_HOME: the owner's own obid, as the app finds it
  const { OBID_PORT, OBID_HOME, ...env } = process.env;
  const app = await launch({ dir: own, userData: join(own, 'electron-data'), env, hidden: !process.env.OBY_DRIVE_SHOW });
  const page = await app.firstWindow();
  await page.waitForLoadState('domcontentloaded');
  // set up already: Home, not the first run's screens (the owner went through those)
  await page.evaluate(() => { for (const k of ['oby.onboarded', 'oby.setUp', 'oby.welcomed']) localStorage.setItem(k, '1'); });
  await page.reload();
  await page.waitForLoadState('domcontentloaded');
  return { app, page };
}

// The jobs obid kept for this run's repository since `after`, as it keeps them (evidence only)
const jobsSince = (after) => readdirSync(join(runner, 'jobs')).filter((f) => f.endsWith('.json'))
  .map((f) => { try { return JSON.parse(readFileSync(join(runner, 'jobs', f), 'utf8')); } catch { return null; } })
  // by `created` (seconds): obid rewrites a job's file, so the file's own times say nothing
  .filter((j) => j && j.created * 1000 >= after - 1000 && j.repo?.toLowerCase() === repo);

// What a scenario gets: the page, and the few things a person does or looks at
const REPLY = '.msg.is-oby .msg-bubble';
function driver(page, record, state) {
  let said = 0;
  const shown = () => page.evaluate(() => document.body.innerText);
  const t = {
    page, repo, state, shown,
    step(label) { record.steps.push({ label, at: Date.now() - record.started }); },
    note(text) { record.notes.push(text); },
    async shot(name) {
      await sleep(600); // a page that's still fading in or out, settled
      const file = `${record.id}-${String(record.shots.length + 1).padStart(2, '0')}-${name}.png`;
      await page.screenshot({ path: join(out, file) });
      record.shots.push(file);
      return file;
    },
    // waits until the window shows this (a string or a RegExp), and gives what matched
    async see(what, { ms = 60_000, label } = {}) {
      const test = (text) => (typeof what === 'string' ? text.includes(what) && what : text.match(what)?.[0]);
      const seen = await until(label ?? `“${what}”`, async () => test(await shown()), ms);
      t.step(`saw ${label ?? JSON.stringify(seen).slice(0, 80)}`);
      return seen;
    },
    // types the words into the visible message box (the last one with this placeholder, if given)
    // and presses Enter, as the owner does
    async say(words, placeholder) {
      // the box a person sees: one that's leaving (a page fading out) isn't it
      const box = page.locator(placeholder ? `[contenteditable="true"][aria-placeholder^="${placeholder}"]` : '[contenteditable="true"]')
        .filter({ visible: true }).last();
      await until('a message box', () => box.evaluate((el) => el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })), 20_000);
      await sleep(500);
      said = await page.locator(REPLY).count();
      // a click, as the owner's; while something animates over the box, its focus (the Enter
      // below is the input that signs)
      await box.click({ timeout: 5000 }).catch(() => box.focus());
      await page.keyboard.insertText(words);
      await page.keyboard.press('Enter');
      t.step(`said “${words.slice(0, 80)}”`);
    },
    // waits for Oby's next reply (after the last words said, or the last reply), done writing, and
    // gives its text
    async reply({ ms = 180_000 } = {}) {
      const text = await until('Oby’s reply', async () => {
        const all = page.locator(REPLY);
        if ((await all.count()) <= said || (await page.locator('.msg.is-streaming, .msg-step').count())) return;
        const a = await all.last().innerText();
        await sleep(1500);
        return a.trim() && a === (await all.last().innerText()) && a;
      }, ms);
      // the next reply() waits for the one after this
      said = await page.locator(REPLY).count();
      t.step(`Oby replied “${text.slice(0, 80).replace(/\n/g, ' ')}…”`);
      return text;
    },
    // clicks the button a person would, by the name they read on it
    async click(name, within) {
      const scope = within ? page.locator(within) : page;
      const button = scope.getByRole('button', { name, exact: typeof name === 'string' }).first();
      await until(`the “${name}” button`, () => button.isVisible(), 20_000);
      await button.click();
      t.step(`clicked “${name}”`);
    },
    // evidence only: a job as obid keeps it
    job: (id) => id && JSON.parse(readFileSync(join(runner, 'jobs', `${id}.json`), 'utf8')),
    // evidence only: the newest job in this run's repository since then (by default, since the
    // scenario started), not one of Oby's own reviews; never another repository's
    newestJob: (after = record.started) => jobsSince(after).filter((j) => j.kind !== 'review').sort((a, b) => b.created - a.created)[0],
    gh,
  };
  return t;
}

// What this run made in the scratch repository, undone: the issues it opened (t.state.issues) and the
// pull requests and `oby/` branches of the tasks it started (t.state.jobs). Nothing older, nothing else.
function cleanUp(state) {
  const done = [];
  const tryIt = (what, f) => { try { if (f() !== false) done.push(what); } catch (e) { done.push(`${what}: failed (${e.message.split('\n')[0]})`); } };
  for (const url of state.issues ?? []) tryIt(`closed ${url}`, () => gh('issue', 'close', url, '-c', 'Closed by the human-drive driver at the end of its run.'));
  const jobs = (state.jobs ?? []).map((id) => { try { return JSON.parse(readFileSync(join(runner, 'jobs', `${id}.json`), 'utf8')); } catch { return null; } })
    .filter((j) => j?.repo?.toLowerCase() === repo);
  for (const url of new Set(jobs.map((j) => j.published).filter(Boolean))) {
    tryIt(`closed ${url}`, () => JSON.parse(gh('pr', 'view', url, '--json', 'state')).state === 'OPEN' && (gh('pr', 'close', url, '-c', 'Closed by the human-drive driver at the end of its run.'), true));
  }
  for (const branch of new Set(jobs.map((j) => j.branch).filter((b) => b?.startsWith('oby/')))) {
    tryIt(`deleted branch ${branch}`, () => {
      try { execFileSync('gh', ['api', `repos/${repo}/git/ref/heads/${branch}`], { stdio: 'ignore', timeout: 60_000 }); } catch { return false; } // not pushed, or gone
      gh('api', '-X', 'DELETE', `repos/${repo}/git/refs/heads/${branch}`);
    });
  }
  return done;
}

// The titles of the cards on Home before the run: the owner's own. A task this run starts may not
// share one (human-drive.scenarios.mjs, track), or its card couldn't be told from theirs.
async function homeTitles(page) {
  const top = page.locator('section[aria-label="Oby is asking"] article.ask-card.is-up');
  // Home shown, then a moment for obid's inbox to fill the stack (it comes after the page)
  await until('Home', () => page.locator('[contenteditable="true"]').filter({ visible: true }).count(), 30_000);
  await until('Home’s cards', () => top.count(), 8_000).catch(() => {}); // none: nothing of the owner's to tell apart
  if (!(await top.count())) return [];
  const total = Number((await top.locator('.ask-count').innerText()).split('/')[1] ?? 1);
  const titles = new Set();
  for (let i = 0; i < total; i++) {
    titles.add((await top.locator('h2.tc-title').innerText()).trim());
    const next = top.getByRole('button', { name: 'Next' });
    if (!(await next.count())) break;
    await next.click();
    await sleep(400);
  }
  return [...titles];
}

const report = { repo, started: new Date().toISOString(), commit: execFileSync('git', ['rev-parse', '--short', 'HEAD'], { encoding: 'utf8', timeout: 10_000 }).trim(), scenarios: [] };
mkdirSync(out, { recursive: true });
const registered = device('register');
if (registered.error || !registered.id) fail(`the driver's device didn't register with obid: ${registered.error ?? 'no id'}`);
report.device = registered.id;
const { app, page } = await openApp();
const errors = [], mainLog = [];
page.on('pageerror', (e) => errors.push(e.message));
// main's own lines: a signature it couldn't make says so here (main/device.ts, ownersWords)
for (const stream of [app.process().stdout, app.process().stderr]) stream?.on('data', (b) => mainLog.push(b.toString()));
const state = {};

// The end of every run, once, whatever stopped it (an error, Control-C, a kill): the app closed (on
// Control-C Playwright closes it itself), the device revoked, then the cleanup, then the report. Each step's failure is
// written down and doesn't stop the next.
let finishing;
const finish = () => (finishing ??= (async () => {
  report.cleanup = [];
  await app.close().catch(() => {});
  // always: a key obid doesn't know is nothing to revoke
  const revoked = device('revoke');
  report.cleanup.push(revoked.revoked ? `revoked the driver's device${revoked.id ? ` ${revoked.id}` : ' (not registered)'}` : `the driver's device wasn't revoked: ${revoked.error}`);
  try { report.cleanup.push(...cleanUp(state)); } catch (e) { report.cleanup.push(`cleanup failed: ${e.message.split('\n')[0]}`); }
  report.pageErrors = errors;
  report.unsigned = mainLog.join('').split('\n').filter((l) => /couldn.t sign/.test(l));
  writeFileSync(join(out, 'main.log'), mainLog.join(''));
  writeFileSync(join(out, 'report.json'), JSON.stringify(report, null, 2));
  writeFileSync(join(out, 'report.md'), markdown(report));
  for (const line of report.cleanup) console.log(`cleanup: ${line}`);
  console.log(`report: ${join(out, 'report.md')}`);
})());
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => finish().finally(() => process.exit(130)));

try {
  state.homeTitles = new Set(await homeTitles(page));
  report.ownersCards = state.homeTitles.size;
  for (const s of scenarios) {
    if (only && !only.includes(s.id)) continue;
    const record = { id: s.id, title: s.title, contract: s.contract ?? '', status: 'pass', started: Date.now(), steps: [], notes: [], shots: [] };
    report.scenarios.push(record);
    const blocked = (s.needs ?? []).find((n) => report.scenarios.find((r) => r.id === n)?.status !== 'pass');
    if (blocked) { record.status = 'skip'; record.error = `needs ${blocked}`; continue; }
    if (page.isClosed()) { record.status = 'skip'; record.error = 'the window closed'; continue; }
    console.log(`▶ ${s.id}: ${s.title}`);
    try {
      await s.run(driver(page, record, state));
    } catch (e) {
      record.status = 'fail';
      record.error = e.message.split('\n')[0];
      await driver(page, record, state).shot('failed').catch(() => {});
    }
    record.seconds = Math.round((Date.now() - record.started) / 1000);
    console.log(`  ${record.status} in ${record.seconds} s${record.error ? `: ${record.error}` : ''}`);
  }
} finally {
  await finish();
}
if (report.scenarios.some((r) => r.status === 'fail')) process.exitCode = 1;

function markdown(r) {
  const lines = [`# Oby, driven as its owner drives it`, '', `${r.started} · commit ${r.commit} · repository ${r.repo}`, '',
    '| Scenario | Contract | Result | Time |', '|---|---|---|---|',
    ...r.scenarios.map((s) => `| ${s.id}: ${s.title} | ${s.contract} | ${s.status}${s.error ? `: ${s.error}` : ''} | ${s.seconds ?? 0} s |`), ''];
  for (const s of r.scenarios) {
    lines.push(`## ${s.id}: ${s.title} (${s.status})`, '');
    for (const st of s.steps) lines.push(`- ${Math.round(st.at / 1000)} s: ${st.label}`);
    for (const n of s.notes) lines.push(`- note: ${n}`);
    for (const f of s.shots) lines.push('', `![${f}](${f})`);
    lines.push('');
  }
  if (r.unsigned.length) lines.push('## Words main couldn’t sign', '', ...r.unsigned.map((e) => `- ${e}`), '');
  if (r.pageErrors.length) lines.push('## Page errors', '', ...r.pageErrors.map((e) => `- ${e}`), '');
  lines.push('## Cleanup', '', ...(r.cleanup.length ? r.cleanup.map((c) => `- ${c}`) : ['- nothing to undo']), '');
  lines.push('## Findings', '', '_Judged after the run against Sangam\'s screens and docs/contracts/ (the human-drive skill)._', '');
  return lines.join('\n');
}
