// An artifact imported from a dropped file (docs/contracts/artifacts-app.md, ARTIFACTSAPP-12): what
// obid's import takes (POST /v1/artifacts/import, ARTIFACTS-11), and the type it's sent as, read
// from its bytes by main as it sends them (main/obid.ts), never from the page's say.

/** The files the conversation offers to import: a project archive, a page, a document. */
export const IMPORTABLE = /\.(zip|html?|md|markdown)$/i;
/** The most obid takes (its import's own bound, 257 MB). */
export const IMPORT_MOST = 257 * 1024 * 1024;

/** A zip (its first bytes `PK\x03\x04`), a page (its text starts with `<`, after a BOM and spaces),
 * or else a document. */
export function importType(bytes: Uint8Array): 'application/zip' | 'text/html' | 'text/markdown' {
  if (bytes[0] === 0x50 && bytes[1] === 0x4b && bytes[2] === 0x03 && bytes[3] === 0x04) return 'application/zip';
  let i = bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf ? 3 : 0;
  while (i < bytes.length && (bytes[i] === 0x20 || bytes[i] === 0x09 || bytes[i] === 0x0a || bytes[i] === 0x0d)) i++;
  return bytes[i] === 0x3c ? 'text/html' : 'text/markdown';
}
