// The human drive's own device with obid, run by human-drive.mjs as a one-shot Electron app (only
// Electron's safeStorage opens the key): `register` adds it, the way main adds a new key on the
// owner's click (home/pairing, then POST /v1/devices), and `revoke` forgets it with its own signature
// (DELETE /v1/devices/{id}). Apart from the driven app, so a revoke still happens once that app is
// closed (Playwright closes it on Control-C). Prints `DEVICE <json>`: {id} or {id, revoked} or {error}.
const { app, safeStorage } = require('electron');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const [action, keyPath, home, name] = process.argv.slice(-4);
app.dock?.hide();
app.setPath('userData', path.join(path.dirname(keyPath), 'device-helper'));

async function run() {
  let key;
  if (fs.existsSync(keyPath)) key = crypto.createPrivateKey(safeStorage.decryptString(fs.readFileSync(keyPath)));
  else if (action === 'register') {
    // as main/device.ts keeps it: PKCS#8 PEM, sealed by safeStorage, 0600, never over one that's there
    key = crypto.generateKeyPairSync('ec', { namedCurve: 'prime256v1' }).privateKey;
    fs.writeFileSync(keyPath, safeStorage.encryptString(key.export({ type: 'pkcs8', format: 'pem' })), { flag: 'wx', mode: 0o600 });
  } else return { error: 'no device key' };
  const { x, y } = crypto.createPublicKey(key).export({ format: 'jwk' });
  const publicKey = Buffer.concat([Buffer.from([4]), Buffer.from(x, 'base64url'), Buffer.from(y, 'base64url')]).toString('base64');
  const token = fs.readFileSync(path.join(home, 'api-token'), 'utf8').trim();
  const call = (method, url, body) => fetch(`http://127.0.0.1:7777${url}`, { method, headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  let r = await call('POST', '/v1/devices', { name, publicKey });
  if (r.status === 403 && action === 'register') {
    const pairing = path.join(home, 'pairing');
    try { fs.writeFileSync(pairing, publicKey, { mode: 0o600 }); r = await call('POST', '/v1/devices', { name, publicKey }); } finally { fs.rmSync(pairing, { force: true }); }
  }
  if (r.status === 403) return { revoked: true }; // not registered: nothing to revoke
  if (!r.ok) return { error: `${r.status} ${await r.text()}` };
  const { id } = await r.json();
  if (action === 'register') return { id };
  const signedAt = new Date().toISOString().replace(/\.\d+Z$/, 'Z');
  const signature = crypto.sign('sha256', Buffer.from(`oby-revoke-device\n${id}\n${signedAt}`), key).toString('base64');
  const d = await call('DELETE', `/v1/devices/${id}`, { approval: { device: id, signedAt, signature } });
  return d.ok ? { id, revoked: true } : { id, error: `${d.status} ${await d.text()}` };
}

app.whenReady().then(async () => {
  let out;
  try { out = await run(); } catch (e) { out = { error: e.message }; }
  process.stdout.write(`\nDEVICE ${JSON.stringify(out)}\n`);
  app.exit(0);
});
