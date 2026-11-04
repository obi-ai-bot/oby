import { useRef, useState } from 'react';
import { api } from './obid';
import { tickRepo } from './sharing';
import type { InboxItem } from '../../shared/obid-wire';

// The card's repositories (docs/contracts/peer-thread.md, PEER-38): each in full, the most recently
// worked on first, ticked only by the owner, with no way to tick them all. Each tick asks obid for the
// card again, whose title, what Confirm signs, names what's ticked, so that's what shows above it.
// onSending: Confirm waits while a tick is on its way.
export default function RepoTicks({ offered, chosen, onCard, onSending }: {
  offered: string[];
  chosen: string[];
  onCard: (card: InboxItem) => void;
  onSending: (sending: boolean) => void;
}) {
  const [ticked, setTicked] = useState(chosen);
  const [problem, setProblem] = useState<string | null>(null);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const sending = useRef(0);
  const toggle = (repo: string) => {
    const next = tickRepo(offered, ticked, repo);
    setTicked(next);
    setProblem(null);
    sending.current += 1;
    onSending(true);
    queue.current = queue.current
      .then(() => api.offerSharing(false, next))
      .then(onCard, (e: Error) => setProblem(e.message))
      .finally(() => { sending.current -= 1; if (!sending.current) onSending(false); });
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
        {!offered.length && <p className="share-repo is-none">No repositories yet.</p>}
      </div>
      {problem && <p className="set-problem">{problem}</p>}
    </>
  );
}
