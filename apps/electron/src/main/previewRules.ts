import { randomBytes } from 'node:crypto';
import { constants } from 'node:fs';
import { lstat, open, rename, unlink } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import { safeInteger, type Decoder } from '../shared/decode';

// The artifact pane's rules in main that need no Electron (docs/contracts/artifacts-app.md): what the
// page may ask the preview for and what its session lets go (main/artifactPreview.ts, ARTIFACTSAPP-3
// and 8), and how an export is saved (main/artifactExport.ts, ARTIFACTSAPP-11).

export type Bounds = { x: number; y: number; width: number; height: number };
export type Ask = { id: string; version: number; bounds: Bounds; edits?: Record<string, string> } | null;
/** The most an ask's unsaved edits may hold: an edit's own bound (obid's, 17 MB). */
export const EDITS_MOST = 17 << 20;

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const size = (v: unknown) => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= 100_000;
/** `{ id, version, bounds, edits? }` (a lower-case UUID, a version from 1, a box in the window, and
 * the owner's unsaved text for some of its files, ARTIFACTSAPP-8), or null. */
export const previewAsk: Decoder<Ask> = (v) => {
  if (v === null) return { value: null };
  if (!v || typeof v !== 'object') return undefined;
  const { id, version, bounds } = v as Record<string, unknown>;
  if (typeof id !== 'string' || !UUID.test(id) || !Number.isSafeInteger(version) || (version as number) < 1) return undefined;
  if (!bounds || typeof bounds !== 'object') return undefined;
  const b = bounds as Record<string, unknown>;
  if (![b.x, b.y, b.width, b.height].every(size)) return undefined;
  const edits = (v as Record<string, unknown>).edits;
  let kept: Record<string, string> | undefined;
  if (edits !== undefined) {
    if (!edits || typeof edits !== 'object' || Array.isArray(edits)) return undefined;
    const entries = Object.entries(edits as Record<string, unknown>);
    let size = 0;
    for (const [path, text] of entries) {
      if (typeof text !== 'string' || !path || path.length > 1024) return undefined;
      size += path.length + text.length;
    }
    if (entries.length > 512 || size > EDITS_MOST) return undefined;
    kept = Object.fromEntries(entries) as Record<string, string>;
  }
  return { value: { id, version: version as number, bounds: { x: b.x as number, y: b.y as number, width: b.width as number, height: b.height as number }, ...(kept ? { edits: kept } : {}) } };
};

/** `{ id, version }` and no other key (a lower-case UUID, a version from 1): the version a card
 * wants a picture of (ARTIFACTSAPP-18). */
export const thumbAsk: Decoder<{ id: string; version: number }> = (v) => {
  if (!v || typeof v !== 'object' || Array.isArray(v) || Object.keys(v).sort().join() !== 'id,version') return undefined;
  const { id, version } = v as Record<string, unknown>;
  if (typeof id !== 'string' || !UUID.test(id) || !Number.isSafeInteger(version) || (version as number) < 1) return undefined;
  return { value: { id, version: version as number } };
};

/** Whether the preview's session lets a request go: its host page, or the relay on its port. */
export function allowed(url: string, relayPort: number, hostPage: string, devOrigin?: string): boolean {
  let u: URL;
  try { u = new URL(url); } catch { return false; }
  if (u.protocol === 'data:' || u.protocol === 'blob:' || u.protocol === 'devtools:') return true;
  if (u.protocol === 'http:' && /^(?:[a-z0-9]{1,64}-)?preview\.localhost$/.test(u.hostname) && Number(u.port) === relayPort) return true;
  if (devOrigin) {
    const d = new URL(devOrigin);
    if ((u.protocol === 'http:' || u.protocol === 'ws:') && u.host === d.host) return true;
  }
  return u.protocol === 'file:' && url.startsWith(hostPage);
}

/** A version's files read from obid, by path: no prototype, so a file named `__proto__` or
 * `constructor` is one of them like any other. */
export type Files = Record<string, { text: string } | { base64: string }>;
export const noFiles = (): Files => Object.create(null) as Files;

/** The owner's unsaved edits laid over a version's files (ARTIFACTSAPP-8): each over its own file of
 * that version, never a new file, and never a name the files only inherit (`__proto__`). */
export function overlay(files: Files, edits?: Record<string, string>): Files {
  if (edits) for (const [path, text] of Object.entries(edits)) if (Object.hasOwn(files, path)) files[path] = { text };
  return files;
}

/** The host page's call that shows `what`: the files go as data, never as code, and as a string
 * JSON.parse reads, where `__proto__` is a key like any other (in an object literal it would set
 * the object's prototype, and that file would go missing). */
export const shownAs = (what: unknown) => `window.obyShow(JSON.parse(${JSON.stringify(JSON.stringify(what))}))`;

// --- an export saved (main/artifactExport.ts)

export type Exported = { saved: string } | { cancelled: true } | { error: string };
export const artifactId: Decoder<string> = (v) => (typeof v === 'string' && UUID.test(v) ? { value: v } : undefined);
export const fromOne: Decoder<number> = (v) => { const n = safeInteger(v); return n && n.value >= 1 ? n : undefined; };

/** The file name obid gives (`Content-Disposition`), or one made the same way. */
export function nameOf(disposition: string | null, as: 'zip' | 'html', version: number): string {
  const m = /filename="([A-Za-z0-9._-]{1,200})"/.exec(disposition ?? '');
  return m && !m[1].startsWith('.') ? m[1] : `artifact-v${version}.${as}`;
}

type Deps = {
  fetch: (path: string) => Promise<Response | { status: number; text: string }>;
  ask: (defaultPath: string) => Promise<string | null>;
  write: (path: string, bytes: Uint8Array) => Promise<void>;
  downloads: string;
};

/** One export saved: obid's bytes, to the path the owner picked; nothing written when they cancel. */
export async function exportArtifact(id: string, version: number, as: 'zip' | 'html', d: Deps): Promise<Exported> {
  const r = await d.fetch(`/v1/artifacts/${id}/export/${version}?as=${as}`);
  if (!(r instanceof Response)) return { error: r.text || 'obid isn’t answering.' };
  if (!r.ok) return { error: r.status === 404 ? 'That version isn’t there any more.' : `obid answered ${r.status}.` };
  const bytes = new Uint8Array(await r.arrayBuffer());
  const path = await d.ask(join(d.downloads, nameOf(r.headers.get('content-disposition'), as, version)));
  if (!path) return { cancelled: true };
  try {
    await d.write(path, bytes);
  } catch (e) {
    return { error: `Oby couldn’t save it: ${e instanceof Error ? e.message : String(e)}` };
  }
  return { saved: path };
}

/** The bytes saved at `path` whole or not at all: written to a new file of their own beside it (never
 * following a link), then renamed over it. A link at `path` is refused, never written through; one
 * swapped in after that look is replaced by the rename, not followed. */
export async function saveFile(path: string, bytes: Uint8Array) {
  if ((await lstat(path).catch(() => null))?.isSymbolicLink()) throw new Error(`${basename(path)} is a link.`);
  const temp = join(dirname(path), `.${basename(path)}.${randomBytes(6).toString('hex')}.part`);
  const file = await open(temp, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o644);
  try {
    try {
      await file.writeFile(bytes);
    } finally {
      await file.close();
    }
    await rename(temp, path);
  } catch (e) {
    await unlink(temp).catch(() => {});
    throw e;
  }
}
