import { useEffect, useRef, useState } from 'react';
import { api } from './obid';
import { shownId } from './live';
import type { Task } from './tasks';

// The side panel's search over every conversation (docs/contracts/chat-first.md, SEARCH-1 to 3), as
// T3 Code's palette searches its threads (apps/web/src/state/queries.ts, useThreadSearch): what's on
// the page is matched as it's typed (a conversation's title and the words of those loaded, a task's
// title and request), and obid's own search (`GET /v1/conversations?q=`, #593) finds the words in
// every conversation it keeps, those never opened here too. It's asked 120 ms after the typing
// stops, at most 200 characters, and what it found stays until its next answer, so rows don't jump.

export const SEARCH_WAIT_MS = 120;
export const SEARCH_MOST = 200;

// What obid's search found: each conversation (as the panel shows its id) with the words around the
// match in its latest turn holding them ('' when only its title does)
export type Found = Map<string, string>;

// obid's own search: one call, the answer as Found (none: a query obid turns down, or no obid)
export async function searchObid(query: string): Promise<Found> {
  const q = query.trim().slice(0, SEARCH_MOST).trim();
  if (!q) return new Map();
  const list = await api.conversations({ q, limit: 50 });
  return new Map(list.map((c) => [shownId(c.id), c.turn?.text ?? '']));
}

// The query as obid hears it, waited for; on: obid is the runner here (lean.ts onObid)
export function useObidSearch(query: string, on: boolean): Found {
  const [found, setFound] = useState<Found>(new Map());
  const asked = useRef(0);
  const q = query.trim().slice(0, SEARCH_MOST).trim();
  useEffect(() => {
    if (!on || !q) { asked.current++; setFound(new Map()); return; }
    const mine = ++asked.current;
    const t = setTimeout(() => {
      // an answer to words since changed is dropped; one that fails leaves what was found
      searchObid(q).then((f) => { if (asked.current === mine) setFound(f); }, () => {});
    }, SEARCH_WAIT_MS);
    return () => clearTimeout(t);
  }, [q, on]);
  return found;
}

// The conversations (as the panel shows their ids) holding a task the words are in: its title or
// what it was asked to do
export function tasksMatching(tasks: Task[], query: string): Set<string> {
  const q = query.trim().toLowerCase();
  if (!q) return new Set();
  return new Set(tasks.filter((t) => `${t.title} ${t.job.request ?? ''}`.toLowerCase().includes(q) && t.job.conversation).map((t) => t.job.conversation!));
}
