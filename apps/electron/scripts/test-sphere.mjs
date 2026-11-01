// SPHERE-5 (docs/contracts/obysphere.md): two Obys on one Obysphere, joined by private links with
// the Electron app's own join (src/shared/sphere.ts, what main runs on the owner's Join click).
// A local Obysphere from an obi-sphere checkout plays Dmytro's: its admin creates it, writes a
// policy and makes an open link and a named one, with the spec's own calls. Two real obids (stand-in
// agents, homes of their own) join, see each other, one asks the other a question across it, and
// one leaves. Nothing here reaches a network beyond 127.0.0.1.
//
//   OBI_SPHERE_DIR=<obi-sphere checkout, `pnpm install` run in server/> \
//   OBY_TEST_OBID=<obid binary> npm run test:sphere
//
// OBY_TEST_KEEP=1 keeps the run's folder (its logs).
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { generateKeyPairSync, randomBytes } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertion, join as joinSphere, keepMembership, leave, parseInvite, publicOf } from '../src/shared/sphere.ts';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(appRoot, '../..');
const agents = join(repoRoot, 'core/crates/obid/tests/agents');
const binary = process.env.OBY_TEST_OBID || join(repoRoot, 'core/target/debug/obid');
const sphereDir = process.env.OBI_SPHERE_DIR;
if (!sphereDir) { console.error('test-sphere: OBI_SPHERE_DIR names an obi-sphere checkout'); process.exit(2); }
const run = mkdtempSync(join(tmpdir(), 'oby-sphere-'));
const children = [];
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const step = (line) => console.log(`ok  ${line}`);

async function until(label, f, ms = 60_000) {
  const deadline = Date.now() + ms;
  let last;
  while (Date.now() < deadline) {
    try { const v = await f(); if (v) return v; } catch (e) { last = e; }
    await sleep(200);
  }
  throw new Error(`${label} timed out${last ? `: ${last.message}` : ''}`);
}
async function freePort() {
  const s = createServer();
  await new Promise((done) => s.listen(0, '127.0.0.1', done));
  const { port } = s.address();
  await new Promise((done) => s.close(done));
  return port;
}
function start(name, cmd, args, opts) {
  const log = join(run, `${name}.log`);
  const child = spawn(cmd, args, { ...opts, stdio: ['ignore', 'pipe', 'pipe'], detached: true });
  const out = [];
  child.stdout.on('data', (d) => out.push(d));
  child.stderr.on('data', (d) => out.push(d));
  child.on('exit', () => writeFileSync(log, Buffer.concat(out)));
  child.log = () => Buffer.concat(out).toString();
  children.push(child);
  return child;
}

// The Obysphere, as its admin calls it
const sphere = { port: await freePort(), token: randomBytes(32).toString('base64url') };
sphere.url = `http://127.0.0.1:${sphere.port}`;
async function api(path, { method = 'GET', token, body, form } = {}) {
  const r = await fetch(`${sphere.url}/v0.1${path}`, {
    method, redirect: 'error',
    headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(body ? { 'content-type': 'application/json' } : form ? { 'content-type': 'application/x-www-form-urlencoded' } : {}) },
    body: body ? JSON.stringify(body) : form ? new URLSearchParams(form) : undefined,
  });
  const text = await r.text();
  return { status: r.status, body: text ? JSON.parse(text) : null };
}

// An obid, and calls to it as main makes them (main/obid.ts: its token from home/api-token)
async function obid(name, owner) {
  const root = join(run, name), home = join(root, 'home');
  for (const d of ['home/accounts/stub', 'env-home', 'tmp']) mkdirSync(join(root, d), { recursive: true, mode: 0o700 });
  const port = await freePort(), token = randomBytes(32).toString('hex');
  writeFileSync(join(home, 'api-token'), `${token}\n`, { mode: 0o600 });
  writeFileSync(join(home, 'accounts/stub/.credentials.json'), JSON.stringify({ claudeAiOauth: { accessToken: 'fake', expiresAt: Date.now() + 86_400_000 } }), { mode: 0o600 });
  writeFileSync(join(root, 'config.json'), JSON.stringify({
    listen: `127.0.0.1:${port}`, home, owner, telemetry: false, claudeKeychain: false, cursorKeychain: false,
    agents: { claude: { bin: join(agents, 'fake-claude'), accounts: [{ id: 'stub', name: 'Stub', configDir: join(home, 'accounts/stub') }] } },
  }));
  const process_ = start(name, binary, ['serve', '--config', join(root, 'config.json')], {
    env: { PATH: `${agents}:${process.env.PATH}`, HOME: join(root, 'env-home'), TMPDIR: join(root, 'tmp'), OBID_MEMORY_AVAILABLE: '1099511627776' },
  });
  const call = async (method, path, body) => {
    try {
      const r = await fetch(`http://127.0.0.1:${port}${path}`, {
        method, headers: { authorization: `Bearer ${token}`, ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      return { status: r.status, text: await r.text() };
    } catch (e) { return { status: 0, text: e.message }; }
  };
  await until(`${name} answering`, async () => process_.exitCode === null && (await call('GET', '/v1/status')).status === 200);
  const get = async (path) => { const r = await call('GET', path); assert.equal(r.status, 200, `${name} ${path}: ${r.text}`); return JSON.parse(r.text); };
  return { name, owner, call, get, key: generateKeyPairSync('ed25519').privateKey };
}

let failed = false;
try {
  // 1. The Obysphere "demo", Dmytro its admin, a policy that lets anyone ask anyone a question,
  //    an open link for the team and a named one for Ernesto
  start('sphere', process.execPath, ['scripts/local.mjs', '--port', String(sphere.port), '--persist-to', join(run, 'sphere-state')], {
    cwd: join(sphereDir, 'server'),
    env: { SPHERE_OPERATOR_TOKEN: sphere.token, HOME: run, TMPDIR: tmpdir(), PATH: process.env.PATH, WRANGLER_SEND_METRICS: 'false' },
  });
  await until('the Obysphere answering', async () => (await api('/health')).status === 200, 90_000);
  const admin = generateKeyPairSync('ed25519').privateKey;
  let r = await api('/spheres', { method: 'POST', token: sphere.token, body: { sphere: 'demo', name: 'Demo team', admin: { owner: 'dmytro', owner_key: publicOf(admin) } } });
  assert.equal(r.status, 201, JSON.stringify(r.body));
  r = await api('/oauth/token', { method: 'POST', form: {
    grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange', subject_token_type: 'urn:ietf:params:oauth:token-type:jwt',
    subject_token: assertion(admin, 'dmytro', 'demo', 'admin directory:read audit:read'),
  } });
  assert.equal(r.status, 200, JSON.stringify(r.body));
  const adminToken = r.body.access_token;
  r = await api('/spheres/demo/policy', { method: 'PUT', token: adminToken, body: { expected_version: 0, rules: [{ id: 'everyone', from_owner: '*', to_owner: '*', actions: ['question', 'review_request'], repos: ['*'] }] } });
  assert.equal(r.status, 200, JSON.stringify(r.body));
  const link = async (body, extra = '') => {
    const i = await api('/spheres/demo/invitations', { method: 'POST', token: adminToken, body: { expires_in: 3600, ...body } });
    assert.equal(i.status, 201, JSON.stringify(i.body));
    return `${sphere.url}/join#sphere=demo&name=Demo%20team&by=dmytro${extra}&id=${i.body.invitation_id}&secret=${i.body.secret}`;
  };
  const open = await link({ max_uses: 5 });
  const named = await link({ owner: 'ernesto', obi: 'obi-ernesto' }, '&for=ernesto&obi=obi-ernesto');
  step('SPHERE-5: the Obysphere is up, with an open link and a named one');

  // 2. Two Obys join, each by its link, through the app's own join
  const a = await obid('obid-a', 'wellington'), b = await obid('obid-b', 'ernesto');
  const ja = await joinSphere(a.call, () => a.key, parseInvite(open));
  const jb = await joinSphere(b.call, () => b.key, parseInvite(named));
  assert.equal(ja.state, 'joined');
  assert.match(ja.obiId, /^obi-[0-9a-f-]{36}$/);
  assert.equal(jb.state, 'joined');
  assert.equal(jb.obiId, 'obi-ernesto');
  step(`SPHERE-5: both joined (${ja.obiId}, ${jb.obiId})`);

  // 3. Each sees the other in its side panel's members (GET /v1/obid/sphere/members), sealed
  const seen = async (o, other) => until(`${o.name} seeing ${other}`, async () => (await o.get('/v1/obid/sphere/members')).find((m) => m.owner === other));
  const ma = await seen(a, 'ernesto'), mb = await seen(b, 'wellington');
  assert.equal(ma.obi, 'obi-ernesto');
  assert.equal(mb.obi, ja.obiId);
  step('SPHERE-5: each sees the other');

  // 3b. SPHERE-6: what main does every 5 minutes signs the card with obid's seal key once both
  //     can read one, so the words between them are sealed end to end
  for (const o of [a, b]) {
    await until(`${o.name} sealing`, async () => {
      await o.get('/v1/obid/sphere/members');
      await keepMembership(o.call, () => o.key);
      return (await o.get('/v1/obid/sphere')).sealing === 'on';
    });
  }
  const sealedFor = async (o, other) => until(`${o.name} sealing for ${other}`, async () => (await o.get('/v1/obid/sphere/members')).find((m) => m.owner === other && m.sealed));
  await sealedFor(a, 'ernesto');
  await sealedFor(b, 'wellington');
  step('SPHERE-6: both cards carry their seal keys, and each seals for the other');

  // 4. Wellington's Oby asks Ernesto's a question across the Obysphere; it arrives as a typed request
  r = await a.call('POST', '/v1/obid/sphere/requests', { to: 'obi-ernesto', type: 'question', body: { text: 'Which branch is the demo on?' }, expiresIn: 3600 });
  assert.equal(r.status, 202, r.text);
  const cid = JSON.parse(r.text).correlationId;
  const got = await until('the question arriving', async () => (await b.get('/v1/obid/sphere/exchanges')).find((e) => e.correlationId === cid));
  assert.equal(got.direction, 'incoming');
  assert.equal(got.type, 'question');
  assert.equal(got.peer, ja.obiId);
  step(`SPHERE-5: a question crossed it (${got.state} on Ernesto's side)`);

  // 5. Wellington's Oby leaves: the Obysphere takes it out, and obid forgets it
  const left = await leave(a.call, () => a.key);
  assert.equal(left.untold, null);
  assert.equal(left.membership.state, 'none');
  r = await api(`/spheres/demo/members/${ja.obiId}`, { token: adminToken });
  assert.equal(r.body?.status, 'revoked', JSON.stringify(r.body));
  step('SPHERE-5: one left, and the Obysphere lists it revoked');
  console.log('\nSPHERE-5 passed.');
} catch (e) {
  failed = true;
  console.error(`FAIL ${e.stack ?? e}`);
  for (const c of children) if (c.log) console.error(`--- ${c.spawnargs[1] ?? c.spawnfile}\n${c.log().slice(-3000)}`);
} finally {
  for (const c of children) { try { process.kill(-c.pid, 'SIGTERM'); } catch { /* gone */ } }
  await sleep(500);
  if (process.env.OBY_TEST_KEEP === '1') console.log(`kept ${run}`); else rmSync(run, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
