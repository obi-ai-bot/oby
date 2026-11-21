import type { Decoder } from '../shared/decode';

// The artifact preview's two rules that need no Electron (main/artifactPreview.ts; docs/contracts/
// artifacts-app.md, ARTIFACTSAPP-3): what the page may ask for, and what the preview's session lets go.

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

