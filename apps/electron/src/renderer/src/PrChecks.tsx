import { useRef, useState } from 'react';
import { Check } from 'lucide-react';
import type { CheckRun } from './obid';

// A pull request's checks, each on its own line (docs/contracts/github-work.md, GITHUBWORK-8 and 9),
// as T3 Code's checks popover lists them (apps/web/src/components/pullRequest/PullRequestChecksPopover.tsx,
// pullRequestDetail.logic.ts groupPullRequestChecks): the failed ones first, then those still running,
// then those that passed, each with its mark, its name, its state and Details, its page; a line over
// them says how many failed or run. While any failed or runs, the passed ones wait behind Show all.
// Everything here is GitHub's data, shown as text; Details opens only an https page.
// On the owner's own red pull request, Fix CI sits on that line, over the failed checks
// (GITHUBWORK-30), as T3 Code puts Fix on a failing check (PullRequestSummaryTab.tsx): no card
// pushed at the owner (GITHUBWORK-28), a button on the red check they're looking at. While its
// start is on its way it's disabled, so a click starts one task (GITHUBWORK-31).

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

export type FixCi = { onFix: () => Promise<unknown> | void };

export default function PrChecks({ runs, fix }: { runs: CheckRun[]; fix?: FixCi }) {
  const [all, setAll] = useState(false);
  // the start on its way: a ref, so a second click before the next render starts nothing either
  const [starting, setStarting] = useState(false), going = useRef(false);
  if (!runs.length) return null;
  const sorted = sortChecks(runs);
  const waiting = sorted.some((r) => r.state !== 'SUCCESS');
  const shown = all || !waiting ? sorted : sorted.filter((r) => r.state !== 'SUCCESS');
  const hidden = sorted.length - shown.length;
  const failed = runs.some((r) => r.state === 'FAILURE');
  const onFix = async () => {
    if (!fix || going.current) return;
    going.current = true;
    setStarting(true);
    try { await fix.onFix(); } finally { going.current = false; setStarting(false); }
  };
  return (
    <div className="pr-checks">
      <p className="pr-checks-line">
        {checksLine(runs)}
        {fix && failed && <button type="button" className="pr-checks-fix" disabled={starting} onClick={onFix}>Fix CI</button>}
      </p>
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
