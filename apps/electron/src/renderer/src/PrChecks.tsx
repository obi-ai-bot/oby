import { useState } from 'react';
import { Check } from 'lucide-react';
import type { CheckRun } from './obid';

// A pull request's checks, each on its own line (docs/contracts/github-work.md, GITHUBWORK-8 and 9),
// as T3 Code's checks popover lists them (apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx,
// pullRequestDetail.logic.ts groupPullRequestChecks): the failed ones first, then those still running,
// then those that passed, each with its mark, its name, its state and Details, its page; a line over
// them says how many failed or run. While any failed or runs, the passed ones wait behind Show all.
// Everything here is GitHub's data, shown as text; Details opens only an https page.

const ORDER = ['FAILURE', 'PENDING', 'SUCCESS'] as const;
const WORD: Record<string, string> = { FAILURE: 'Failed', PENDING: 'Running', SUCCESS: 'Passed' };
const rank = (state: string) => { const i = ORDER.indexOf(state as typeof ORDER[number]); return i < 0 ? 1 : i; };

// failed, then running (and any state this build doesn't know), then passed; GitHub's order within each
export const sortChecks = (runs: CheckRun[]) => runs.map((r, i) => ({ r, i })).sort((a, b) => rank(a.r.state) - rank(b.r.state) || a.i - b.i).map(({ r }) => r);

// "2 of 9 failed", "3 of 9 running", "9 passed"
export function checksLine(runs: CheckRun[]) {
  const failed = runs.filter((r) => r.state === 'FAILURE').length, running = runs.filter((r) => rank(r.state) === 1).length;
  if (failed) return `${failed} of ${runs.length} failed${running ? `, ${running} running` : ''}`;
  if (running) return `${running} of ${runs.length} running`;
  return `${runs.length} passed`;
}

const openCheck = (url: string | null) => { if (url?.startsWith('https://')) window.app?.openExternal(url); };

export default function PrChecks({ runs }: { runs: CheckRun[] }) {
  const [all, setAll] = useState(false);
  if (!runs.length) return null;
  const sorted = sortChecks(runs);
  const waiting = sorted.some((r) => r.state !== 'SUCCESS');
  const shown = all || !waiting ? sorted : sorted.filter((r) => r.state !== 'SUCCESS');
  const hidden = sorted.length - shown.length;
  return (
    <div className="pr-checks">
      <p className="pr-checks-line">{checksLine(runs)}</p>
      <ul className="pr-checks-list" aria-label="Checks">
        {shown.map((r, i) => (
          <li key={`${r.name}-${i}`} className={`is-${(WORD[r.state] ?? 'Running').toLowerCase()}`}>
            <span className="tc-mark" role="img" aria-label={WORD[r.state] ?? r.state}>
              {r.state === 'SUCCESS' ? <span className="tc-mark-done"><Check size={9} strokeWidth={3.2} /></span>
                : <span className={`status-dot ${r.state === 'FAILURE' ? 'is-failed' : 'is-working'}`} />}
            </span>
            <span className="pr-check-name" title={r.name}>{r.name}</span>
            <span className="pr-check-state">{WORD[r.state] ?? r.state}</span>
            {r.url?.startsWith('https://') && <button type="button" className="pr-check-details" onClick={() => openCheck(r.url)}>Details</button>}
          </li>
        ))}
      </ul>
      {hidden > 0 && <button type="button" className="pr-checks-all" onClick={() => setAll(true)}>Show all {sorted.length}</button>}
    </div>
  );
}
