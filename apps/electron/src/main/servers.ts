import { shell } from 'electron';
import { obid, type Trusted } from './obid';
import { takeInput } from './words';
import { ipc } from './ipc';
import { JOB_ID } from './worktree';
import { safeInteger, string } from '../shared/decode';

// Open on a task's local server (docs/contracts/task-servers.md, TASKSERVERS-8): the page sends only
// the job's id and a port. Main asks obid for that job and opens http://localhost:<port> in the
// owner's default browser only when obid lists that port in the job's `servers` now: never a URL
// from an agent's words, a job's text or the page, and never another host, scheme or path.

// whether obid's job, as GET /v1/jobs/{id} answered it, lists `port` among its servers
export function lists(text: string, port: number): boolean {
  try {
    const servers = (JSON.parse(text) as { servers?: unknown }).servers;
    return Array.isArray(servers) && servers.some((s) => !!s && typeof s === 'object' && (s as { port?: unknown }).port === port);
  } catch {
    return false;
  }
}

// Opens job `id`'s server on `port`: true once the browser was asked, false when main opens
// nothing (not a job's id, not a port, a job obid doesn't have, a port it doesn't list)
export async function openServer(id: string, port: number, open: (url: string) => Promise<void> = shell.openExternal): Promise<boolean> {
  if (!JOB_ID.test(id) || !Number.isInteger(port) || port < 1 || port > 65535) return false;
  const job = await obid('GET', `/v1/jobs/${encodeURIComponent(id)}`);
  if (job.status !== 200 || !lists(job.text, port)) return false;
  await open(`http://localhost:${port}`);
  return true;
}

export function servers(trusted: Trusted) {
  ipc(trusted).handle('open-task-server', [string, safeInteger], (_e, id, port) => (takeInput() ? openServer(id, port) : false), false);
}
