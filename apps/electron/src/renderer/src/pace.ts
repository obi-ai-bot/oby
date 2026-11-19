// An account's usage pace (docs/contracts/usage.md, USAGE-18), worked out here from what obid
// already says: the share of its window used against the share of the window gone, the T3 Code
// way (ThreadDetailsUsage's pace). More than 5 points over an even pace is ahead of it, more than 5
// under is under it, the rest on it. The words are proposed, Sangam's to change.

export type Pace = 'ahead' | 'on' | 'under';
export const PACE_WORDS: Record<Pace, string> = { ahead: 'Ahead of pace', on: 'On pace', under: 'Under pace' };

const MINUTE = 60_000, HOUR = 60 * MINUTE, DAY = 24 * HOUR;
// the windows obid reads: Claude Code's and Codex's 5-hour session, and their week
export const SESSION_MS = 5 * HOUR;
export const WEEK_MS = 7 * DAY;

// The pace of `used` percent of a window of `length` ms that ends at `ends`, at `now`; none when
// the window has ended or its end is unreadable
export function paceOf(used: number, ends: string, length: number, now: number): Pace | null {
  const end = Date.parse(ends);
  if (!Number.isFinite(end) || end <= now) return null;
  const gone = Math.min(1, Math.max(0, 1 - (end - now) / length));
  const gap = used - gone * 100;
  return gap > 5 ? 'ahead' : gap < -5 ? 'under' : 'on';
}

// what's left of a window, in a few characters: 3d 4h, 2h 10m, 40m
export function leftWords(ms: number): string {
  const left = Math.max(0, ms);
  const d = Math.floor(left / DAY), h = Math.floor((left % DAY) / HOUR), m = Math.floor((left % HOUR) / MINUTE);
  return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m}m` : `${Math.max(1, m)}m`;
}

// The line under an account's bar: the bar's own window (its 5-hour session while one shows, else
// its week), its pace and what's left of it. None for a window with no length obid knows (Cursor's
// plan, an API key), no figure or no reset.
export function paceLine(a: { session?: number | null; sessionEnds?: string | null; used: number | null; weekly?: boolean; weekEnds?: string | null }, now: number): string | null {
  const window = a.session != null
    ? (a.sessionEnds ? { used: a.session, ends: a.sessionEnds, length: SESSION_MS, name: 'session' } : null)
    : a.weekly && a.used != null && a.weekEnds ? { used: a.used, ends: a.weekEnds, length: WEEK_MS, name: 'week' } : null;
  if (!window) return null;
  const pace = paceOf(window.used, window.ends, window.length, now);
  return pace && `${PACE_WORDS[pace]} · ${leftWords(Date.parse(window.ends) - now)} of the ${window.name} left`;
}
