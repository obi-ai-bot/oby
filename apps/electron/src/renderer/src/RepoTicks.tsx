import { useEffect, useRef, useState } from 'react';
import { api } from './obid';
import { tickRepo } from './sharing';
import type { InboxItem } from '../../shared/obid-wire';

// The card's repositories (docs/contracts/peer-thread.md, PEER-38): each in full, the most recently
// worked on first, ticked only by the owner, with no way to tick them all. Each tick asks obid for the
// card again, whose title, what Confirm signs, names what's ticked, so that's what shows above it. The
// boxes follow what obid has: another view of the card moving it, or a tick it refused.
// onHold: Confirm stays off while a tick is on its way or one was refused.
export default function RepoTicks({ offered, chosen, onCard, onHold }: {
  offered: string[];
  chosen: string[];
  onCard: (card: InboxItem) => void;
  onHold: (hold: boolean) => void;
}) {
  const [ticked, setTicked] = useState(chosen);
  const [sending, setSending] = useState(0);
  const [problem, setProblem] = useState<string | null>(null);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const obids = useRef(chosen);
  obids.current = chosen;
  const hold = sending > 0 || !!problem;
  useEffect(() => onHold(hold), [hold]);
  const said = chosen.join('\n');
  useEffect(() => { if (!sending) setTicked(obids.current); }, [said, sending]);
  const toggle = (repo: string) => {
    const next = tickRepo(offered, ticked, repo);
    setTicked(next);
    setProblem(null);
    setSending((n) => n + 1);
    queue.current = queue.current
      .then(() => api.offerSharing(false, next))
      .then(onCard, (e: Error) => setProblem(e.message))
      .finally(() => setSending((n) => n - 1));
  };
  return (
    <>
      <div className="share-repos" role="group" aria-label="Repositories to tell about">
        {offered.map((repo) => (
          <label key={repo} className="share-repo">
            <input type="checkbox" checked={ticked.includes(repo)} onChange={() => toggle(repo)} />
            <span>{repo}</span>
          </label>
        ))}
      </div>
      {problem && <p className="set-problem">{problem}</p>}
    </>
  );
}
