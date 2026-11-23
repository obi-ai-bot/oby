// The partition the window opens an agent's tab in (docs/contracts/browser-app.md, BROWSERAPP-45),
// written once for main and the page: main checks a webview's partition against it, and the page
// mounts the webview in it. On another computer each agent partition is one of that computer's own,
// as T3 Code gives each environment its own (apps/desktop/src/ipc/methods/preview.ts,
// resolvePartitionScope), so its sign-ins stay apart from this Mac's and another computer's.

/**
 * The partitions a page may live in, as obid names them: one per agent for the owner's own work
 * (sign-ins kept), and one per job for another Oby's or an untrusted job (not persistent). Never the
 * default session or the window's own.
 */
export const PARTITION = /^(persist:oby-agent-(claude|codex|cursor)|oby-box-[A-Za-z0-9_-]{1,128})$/;
/** a computer's id, as obid gives it (core/crates/obid/src/remote/computers.rs, `add`) */
const COMPUTER = /^[0-9a-f]{16}$/;

/**
 * The partition the window opens for one obid `named`, on `computer` (null: This Mac): obid's name
 * itself on This Mac, `persist:oby-agent-<agent>-on-<id>` for an agent's on another computer, and
 * null for anything else (a box on another computer, a bad name or id).
 */
export function windowPartition(named: unknown, computer: string | null): string | null {
  if (typeof named !== 'string' || !PARTITION.test(named)) return null;
  if (computer === null) return named;
  return COMPUTER.test(computer) && named.startsWith('persist:oby-agent-') ? `${named}-on-${computer}` : null;
}
