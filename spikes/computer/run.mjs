// Spike: one own-lane Claude Code job on a scratch obid built from PR #2088, with agent-desktop
// in the owner's user-scope MCP config. Read-only: status, a snapshot and a screenshot of Calculator.
// Usage: node run.mjs <pr-tree> <obid-binary> <real-claude> <root>
import { spawn } from 'node:child_process';
import { chmodSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { join } from 'node:path';

const [tree, binary, claude, root] = process.argv.slice(2);
const { buildFixture } = await import(join(tree, 'e2e/lib/fixture.mjs'));
const home = join(root, 'home'), logs = join(root, 'logs');
for (const dir of ['home', 'tmp', 'logs']) mkdirSync(join(root, dir), { recursive: true });
const fixture = buildFixture(join(root, 'fixture'));

// The wrapper keeps what obid started Claude Code with and what the two said: argv, the names
// of the environment's keys (never their values), and the stream both ways.
const wrapper = join(root, 'claude-wrapper');
writeFileSync(wrapper, `#!/bin/bash
L="${logs}/claude-$$"
printf '%s\\n' "$@" > "$L.argv"
{ echo "HOME=$HOME"; echo "CLAUDE_CONFIG_DIR=\${CLAUDE_CONFIG_DIR-<unset>}"; echo "HTTPS_PROXY=\${HTTPS_PROXY:+set}"; echo "keys: $(env | cut -d= -f1 | sort | tr '\\n' ' ')"; } > "$L.env"
exec "${claude}" "$@" < <(tee "$L.in") > >(tee "$L.out")
`);
chmodSync(wrapper, 0o755);

const port = await new Promise((done) => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => done(p)); }); });
writeFileSync(join(root, 'config.json'), JSON.stringify({
  listen: `127.0.0.1:${port}`, home, owner: 'spike', telemetry: false, allowFileRemotes: true, cursorKeychain: false,
  agents: { claude: { bin: wrapper, accounts: [{ id: 'default', name: 'Default', configDir: null }] } },
  repos: [{ repo: 'obi-e2e/fixture-app', remote: fixture.remoteUrl, defaultBranch: 'main', check: 'node --test', pullRequests: false }],
}), { mode: 0o600 });

const out = [];
const daemon = spawn(binary, ['serve', '--config', join(root, 'config.json')], {
  env: { PATH: process.env.PATH, HOME: process.env.HOME, USER: process.env.USER, LOGNAME: process.env.USER, TMPDIR: join(root, 'tmp'), OBID_PRESTART: 'off', OBID_USAGE_READS: 'off', OBID_LOAD: '0', OBID_MEMORY_AVAILABLE: '1099511627776' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
daemon.stdout.on('data', (b) => out.push(b.toString()));
daemon.stderr.on('data', (b) => out.push(b.toString()));
const call = async (method, path, body) => {
  const token = readFileSync(join(home, 'api-token'), 'utf8').trim();
  const r = await fetch(`http://127.0.0.1:${port}${path}`, { method, headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, ...(body ? { body: JSON.stringify(body) } : {}) });
  const text = await r.text();
  if (!r.ok) throw new Error(`${method} ${path}: ${r.status}: ${text}`);
  return text ? JSON.parse(text) : null;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const stop = () => { try { daemon.kill('SIGTERM'); } catch {} };
process.on('exit', stop);

try {
  for (let i = 0; ; i++) { try { await call('GET', '/v1/status'); break; } catch (e) { if (i > 100) throw e; await sleep(200); } }
  const status = await call('GET', '/v1/status');
  console.log('accounts:', JSON.stringify(status.accounts?.map((a) => ({ id: a.id, tool: a.tool, signedIn: a.signedIn, state: a.state, problem: a.problem }))));
  const task = [
    'This is a read-only check of a desktop driver. Do exactly these steps and nothing else, and change no file.',
    '1. List the name of every MCP server you have tools from.',
    '2. Call the agent-desktop MCP server\'s status tool (desktop_status).',
    '3. Call its desktop_snapshot tool for the app "Calculator" with interactive_only true and compact true.',
    '4. Call its desktop_screenshot tool for the app "Calculator".',
    'Never click, type, press a key, launch, close, move or change anything, and call no other agent-desktop tool.',
    'In your summary say, for each step: whether the tool existed, whether it ran or was refused, the exact words of any refusal or error, what desktop_status reported for permissions, how many refs the snapshot had, and whether you could see the screenshot as an image.',
  ].join('\n');
  const job = await call('POST', '/v1/jobs', { job: { repo: 'obi-e2e/fixture-app', kind: 'research', title: 'Spike: read Calculator through agent-desktop', task } });
  console.log('job', job.id);
  let j;
  for (let i = 0; i < 300; i++) {
    j = await call('GET', `/v1/jobs/${job.id}`);
    if (['done', 'failed', 'stopped', 'interrupted'].includes(j.status) && !j.permission) break;
    if (j.permission && i % 10 === 0) console.log('waiting on a permission:', JSON.stringify(j.permission).slice(0, 400));
    await sleep(2000);
  }
  const pick = (o, keys) => Object.fromEntries(keys.filter((k) => k in o).map((k) => [k, o[k]]));
  console.log('JOB', JSON.stringify(pick(j, ['status', 'lane', 'mode', 'step', 'reached', 'untrusted', 'foreign', 'unsigned', 'vetted', 'changes', 'error', 'resumeCommand', 'activity', 'outgoing', 'artifactProblems', 'permission']), null, 1));
  console.log('SUMMARY\n' + (j.summary ?? ''));
  try {
    const t = await call('GET', `/v1/jobs/${job.id}/transcript`);
    const entries = t.entries ?? t.transcript ?? t;
    for (const e of entries) if (e.kind === 'tool' || e.tool) console.log('TOOL', JSON.stringify(pick(e, ['tool', 'state', 'summary', 'activity', 'seconds'])));
  } catch (e) { console.log('transcript:', e.message); }
} catch (e) {
  console.log('FAILED', e.message);
} finally {
  writeFileSync(join(logs, 'obid.log'), out.join(''));
  stop();
}
