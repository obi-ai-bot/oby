// Settings › Routines' words for a routine (docs/contracts/routines.md, ROUTINES-18): when it runs
// next and how its last run went, from what obid lists (docs/api/routines.md). obid's own `when`
// says its schedule. The words are proposed, Sangam's to change: his design has no screen for
// routines yet.
import type { Routine, RoutineRun } from '../../shared/obid-wire';

const DAY = 86_400_000;
const dayStart = (t: number) => { const d = new Date(t); d.setHours(0, 0, 0, 0); return d.getTime(); };

// A time near now in words: Today at 9:00 AM, Tomorrow at 9:00 AM, Yesterday at 9:00 AM, Monday
// at 9:00 AM within a week, else Nov 28 at 9:00 AM. In the Mac's own time. inline: today, tomorrow
// and yesterday in lower case, for the middle of a line (a day's name keeps its capital)
export function atWords(iso: string, now: number, inline = false): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return iso;
  const time = new Date(t).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const days = Math.round((dayStart(t) - dayStart(now)) / DAY);
  const near = days === 0 ? 'Today' : days === 1 ? 'Tomorrow' : days === -1 ? 'Yesterday' : null;
  const day = near ? (inline ? near.toLowerCase() : near)
    : Math.abs(days) < 7 ? new Date(t).toLocaleDateString([], { weekday: 'long' })
    : new Date(t).toLocaleDateString([], { month: 'short', day: 'numeric' });
  return `${day} at ${time}`;
}

// a started run's job, once it ended
const ENDED: Record<string, string> = { done: 'Done', failed: 'Failed', stopped: 'Stopped', interrupted: 'Waiting for you' };

// How its latest run went, and when
export function lastWords(run: RoutineRun | null, now: number): string {
  if (!run) return 'Hasn’t run yet';
  const when = atWords(run.at, now, true);
  const why = run.reason ? `: ${run.reason.replace(/\.$/, '')}` : '';
  switch (run.outcome) {
    case 'started': return `${run.ended ? ENDED[run.ended] ?? run.ended : 'Running'} · started ${when}`;
    case 'skipped': return `Skipped ${when}${why}`;
    case 'missed': return `Missed${why}`;
    case 'failed': return `Couldn’t start ${when}${why}`;
    default: return `${run.outcome} · ${atWords(run.at, now)}`;
  }
}

// What its line under the name says: its repository, then Paused, when it runs next, or for one on
// GitHub events that it waits for them
export function nextWords(r: Routine, now: number): string {
  const next = !r.enabled ? 'Paused' : r.on ? 'Runs when its event comes' : r.nextRun ? `Next ${atWords(r.nextRun, now, true)}` : 'Not scheduled';
  return `${r.repo} · ${next}`;
}

// whether its last run's outcome reads as a problem (red)
export const lastWentWrong = (run: RoutineRun | null) => !!run && (run.outcome === 'failed' || (run.outcome === 'started' && run.ended === 'failed'));
