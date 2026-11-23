import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, describe, expect, it, vi } from 'vitest';
import type { Gitleaks } from '../../../main/help';
import { PICTURE_SECRET, PICTURE_UNCHECKED, setPicture } from '../../../shared/sphere';

// SPHEREAPP-18 (docs/contracts/sphere-app.md): the text drawn in an Obysphere picture goes through
// the secret scan with its bytes before it leaves. Main's pictureText.ts runs for real, with
// Electron stood in for; its reader is a stand-in, then the real oby-picture-text (Vision) and
// gitleaks where this Mac has them.
const the = vi.hoisted(() => ({ packaged: false, exe: '/nonexistent/Oby.app/Contents/MacOS/Oby', appPath: '/nonexistent/apps/electron' }));
vi.mock('electron', () => ({
  app: { get isPackaged() { return the.packaged; }, getPath: () => the.exe, getAppPath: () => the.appPath, getVersion: () => '0.0.0' },
  clipboard: {}, shell: {}, dialog: {}, ipcMain: { handle() {}, on() {} },
}));
vi.mock('../../../main/obid', () => ({ obid: async () => ({ status: 0, text: '' }) }));

const { PICTURE_TEXT, pictureTextBin, readings, runPictureText, scanPicture, textToScan } = await import('../../../main/pictureText');
const folder = mkdtempSync(join(tmpdir(), 'oby-picture-text-test-'));
afterAll(() => rmSync(folder, { recursive: true, force: true }));

// a made-up GitHub token, built here so this file holds none
const token = () => `ghp_${[...randomBytes(36)].map((b) => 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'[b % 62]).join('')}`;
// gitleaks as it behaves on GitHub tokens: each line holding one is a finding (exit 99), none exit 0
const fakeGitleaks: Gitleaks = async (input, report) => {
  const found = readFileSync(input, 'utf8').split('\n').flatMap((line, i) => (/ghp_[A-Za-z0-9]{36}/.test(line) ? [{ StartLine: i + 1, RuleID: 'github-pat' }] : []));
  writeFileSync(report, JSON.stringify(found));
  return found.length ? 99 : 0;
};
// 256 px pictures as main sends them: a token drawn in pixels (made up, drawn in two lines under
// GITHUB_TOKEN=; its bytes hold no text of it), and a logo that says "Acme"
const TOKEN_PNG = new Uint8Array(readFileSync(join(__dirname, 'fixtures/picture-token.png')));
const LOGO_PNG = new Uint8Array(readFileSync(join(__dirname, 'fixtures/picture-logo.png')));
// what oby-picture-text prints: a JSON array of readings, best first, for each line it read
const printed = (...lines: string[][]) => lines.map((l) => JSON.stringify(l)).join('\n') + '\n';
// a stand-in helper: a shell script
function helper(name: string, body: string) {
  const bin = join(folder, name);
  writeFileSync(bin, `#!/bin/sh\n${body}\n`);
  chmodSync(bin, 0o755);
  return bin;
}

describe('SPHEREAPP-18: the text in a picture is scanned before it leaves', () => {
  it('sphereapp_18_a_picture_whose_text_reads_as_a_secret_is_refused_and_a_clean_logo_goes', async () => {
    const t = token();
    // drawn over two lines, and read with a mark for a letter (`]` for `j`): still found
    for (const read of [printed(['GITHUB_TOKEN='], [t]), printed(['GITHUB_TOKEN='], [t.slice(0, 20)], [t.slice(20)]), printed([`${t.slice(0, 10)}]${t.slice(11)}`])]) {
      expect(await scanPicture(TOKEN_PNG, { read: async () => read, gitleaks: fakeGitleaks })).toEqual({ kind: 'found', rules: ['github-pat'] });
    }
    // the bytes alone hold no text of it: the scan of the bytes would let it go
    expect(await scanPicture(TOKEN_PNG, { read: async () => '', gitleaks: fakeGitleaks })).toEqual({ kind: 'clean' });
    expect(await scanPicture(LOGO_PNG, { read: async () => printed(['Acme', 'Acne']), gitleaks: fakeGitleaks })).toEqual({ kind: 'clean' });
    // through setPicture: refused with its words, and nothing reaches obid or the Sphere
    const call = vi.fn(), http = vi.fn();
    const ownerKey = () => { throw new Error('no key is used'); };
    expect(await setPicture(call, ownerKey, TOKEN_PNG, (png) => scanPicture(png, { read: async () => printed(['GITHUB_TOKEN='], [t]), gitleaks: fakeGitleaks }), http))
      .toMatchObject({ refused: 'failed', why: PICTURE_SECRET, image: null });
    expect(call).not.toHaveBeenCalled();
    expect(http).not.toHaveBeenCalled();
  });

  it('sphereapp_18: a token drawn over two lines is found whichever readings Vision gives of each half', async () => {
    const t = token(), [one, two] = [t.slice(0, 20), t.slice(20)];
    // three readings of each half, the right one second of the first and last of the second; the
    // others break the token (a mark no token holds), so only the right pair makes it
    const first = [`${one.slice(0, 8)}-${one.slice(9)}`, one, `${one.slice(0, 12)}.${one.slice(13)}`];
    const second = [`${two.slice(0, 5)}-${two.slice(6)}`, `${two.slice(0, 10)}.${two.slice(11)}`, two];
    for (const lines of [[first, second], [['GITHUB_TOKEN=', 'GITHUB-TOKEN='], first, second, ['done']]]) {
      expect(await scanPicture(TOKEN_PNG, { read: async () => printed(...lines), gitleaks: fakeGitleaks })).toEqual({ kind: 'found', rules: ['github-pat'] });
    }
    // with only wrong readings of a half, nothing is made up
    expect(await scanPicture(TOKEN_PNG, { read: async () => printed([first[0], first[2]], second), gitleaks: fakeGitleaks })).toEqual({ kind: 'clean' });
  });

  it('sphereapp_18: a `j` read as `]` in a prefix a rule needs is still found', async () => {
    // a scanner whose rule needs a fixed prefix with a `j` in it (made up: `pj_` and 30 letters or digits)
    const suffix = token().slice(4, 34);
    const needsJ: Gitleaks = async (input, report) => {
      const found = readFileSync(input, 'utf8').split('\n').flatMap((line, i) => (/pj_[A-Za-z0-9]{30}/.test(line) || /ghp_[A-Za-z0-9]{36}/.test(line) ? [{ StartLine: i + 1, RuleID: 'made-up' }] : []));
      writeFileSync(report, JSON.stringify(found));
      return found.length ? 99 : 0;
    };
    // the prefix's `j` read as `]`, alone, beside a `|` read for an `l` in the rest, and over two lines
    for (const lines of [[[`p]_${suffix}`]], [[`p]_${suffix.slice(0, 9)}|${suffix.slice(10)}`]], [['p]_'], [suffix]]]) {
      expect(await scanPicture(LOGO_PNG, { read: async () => printed(...lines), gitleaks: needsJ })).toEqual({ kind: 'found', rules: ['made-up'] });
    }
  });

  it('sphereapp_18: a text reader that isn’t there, fails or takes too long sends nothing', async () => {
    const couldNotRun = expect.objectContaining({ kind: 'couldNotRun' });
    expect(await scanPicture(LOGO_PNG, { read: null, gitleaks: fakeGitleaks })).toEqual({ kind: 'couldNotRun', why: 'the picture’s text reader isn’t there' });
    expect(await scanPicture(LOGO_PNG, { read: runPictureText(helper('fails', 'echo nope >&2; exit 70')), gitleaks: fakeGitleaks })).toEqual(couldNotRun);
    const slow = await scanPicture(LOGO_PNG, { read: runPictureText(helper('slow', 'exec sleep 5'), 200), gitleaks: fakeGitleaks });
    expect(slow).toEqual({ kind: 'couldNotRun', why: 'the picture’s text couldn’t be read: it took over 0.2 s' });
    // read, but the scan can't run: nothing goes either
    expect(await scanPicture(LOGO_PNG, { read: async () => printed(['Acme']), gitleaks: null })).toEqual(couldNotRun);
    // a helper that prints something else than readings: nothing goes
    for (const said of ['Acme\n', '{"a":1}\n', '[]\n', '[1]\n']) {
      expect(await scanPicture(LOGO_PNG, { read: async () => said, gitleaks: fakeGitleaks })).toEqual(couldNotRun);
    }
    // and setPicture says so, sending nothing
    const http = vi.fn();
    expect(await setPicture(vi.fn(), () => { throw new Error('no key'); }, LOGO_PNG, (png) => scanPicture(png, { read: null }), http)).toMatchObject({ refused: 'failed', why: PICTURE_UNCHECKED });
    expect(http).not.toHaveBeenCalled();
  });

  it('sphereapp_18: the reader gets the exact outgoing bytes in a private file, removed after', async () => {
    const copy = join(folder, 'seen.png'), seen = join(folder, 'seen');
    const bin = helper('records', `cp "$1" ${copy}; ls -l "$1" | cut -c1-10; ls -ld "$(dirname "$1")" | cut -c1-10; echo "$1" > ${seen}`);
    expect((await runPictureText(bin)(TOKEN_PNG)).trim().split('\n')).toEqual(['-rw-------', 'drwx------']);
    expect(new Uint8Array(readFileSync(copy))).toEqual(TOKEN_PNG);
    expect(existsSync(readFileSync(seen, 'utf8').trim())).toBe(false);
  });

  it('sphereapp_18: main runs the helper the app carries, or the one helpers.sh staged, and no other', () => {
    const app = join(folder, 'Oby.app/Contents/MacOS'), dev = join(folder, 'apps/electron');
    mkdirSync(app, { recursive: true });
    mkdirSync(join(dev, 'build/helpers/Contents/MacOS'), { recursive: true });
    the.exe = join(app, 'Oby'); the.appPath = dev;
    the.packaged = true;
    expect(pictureTextBin()).toBeNull();
    writeFileSync(join(app, PICTURE_TEXT), '#!/bin/sh\n', { mode: 0o755 });
    expect(pictureTextBin()).toBe(join(app, PICTURE_TEXT));
    the.packaged = false;
    expect(pictureTextBin()).toBeNull();
    writeFileSync(join(dev, 'build/helpers/Contents/MacOS', PICTURE_TEXT), '#!/bin/sh\n', { mode: 0o644 });
    // not one it can run
    expect(pictureTextBin()).toBeNull();
    chmodSync(join(dev, 'build/helpers/Contents/MacOS', PICTURE_TEXT), 0o755);
    expect(pictureTextBin()).toBe(join(dev, 'build/helpers/Contents/MacOS', PICTURE_TEXT));
  });

  it('sphereapp_18: what the scan reads of the text: each reading, each next pair, the best run together, and mended', () => {
    expect(textToScan(readings(printed(['ab]c', 'abjc'], [' de f|g ']))).split('\n')).toEqual([
      'ab]c', 'abjc', 'de f|g', 'ab]cde f|g', 'abjcde f|g', 'ab]cde f|g',
      'ablc', 'abjc', 'abjc', 'deflg', 'ablcdeflg', 'abjcdeflg', 'abjcdeflg', 'ablcdeflg', 'abjcdeflg',
    ]);
    expect(textToScan([])).toBe('\n');
  });

  // On a Mac with the helper staged (scripts/helpers.sh) and gitleaks: Vision and gitleaks for real
  const staged = join(__dirname, '../../../../build/helpers/Contents/MacOS', PICTURE_TEXT);
  const gitleaks = ['/opt/homebrew/bin/gitleaks', '/usr/local/bin/gitleaks'].find((p) => existsSync(p));
  it.skipIf(!existsSync(staged) || !gitleaks)('sphereapp_18: with the real oby-picture-text and gitleaks, a drawn token is found and a logo goes', async () => {
    const { runGitleaks } = await import('../../../main/help');
    const real = { read: runPictureText(staged), gitleaks: runGitleaks(gitleaks!) };
    expect(await scanPicture(TOKEN_PNG, real)).toMatchObject({ kind: 'found' });
    expect(await scanPicture(LOGO_PNG, real)).toEqual({ kind: 'clean' });
  }, 70_000);
});
