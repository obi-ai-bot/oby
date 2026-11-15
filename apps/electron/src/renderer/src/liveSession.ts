import { useCallback, useEffect, useSyncExternalStore } from 'react';
import { knownUpTo, withEntries, withLine } from './liveTask';
import { api, type Event, type TranscriptEntry } from './obid';
import { detailOf } from './jobPage';
// (live.ts's, read only when called: live.ts imports this module too)
import { liveState, say, useLive } from './live';

// A job's session, for whatever shows it (its live card, its pane, Full session): kept apart from
// the rest of the state, so an agent's every line re-renders only those, not all that `publish` feeds.
// Only a job something asked for is kept. Fetched when it's asked for and again at each of the job's
// events (at most once a second while its agent works, and on coming back after a lost connection),
// from the last entry known with none missing before it, so an event that didn't reach here is fetched
// too; page after page (obid sends at most 1,000 at a time) until obid's `next` is null.
const sessions = new Map<string, TranscriptEntry[]>();
const sessionListeners = new Map<string, Set<() => void>>();
export const sessionNow = (id: string) => sessions.get(id);
// a deleted job's goes with it
export const forgetSession = (id: string) => sessions.delete(id);
const setSession = (id: string, entries: TranscriptEntry[]) => { sessions.set(id, entries); sessionListeners.get(id)?.forEach((l) => l()); };
export function heard(e: Extract<Event, { kind: 'transcript' | 'transcriptLine' }>) {
  const had = sessions.get(e.job);
  if (had) setSession(e.job, e.kind === 'transcript' ? withEntries(had, [e.entry]) : withLine(had, e.n, e.append));
}
// the last to stop showing it lets it go: a job looked at once costs nothing from then on
export const useSession = (id: string) => useSyncExternalStore(useCallback((l: () => void) => {
  const all = sessionListeners.get(id) ?? new Set();
  sessionListeners.set(id, all.add(l));
  return () => {
    all.delete(l);
    if (all.size === 0) { sessionListeners.delete(id); sessions.delete(id); }
  };
}, [id]), () => sessions.get(id));
// follow: keep it up to date while this shows it (a card only while its task runs)
export function useFollowedSession(id: string, follow = true) {
  const wire = useLive().jobs[id];
  useEffect(() => { if (follow) void loadSession(id); }, [id, follow, wire]);
  return useSession(id);
}
// What a call ran and gave back, fetched when its line opens
export const entryDetail = (id: string, n: number) => api.transcriptEntry(id, n).then((e) => detailOf(e.detail), (e) => { say(e); return undefined; });
const loading = new Map<string, boolean>();  // in flight; true: asked again meanwhile
export async function loadSession(id: string): Promise<void> {
  if (liveState().gone.has(id)) return;
  if (loading.has(id)) { loading.set(id, true); return; }
  loading.set(id, false);
  if (!sessions.has(id)) sessions.set(id, []);
  try {
    let after: number | null | undefined = knownUpTo(sessions.get(id)!) || undefined;
    do {
      const page = await api.transcript(id, after ?? undefined);
      const had = sessions.get(id);
      if (!had) break;
      if (page.entries.length) setSession(id, withEntries(had, page.entries.map((e) => ({ ...e, fetched: true }))));
      after = page.next;
    } while (after != null);
  } catch (e) { say(e); }
  const again = loading.get(id);
  loading.delete(id);
  if (again && sessions.has(id)) return loadSession(id);
}
