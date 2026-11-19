import { Globe } from 'lucide-react';

// A task's local servers (docs/contracts/task-servers.md, TASKSERVERS-7 and 8): one quiet line,
// "localhost:5173 · Open" for each port obid lists on its job, on the task's page and in its
// session's header, as T3 Code lists the servers a thread started. Open sends main only the job's
// id and the port: main opens http://localhost:<port> itself, only for a port obid lists now.
// Sangam has no screen for it yet: the words are his to change.
export default function TaskServers({ job, ports }: { job: string; ports?: number[] }) {
  if (!ports?.length) return null;
  return (
    // a click here is the line's own: the task's page goes Home on a click off its card
    <div className="task-servers" aria-label="Local servers" onClick={(e) => e.stopPropagation()}>
      <Globe size={13} strokeWidth={1.8} aria-hidden />
      {ports.map((port) => (
        <span key={port} className="task-server">
          localhost:{port} ·{' '}
          <button type="button" aria-label={`Open localhost:${port}`} onClick={() => void window.app?.openTaskServer?.(job, port)}>Open</button>
        </span>
      ))}
    </div>
  );
}
