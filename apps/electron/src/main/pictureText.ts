import { app } from 'electron';
import { execFile } from 'node:child_process';
import { accessSync, constants, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { printable, scanText, type Gitleaks, type Scan } from './help';

// The secret scan of an Obysphere picture before it leaves (SPHEREAPP-18; AGENTS.md, "Non-negotiable"):
// its exact bytes, as scanBytes reads them, and the text macOS reads in its pixels, so a screenshot
// of a token is caught too. The text is read by oby-picture-text (native/picture-text), Vision's
// own text recognition on this Mac, nothing sent anywhere; the app carries it beside gh and
// gitleaks (scripts/helpers.sh). A finding, a reader that isn't there, fails, or takes longer than
// TEXT_WAIT, sends nothing.

export const PICTURE_TEXT = 'oby-picture-text';
// Vision answers a picture in well under a second once loaded; loading it cold took 14 s once on this Mac
export const TEXT_WAIT = 30_000;
// The text in a picture, or a throw when it couldn't be read
export type TextReader = (png: Uint8Array) => Promise<string>;

// Beside the app's executable once packaged (Contents/MacOS, as gitleaks); in development where
// scripts/helpers.sh stages it. Never one on the PATH: none there, nothing goes.
export function pictureTextBin(): string | null {
  const bin = app.isPackaged ? join(dirname(app.getPath('exe')), PICTURE_TEXT) : join(app.getAppPath(), 'build/helpers/Contents/MacOS', PICTURE_TEXT);
  try { if (statSync(bin).isFile()) { accessSync(bin, constants.X_OK); return bin; } } catch { /* below */ }
  return null;
}

// The helper run on the exact bytes, written to a file only this user reads, removed after
export const runPictureText = (bin: string, wait = TEXT_WAIT): TextReader => async (png) => {
  const folder = mkdtempSync(join(tmpdir(), 'oby-picture-'));
  try {
    const file = join(folder, 'picture.png');
    writeFileSync(file, png, { mode: 0o600, flag: 'wx' });
    return await new Promise<string>((resolve, reject) => {
      execFile(bin, [file], { env: { PATH: '/usr/bin:/bin', HOME: folder, TMPDIR: folder }, timeout: wait, maxBuffer: 1024 * 1024, encoding: 'utf8' },
        (error, stdout) => (error ? reject((error as { killed?: boolean }).killed ? new Error(`it took over ${wait / 1000} s`) : error) : resolve(stdout)));
    });
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
};

// What the scan reads of the picture's text: each line as read, then all of them run together (a
// token drawn over two lines), and both again with the marks a reading puts for a letter (`]` for
// `j`, `|` for `l`) as a letter and no spaces, so a token read a little wrong still reads as one
const MISREAD = /[[\]|!(){}]/g;
export function textToScan(read: string): string {
  const lines = read.split('\n').map((l) => l.trim()).filter(Boolean);
  const mended = lines.map((l) => l.replace(MISREAD, 'l').replace(/\s+/g, ''));
  return [...lines, lines.join(''), ...mended, mended.join('')].join('\n');
}

const appReader = (): TextReader | null => { const bin = pictureTextBin(); return bin ? runPictureText(bin) : null; };

// One gitleaks run over the picture's printable bytes and its text; the text is read first, so a
// reader that can't run sends nothing whatever the bytes are
export async function scanPicture(png: Uint8Array, the: { gitleaks?: Gitleaks | null; read?: TextReader | null } = {}): Promise<Scan> {
  const read = the.read === undefined ? appReader() : the.read;
  if (!read) return { kind: 'couldNotRun', why: 'the picture’s text reader isn’t there' };
  let text: string;
  try { text = await read(png); } catch (e) { return { kind: 'couldNotRun', why: `the picture’s text couldn’t be read: ${e instanceof Error ? e.message : String(e)}` }; }
  return scanText(`${printable(png)}\n${textToScan(text)}`, the.gitleaks);
}
