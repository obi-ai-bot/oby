import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { X } from 'lucide-react';
import { api, type ChangeRound, type JobChanges } from './obid';
import { useTasks } from './tasks';
import './changes.css';

// A task's Changes (docs/contracts/task-changes.md, CHANGES-4 and 5), as T3 Code's diff panel
// shows a thread's: beside the conversation, the files the task changed first, then each one's
// unified diff, for the whole task or one round, with whitespace left out on a click. obid reads
// it all (GET /v1/jobs/{id}/changes); this only shows it. One panel at a time, for one task:
// openChanges from the task's card or menu, Esc or × to close.

let open: string | null = null;
const listeners = new Set<() => void>();
const changed = () => listeners.forEach((l) => l());
export const openChanges = (task: string) => { open = task; changed(); };
export const closeChanges = () => { open = null; changed(); };
export const useChangesOpen = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => open, () => open);

// One file's part of a unified diff, as git wrote it: from its `diff --git` line to the next
export type FileDiff = { path: string; lines: string[] };
export function splitDiff(diff: string): FileDiff[] {
  const files: FileDiff[] = [];
  for (const line of diff.split('\n')) {
    if (line.startsWith('diff --git ')) {
      // `diff --git a/<path> b/<path>`: the b side, as the files list names it
      const at = line.lastIndexOf(' b/');
      files.push({ path: at >= 0 ? line.slice(at + 3) : line.slice(11), lines: [] });
    } else if (files.length) files[files.length - 1].lines.push(line);
  }
  return files;
}

// a diff line's look: added, deleted, a hunk's head, or git's own header lines (hidden)
const kindOf = (line: string) =>
  line.startsWith('@@') ? 'hunk'
    : line.startsWith('+++') || line.startsWith('---') || /^(index|new file|deleted file|old mode|new mode|similarity|rename|Binary)/.test(line) ? 'meta'
      : line.startsWith('+') ? 'add' : line.startsWith('-') ? 'del' : 'ctx';

const STATUS: Record<string, string> = { added: 'Added', deleted: 'Deleted', modified: 'Changed' };
const roundName = (r: ChangeRound) => `Round ${r.round} · ${new Date(r.at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;

export default function ChangesPanel() {
  const id = useChangesOpen();
  const tasks = useTasks();
  const task = id ? tasks.find((t) => t.id === id) : undefined;
  const [round, setRound] = useState<number | undefined>(undefined);
  const [ignoreWhitespace, setIgnoreWhitespace] = useState(false);
  const [shown, setShown] = useState<JobChanges | null>(null);
  const [error, setError] = useState<string | null>(null);
  const body = useRef<HTMLDivElement>(null);
  // another task: from its whole change again
  useEffect(() => { setRound(undefined); setShown(null); setError(null); }, [id]);
  // read again as the task moves on (its job's revision), a round is picked or whitespace toggled
  const revision = task ? `${task.job.status}·${task.job.commits ?? 0}·${task.job.step}` : '';
  useEffect(() => {
    if (!id) return;
    let live = true;
    api.changes(id, { round, ignoreWhitespace })
      .then((c) => { if (live) { setShown(c); setError(null); } })
      .catch((e: Error) => { if (live) setError(e.message); });
    return () => { live = false; };
  }, [id, round, ignoreWhitespace, revision]);
  useEffect(() => {
    if (!id) return;
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') closeChanges(); };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [id]);
  const diffs = useMemo(() => splitDiff(shown?.diff ?? ''), [shown?.diff]);
  if (!id) return null;
  const files = shown?.files ?? [];
  const added = files.reduce((n, f) => n + f.additions, 0), deleted = files.reduce((n, f) => n + f.deletions, 0);
  const rounds = shown?.rounds ?? [];
  const goTo = (path: string) => body.current?.querySelector(`[data-file="${CSS.escape(path)}"]`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  return (
    <aside className="changes-panel" role="complementary" aria-label="Changes">
      <header className="changes-head">
        <div className="changes-title">
          <h2>Changes</h2>
          <span className="changes-task">{task?.title ?? ''}</span>
        </div>
        <span className="changes-total" aria-label={`${added} added, ${deleted} deleted`}>
          <span className="is-add">+{added}</span> <span className="is-del">−{deleted}</span>
        </span>
        <button type="button" className="changes-close" aria-label="Close Changes" onClick={closeChanges}><X size={16} strokeWidth={1.8} /></button>
      </header>
      <div className="changes-controls">
        <select aria-label="Which round" value={round ?? ''} onChange={(e) => setRound(e.target.value ? Number(e.target.value) : undefined)}>
          <option value="">All rounds</option>
          {rounds.map((r) => <option key={r.round} value={r.round}>{roundName(r)}</option>)}
        </select>
        <label className="changes-ws">
          <input type="checkbox" checked={ignoreWhitespace} onChange={(e) => setIgnoreWhitespace(e.target.checked)} />
          Ignore whitespace
        </label>
      </div>
      <div className="changes-body" ref={body}>
        {error && <p className="changes-empty">{error}</p>}
        {!error && shown && files.length === 0 && <p className="changes-empty">No changes yet.</p>}
        {files.length > 0 && (
          <ul className="changes-files" aria-label="Files">
            {files.map((f) => (
              <li key={f.path}>
                <button type="button" onClick={() => goTo(f.path)} title={f.path}>
                  <span className={`changes-status is-${f.status}`}>{STATUS[f.status] ?? f.status}</span>
                  <span className="changes-path">{f.path}</span>
                  <span className="is-add">+{f.additions}</span>
                  <span className="is-del">−{f.deletions}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        {diffs.map((d) => (
          <section key={d.path} className="changes-file" data-file={d.path} aria-label={d.path}>
            <h3>{d.path}</h3>
            <pre>
              {d.lines.map((line, i) => {
                const kind = kindOf(line);
                return kind === 'meta' ? null : <span key={i} className={`changes-line is-${kind}`}>{line || ' '}{'\n'}</span>;
              })}
            </pre>
          </section>
        ))}
        {/* cut at obid's cap; or none at all, too big or not readable just now (its job at work): the files stand */}
        {shown?.truncated && <p className="changes-empty">{shown.diff ? 'The diff is too big to show whole: Oby cut it here.' : 'Oby couldn’t read the diff whole just now: the files are above.'}</p>}
      </div>
    </aside>
  );
}
