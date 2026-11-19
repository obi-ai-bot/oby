import { useCallback, useEffect, useState } from 'react';
import { Group, Row } from './SettingsView';
import { api, type Routine } from './obid';
import { onObid } from './lean';
import { MOCK_ROUTINES } from './mock';
import { lastWentWrong, lastWords, nextWords } from './routines';

// Settings › Routines (docs/contracts/routines.md, ROUTINES-18 to 21): every routine obid has
// (GET /v1/routines), each with obid's words for when it runs, its repository and next run, and how
// its last run went; Pause or Resume (PATCH enabled), Run now (POST …/run) and Delete, which asks
// first here in the page (the app has no confirm()). Read as the page opens, after each change and
// every 30 seconds while it's open. Not in Sangam's design yet: ours until his comes, the T3 Code
// way (ScheduledTasksSettings). On the mock, the mock's routines, changed here only.
const EVERY_MS = 30_000;

export default function RoutinesSettings() {
  const [routines, setRoutines] = useState<Routine[] | null>(onObid ? null : MOCK_ROUTINES);
  const [problem, setProblem] = useState<string | null>(null);
  const read = useCallback(() => {
    if (!onObid) return;
    api.routines().then((list) => { setRoutines(list); setProblem(null); }, (e: Error) => setProblem(e.message));
  }, []);
  useEffect(() => {
    read();
    const t = setInterval(read, EVERY_MS);
    return () => clearInterval(t);
  }, [read]);
  const put = (r: Routine) => setRoutines((list) => (list ?? []).map((x) => (x.id === r.id ? r : x)));
  const gone = (id: string) => setRoutines((list) => (list ?? []).filter((x) => x.id !== id));

  if (routines === null) return problem ? <Group i={1}><Row label={<span className="set-problem" role="alert">{problem}</span>} /></Group> : null;
  if (!routines.length) {
    return (
      <Group i={1} footer="Ask Oby in a conversation for work on a schedule, like “every weekday at 9, review the open pull requests in obi”.">
        <Row label={<span className="set-none">No routines yet</span>} />
      </Group>
    );
  }
  return <>
    {problem && <p className="set-foot set-problem" role="alert">{problem}</p>}
    {routines.map((r, k) => <RoutineGroup key={r.id} routine={r} i={1 + k} onChanged={put} onGone={gone} />)}
  </>;
}

function RoutineGroup({ routine: r, i, onChanged, onGone }: { routine: Routine; i: number; onChanged: (r: Routine) => void; onGone: (id: string) => void }) {
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const now = Date.now();
  // one change at a time; obid's refusal shows under it, in its words
  const act = async (change: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    setProblem(null);
    try { await change(); } catch (e) { setProblem((e as Error).message); } finally { setBusy(false); }
  };
  const pause = () => act(async () => onChanged(onObid ? await api.pauseRoutine(r.id, !r.enabled) : { ...r, enabled: !r.enabled, nextRun: r.enabled ? null : r.nextRun }));
  const run = () => act(async () => {
    if (onObid) onChanged((await api.runRoutine(r.id)).routine);
    else onChanged({ ...r, lastRun: { scheduled: new Date(now).toISOString(), at: new Date(now).toISOString(), outcome: 'started', reason: null, job: 'mock', catchUp: false, ended: null } });
  });
  const remove = () => act(async () => {
    if (onObid) await api.deleteRoutine(r.id);
    onGone(r.id);
  });
  return (
    <Group i={i} title={r.name}>
      <Row label={r.when} about={nextWords(r, now)} />
      {/* a routine on GitHub events has no runs: its events' receipts are its own (docs/api/routines.md) */}
      {!r.on && (
        <Row label="Last run" about={<span className={lastWentWrong(r.lastRun) ? 'set-problem' : undefined}>{lastWords(r.lastRun, now)}</span>} />
      )}
      {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
      {deleting
        ? (
          <div className="set-row is-action" role="group" aria-label={`Delete ${r.name}?`}>
            <span className="set-label">
              {`Delete ${r.name}?`}
              <span className="set-about">The tasks its runs started stay.</span>
            </span>
            <span className="set-control">
              <button className="set-button" autoFocus onClick={() => setDeleting(false)}>Cancel</button>
              <button className="set-button is-danger" disabled={busy} onClick={() => void remove()}>Delete</button>
            </span>
          </div>
        )
        : (
          <div className="set-row is-action">
            <button className="set-button" disabled={busy} onClick={() => void pause()}>{r.enabled ? 'Pause' : 'Resume'}</button>
            {!r.on && <button className="set-button" disabled={busy} onClick={() => void run()}>Run now</button>}
            <button className="set-button is-danger" disabled={busy} onClick={() => setDeleting(true)}>Delete</button>
          </div>
        )}
    </Group>
  );
}
