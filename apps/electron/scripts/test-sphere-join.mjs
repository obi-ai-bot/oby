// SPHERE-2 (docs/contracts/obysphere.md), through a real obid: a link the Obysphere refuses joins
// nothing and says why, in the words main's dialog shows ("Oby couldn’t join …"). A fake Obysphere
// on 127.0.0.1 answers each redeem as the spec says a live one does (sphere-api.md section 1 and
// 3.1: used up, expired or withdrawn are one `403 invitation_invalid`; an owner bound to another key
// is `403 forbidden`; and `404`, `429`, which obid tries again a few times as Retry-After says),
// plus an address nobody answers at. The link is in the live
// Obysphere's form (https://obysphere.deskree.com/join#sphere=…&name=…&by=…&id=…&secret=…), here
// on loopback, the one place plain http is allowed. The app's own join (src/shared/sphere.ts, what
// main runs on the owner's Join click) calls obid as main does. Nothing leaves 127.0.0.1.
//
// Run through jiti (`npm run test:sphere-join`), as test:sphere is. The obid is OBY_TEST_OBID, else
// this checkout's core/target/debug/obid, else the installed Oby's.
//
// OBY_TEST_KEEP=1 keeps the run's folder (obid's log).
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { generateKeyPairSync, randomBytes, randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer as http } from 'node:http';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { join as joinSphere, parseInvite } from '../src/shared/sphere.ts';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(appRoot, '../..');
const agents = join(repoRoot, 'core/crates/obid/tests/agents');
const binary = process.env.OBY_TEST_OBID
  || [join(repoRoot, 'core/target/debug/obid'), '/Applications/Oby.app/Contents/Helpers/obid'].find((p) => existsSync(p));
if (!binary) { console.error('test-sphere-join: no obid (OBY_TEST_OBID, core/target/debug/obid or /Applications/Oby.app)'); process.exit(2); }
const run = mkdtempSync(join(tmpdir(), 'oby-sphere-join-'));
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const step = (line) => console.log(`ok  ${line}`);
async function freePort() {
  const s = createServer();
  await new Promise((done) => s.listen(0, '127.0.0.1', done));
  const { port } = s.address();
  await new Promise((done) => s.close(done));
  return port;
}

// The fake Obysphere "deskree": each invitation id answers its redeem one way; it notes every call
const SPHERE = 'deskree';
const refusals = {
  'used-up': [403, 'invitation_invalid', 'the invitation is unknown, used up, expired, or the secret is wrong'],
  expired: [403, 'invitation_invalid', 'the invitation is unknown, used up, expired, or the secret is wrong'],
  withdrawn: [403, 'invitation_invalid', 'the invitation is unknown, used up, expired, or the secret is wrong'],
  'owner-bound': [403, 'forbidden', 'the owner already has another bound key'],
  'rate-limited': [429, 'rate_limited', 'too many redeems from this address; try again later'],
};
const ids = Object.fromEntries(Object.keys(refusals).map((k) => [k, randomUUID()]));
const redeemed = [];
const sphere = http((req, res) => {
  let body = '';
  req.on('data', (d) => { body += d; }).on('end', () => {
    const answer = (status, code, message) => {
      res.writeHead(status, { 'content-type': 'application/json', ...(status === 429 ? { 'retry-after': '1' } : {}) });
      res.end(JSON.stringify({ error: { code, message } }));
    };
    const m = /^\/v0\.1\/spheres\/([^/]+)\/invitations\/([^/]+)\/redeem$/.exec(req.url ?? '');
    if (req.method !== 'POST' || !m) return answer(404, 'not_found', 'no such endpoint');
    redeemed.push({ sphere: m[1], id: m[2], secret: JSON.parse(body || '{}').secret });
    if (m[1] !== SPHERE) return answer(404, 'not_found', 'no such sphere');
    const kind = Object.keys(ids).find((k) => ids[k] === m[2]);
    return kind ? answer(...refusals[kind]) : answer(403, 'invitation_invalid', 'the invitation is unknown, used up, expired, or the secret is wrong');
  });
});
const port = await freePort();
await new Promise((done) => sphere.listen(port, '127.0.0.1', done));
const secret = randomBytes(32).toString('base64url');
const link = (id, { host = `127.0.0.1:${port}`, sphere: s = SPHERE } = {}) =>
  `http://${host}/join#sphere=${s}&name=Deskree&by=dmytro&id=${id}&secret=${secret}`;

// A real obid with a home of its own and a stand-in agent, called as main calls it
const root = join(run, 'obid'), home = join(root, 'home');
for (const d of ['home/accounts/stub', 'env-home', 'tmp']) mkdirSync(join(root, d), { recursive: true, mode: 0o700 });
const obidPort = await freePort(), token = randomBytes(32).toString('hex');
writeFileSync(join(home, 'api-token'), `${token}\n`, { mode: 0o600 });
writeFileSync(join(home, 'accounts/stub/.credentials.json'), JSON.stringify({ claudeAiOauth: { accessToken: 'fake', expiresAt: Date.now() + 86_400_000 } }), { mode: 0o600 });
writeFileSync(join(root, 'config.json'), JSON.stringify({
  listen: `127.0.0.1:${obidPort}`, home, owner: 'wellington', claudeKeychain: false, cursorKeychain: false,
  agents: { claude: { bin: join(agents, 'fake-claude'), accounts: [{ id: 'stub', name: 'Stub', configDir: join(home, 'accounts/stub') }] } },
}));
const out = [];
const obid = spawn(binary, ['serve', '--config', join(root, 'config.json')], {
  env: { PATH: `${agents}:${process.env.PATH}`, HOME: join(root, 'env-home'), TMPDIR: join(root, 'tmp'), OBID_MEMORY_AVAILABLE: '1099511627776' },
  stdio: ['ignore', 'pipe', 'pipe'], detached: true,
});
obid.stdout.on('data', (d) => out.push(d));
obid.stderr.on('data', (d) => out.push(d));
const call = async (method, path, body) => {
  try {
    const r = await fetch(`http://127.0.0.1:${obidPort}${path}`, {
      method, headers: { authorization: `Bearer ${token}`, ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    return { status: r.status, text: await r.text() };
  } catch (e) { return { status: 0, text: e.message }; }
};

let failed = false;
try {
  for (let i = 0; (await call('GET', '/v1/status')).status !== 200; i++) {
    if (i > 300 || obid.exitCode !== null) throw new Error(`obid didn't answer:\n${Buffer.concat(out).toString().slice(-2000)}`);
    await sleep(100);
  }
  const key = generateKeyPairSync('ed25519').privateKey;
  const owner = () => key;
  // What main's dialog says ("Oby couldn’t join Deskree." and this), and that nothing joined
  const refused = async (url, words) => {
    const invite = parseInvite(url);
    assert.ok(invite, `parseInvite took ${url.replace(/secret=.*/, 'secret=…')}`);
    await assert.rejects(joinSphere(call, owner, invite), (e) => { assert.match(e.message, words); return true; });
    const now = JSON.parse((await call('GET', '/v1/obid/sphere')).text);
    assert.notEqual(now.state, 'joined');
  };

  // The live Obysphere's link, as Dmytro's admin makes it, and the join page's Open in Oby, read
  const live = parseInvite(`https://obysphere.deskree.com/join#sphere=${SPHERE}&name=Deskree%20team&by=dmytro&id=${ids['used-up']}&secret=${secret}`);
  assert.deepEqual({ ...live }, { url: 'https://obysphere.deskree.com', host: 'obysphere.deskree.com', sphere: SPHERE, name: 'Deskree team', by: 'dmytro', owner: undefined, obi: undefined, domain: undefined, id: ids['used-up'], secret });
  assert.equal(parseInvite(`oby://join/obysphere.deskree.com#sphere=${SPHERE}&name=Deskree%20team&by=dmytro&id=${ids['used-up']}&secret=${secret}`)?.url, 'https://obysphere.deskree.com');
  step('sphere_2: the live Obysphere’s link and its oby:// form read as its join page makes them');

  const gone = /This link has expired, was used up or was cancelled\. Ask whoever sent it for a new one\./;
  for (const kind of ['used-up', 'expired', 'withdrawn']) await refused(link(ids[kind]), gone);
  step('sphere_2: a link used up, expired or withdrawn (403 invitation_invalid) says so, in plain words');

  await refused(link(ids['owner-bound']), /Deskree already has wellington with another Mac’s key/);
  step('sphere_2: an owner the Obysphere bound to another key (403 forbidden) is told to join from that Mac');

  await refused(link(ids['rate-limited']), /^The Sphere refused it: rate_limited: too many redeems/);
  await refused(link(randomUUID(), { sphere: 'not-there' }), /^The Sphere refused it: not_found: no such sphere/);
  step('sphere_2: any other refusal (429, 404) is obid’s quote of the Obysphere, its code first');

  const nobody = await freePort();
  await refused(link(ids['used-up'], { host: `127.0.0.1:${nobody}` }), /^The Sphere can’t be reached|^The Sphere can't be reached/);
  step('sphere_2: an Obysphere nobody answers at says it can’t be reached');

  // each redeem reached the Obysphere as the link said, its secret unchanged, and none joined (a
  // 429 is sent again: obi-protocol's client, RATE_LIMIT_TRIES)
  for (const kind of Object.keys(ids)) assert.ok(redeemed.some((r) => r.id === ids[kind]), kind);
  assert.ok(redeemed.filter((r) => r.id === ids['rate-limited']).length > 1, 'a 429 is tried again');
  assert.ok(redeemed.every((r) => r.secret === secret));
  assert.equal(JSON.parse((await call('GET', '/v1/obid/sphere')).text).state, 'none');
  step('sphere_2: every refused link joined nothing');
  console.log('\nSPHERE-2 (the Obysphere refuses) passed against a fake Obysphere.');
} catch (e) {
  failed = true;
  console.error(`FAIL ${e.stack ?? e}`);
  console.error(`--- obid\n${Buffer.concat(out).toString().slice(-3000)}`);
} finally {
  try { process.kill(-obid.pid, 'SIGTERM'); } catch { /* gone */ }
  sphere.close();
  await sleep(300);
  if (process.env.OBY_TEST_KEEP === '1') console.log(`kept ${run}`); else rmSync(run, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
