// What the browser shows with no page (docs/contracts/browser-app.md, BROWSERAPP-6, 47): with no tab,
// and on a blank tab of the owner's own (New tab), the local servers Oby's agents are running, as
// T3 Code's empty preview lists a thread's local servers (pingdotgg/t3code@304cf149:
// apps/web/src/components/preview/PreviewEmptyState.tsx:51-93, PreviewLocalServerCard.tsx,
// components/ui/discovery-list.tsx). Only obid's own data: each job's `servers`, the ports its
// agent's processes listen on at localhost (docs/contracts/task-servers.md, TASKSERVERS-1). One click
// opens it in a tab of the owner's own, in a throwaway partition of that tab's (store.tsx `open`), never
// the owner's own cookies. With none, board 17's words. The list's words are proposals
// for Sangam, who has no screen for it yet.
import { Globe, RadioTower, X } from 'lucide-react';
import type { Job } from '../../../shared/obid-wire';
import { useLive } from '../live';
import { useBrowser } from './store';

export type RunningServer = { port: number; process: string; job: string; task: string };

/** every job's local servers now, one a port, by port: obid's `servers` on its jobs */
export function runningServers(jobs: Record<string, Job>): RunningServer[] {
  const byPort = new Map<number, RunningServer>();
  for (const j of Object.values(jobs)) {
    for (const v of j.servers ?? []) {
      if (!Number.isInteger(v.port) || v.port < 1 || v.port > 65535 || byPort.has(v.port)) continue;
      byPort.set(v.port, { port: v.port, process: v.process, job: j.id, task: j.title });
    }
  }
  return [...byPort.values()].sort((a, b) => a.port - b.port);
}

/** where a server opens: its localhost address, http as T3 Code opens a loopback one */
export const serverAddress = (port: number) => `http://localhost:${port}/`;

/** the page with no page: board 17's first-time note (no tab only), then the servers, else its words */
export function StartPage({ tab }: { tab?: string }) {
  const { ui, setUi, api } = useBrowser();
  const { jobs } = useLive();
  const servers = runningServers(jobs);
  return (
    <div className="br-view is-blank" style={{ borderTop: '1px solid var(--divider)' }} data-browser="view">
      {!tab && !ui.tipClosed && (
        <div className="br-tip" role="note">
          <Globe size={15} strokeWidth={1.6} />
          <p>Your agents can use this browser by themselves. You watch here and take control any time; passwords you type go straight to the page, never to them. Settings › General turns it off.</p>
          <button className="icon-button is-28" aria-label="Close" onClick={() => setUi({ tipClosed: true })}><X size={13} strokeWidth={2} /></button>
        </div>
      )}
      {servers.length === 0
        ? <p className="empty-words">No pages yet.{'\n'}When an agent opens one, you see it here, live.</p>
        : (
          <section className="br-servers" aria-label="Local servers" data-browser="servers">
            <h2 className="br-servers-head"><RadioTower size={15} strokeWidth={1.6} aria-hidden />Local servers</h2>
            <div className="br-servers-list">
              {servers.map((v) => (
                <button key={v.port} type="button" className="br-server" onClick={() => void api.open(serverAddress(v.port), tab, true)}>
                  <span className="br-server-icon" aria-hidden><Globe size={15} strokeWidth={1.6} /></span>
                  <span className="br-server-text">
                    <span className="br-server-title">localhost:{v.port}</span>
                    <span className="br-server-about">{v.process} · {v.task}</span>
                  </span>
                </button>
              ))}
            </div>
            <p className="br-servers-foot">The servers your agents are running. Click one to open it in a tab of your own.</p>
          </section>
        )}
    </div>
  );
}
