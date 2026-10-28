import { useLayoutEffect, useRef, useState } from 'react';

// What Home's box says while it's empty: Oby asking. No names: there's no account
// to take one from.
export const PROMPTS = [
  'What should I work on?',
  'Fix a bug, ship a feature, review a PR…',
  'Tell me about any issue',
  'Describe the change, I’ll pick the agent',
  'What should ship next?',
  'Which pull request needs a review?',
  'Need a refactor? Tell me where',
  'What’s been sitting in your backlog?',
  'Point me at a failing build',
];

// It never changes in front of you: only when you come back to Home (from a page,
// a conversation, a relaunch), and then only if the last one has had 10 minutes.
// Which one, and since when, are remembered across launches (the first ever: the first).
const EVERY = 10 * 60_000;
const KEY = 'oby.prompt';
let last = (() => {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? '') as { i: number; at: number };
    if (Number.isInteger(s.i) && Number.isFinite(s.at)) return s;
  } catch { /* none yet */ }
  const first = { i: 0, at: Date.now() };
  try { localStorage.setItem(KEY, JSON.stringify(first)); } catch { /* not remembered, that's all */ }
  return first;
})();
function pick(): number {
  if (Date.now() - last.at >= EVERY) {
    last = { i: (last.i + 1) % PROMPTS.length, at: Date.now() };
    try { localStorage.setItem(KEY, JSON.stringify(last)); } catch { /* not remembered, that's all */ }
  }
  return last.i % PROMPTS.length;
}

// The prompt for a box that's away (hidden) now and then: picked on showing, and
// again each time it shows after being away. Off (on = false): none.
export function usePrompt(on: boolean, away: boolean): string {
  const [i, setI] = useState(() => (on ? pick() : 0));
  const wasAway = useRef(away);
  // before paint, so the old one never flashes as it comes back
  useLayoutEffect(() => {
    if (on && wasAway.current && !away) setI(pick());
    wasAway.current = away;
  }, [on, away]);
  return on ? PROMPTS[i] : '';
}
