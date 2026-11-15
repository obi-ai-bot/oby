#!/usr/bin/env node
// Signs latest-mac.yml for the updater (docs/contracts/updates.md, "Signed by Deskree"): its last
// line becomes `signature: '<base64>'`, ed25519 over every byte before it, which the app checks
// with the public key it carries (FEED_KEY in src/shared/updates.ts, verifyFeed). The private key
// is the file OBY_FEED_KEY_FILE names, outside the app and its repository; nothing here prints,
// copies or passes it on.
// Usage: node scripts/sign-feed.mjs                    checks the key only (package.sh, before building)
//        node scripts/sign-feed.mjs <unsigned> <out>   writes <out>: <unsigned>, then its signature
import { spawnSync } from 'node:child_process';
import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const app = join(dirname(fileURLToPath(import.meta.url)), '..');
const die = (why) => {
  console.error(`error: ${why}`);
  process.exit(1);
};
const args = process.argv.slice(2);
if (args.length !== 0 && args.length !== 2) die('usage: node scripts/sign-feed.mjs [<unsigned> <out>]');

// file inside dir, or dir itself
const inside = (dir, file) => {
  const r = relative(dir, file);
  return r === '' || (!isAbsolute(r) && r.split(sep)[0] !== '..');
};

const named = process.env.OBY_FEED_KEY_FILE;
if (!named) die('OBY_FEED_KEY_FILE names no file: set it to the feed key\'s file, outside the repository');
let file;
try {
  file = realpathSync(named);
} catch {
  die('the file OBY_FEED_KEY_FILE names isn\'t there');
}
const own = realpathSync(app);
const git = spawnSync('git', ['-C', own, 'rev-parse', '--show-toplevel'], { encoding: 'utf8' });
const tree = git.status === 0 && git.stdout.trim() ? realpathSync(git.stdout.trim()) : null;
if (inside(own, file) || (tree && inside(tree, file))) die('the feed key is inside the repository: keep it outside, where nothing scans or commits it');
let key;
try {
  key = createPrivateKey(readFileSync(file));
} catch {
  die('the file OBY_FEED_KEY_FILE names isn\'t a readable private key');
}
if (key.asymmetricKeyType !== 'ed25519') die('the feed key isn\'t an ed25519 private key');

// the one FEED_KEY line the app carries
const source = readFileSync(join(app, 'src/shared/updates.ts'), 'utf8');
const lines = [...source.matchAll(/^export const FEED_KEY = '([A-Za-z0-9+/=]+)';$/gm)];
if (lines.length !== 1) die('src/shared/updates.ts doesn\'t carry exactly one FEED_KEY line');
const carried = Buffer.from(lines[0][1], 'base64');
if (!createPublicKey(key).export({ type: 'spki', format: 'der' }).equals(carried)) {
  die('the feed key doesn\'t match the key the app carries (FEED_KEY in src/shared/updates.ts)');
}
if (args.length === 0) {
  console.log('==> The feed key matches the app\'s');
  process.exit(0);
}

const [from, to] = args;
let unsigned;
try {
  unsigned = readFileSync(from);
} catch {
  die(`can't read ${from}`);
}
if (unsigned.length === 0 || unsigned[unsigned.length - 1] !== 0x0a) die(`${from} doesn't end in a new line`);
if (unsigned.toString('utf8').split('\n').some((l) => l.startsWith('signature:'))) die(`${from} is signed already`);
const signature = sign(null, unsigned, key);
const pinned = createPublicKey({ key: carried, format: 'der', type: 'spki' });
if (!verify(null, unsigned, pinned, signature)) die('the signature doesn\'t verify with the key the app carries');
writeFileSync(to, Buffer.concat([unsigned, Buffer.from(`signature: '${signature.toString('base64')}'\n`)]));
console.log(`==> ${to} signed`);
