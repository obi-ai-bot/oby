import type { BrowserInput } from '../shared/obid-wire';
import { WITHIN_MS } from '../shared/words';

// The owner's mouse and keys on a browser tab they hold (docs/contracts/browser-app.md, BROWSERAPP-10):
// main signs each `…/input` only right after a real input of that kind it saw reach the window itself
// (index.ts), so a script in the page can't drive a held tab alone (device.ts, ownersWords).

type Seen = { kind: 'mouseDown' | 'mouseUp' | 'mouseMove' | 'mouseWheel' | 'keyDown' | 'keyUp'; key?: string; at: number };
let seen: Seen[] = [];
export function sawInput(kind: Seen['kind'], key?: string, at = Date.now()) {
  seen = [...seen.filter((s) => at - s.at < WITHIN_MS), { kind, key, at }].slice(-64);
}

// what each input the page sends must follow: a press, a release and a key are each one real input
// of theirs, used once; a move or the wheel follows any real one of its kind in the last 2 s
const NEEDS: Record<string, Seen['kind']> = {
  mousePressed: 'mouseDown', mouseReleased: 'mouseUp', mouseMoved: 'mouseMove', mouseWheel: 'mouseWheel',
  keyDown: 'keyDown', char: 'keyDown', keyUp: 'keyUp',
};
const ONCE = new Set<Seen['kind']>(['mouseDown', 'mouseUp', 'keyDown', 'keyUp']);
export function mayInput(body: unknown, at = Date.now()): boolean {
  const i = body as BrowserInput | null;
  if (!i || typeof i !== 'object' || (i.type !== 'mouse' && i.type !== 'key')) return false;
  const kind = NEEDS[i.eventType];
  if (!kind || (i.type === 'mouse') !== kind.startsWith('mouse')) return false;
  seen = seen.filter((s) => at - s.at < WITHIN_MS);
  // a key: the very key main saw (a char rides on its keyDown, so it doesn't use it up)
  const match = seen.findIndex((s) => s.kind === kind && (i.type !== 'key' || s.key === i.key));
  if (match < 0) return false;
  if (ONCE.has(kind) && i.eventType !== 'char') seen.splice(match, 1);
  return true;
}
export const INPUT_PATH = /^\/v1\/browser\/tabs\/[^/]+\/input$/;
