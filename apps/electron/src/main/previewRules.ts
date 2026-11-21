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
