// What main's one follower of obid's events (main/live.ts) tells the page, through the preload
// (docs/contracts/live-state.md, LIVE-10 and 11): what it holds when the page asks
// (`obid-snapshot`), then each message it sends after (`obid-live`), every one numbered (`seq`).
import type { ObidState } from './live-state';
import type { Event, LimitDecision, Turn } from './obid-wire';

// ObidState as it crosses IPC: its sets as arrays
export type WireState = Omit<ObidState, 'gone' | 'goneConversations'> & { gone: string[]; goneConversations: string[] };
export const wired = (s: ObidState): WireState => ({ ...s, gone: [...s.gone], goneConversations: [...s.goneConversations] });
export const unwired = (s: WireState): ObidState => ({ ...s, gone: new Set(s.gone), goneConversations: new Set(s.goneConversations) });

// Whether main reaches obid's stream: live once it synced, down with why (main/obid.ts, reason)
export type Connection = { connection: 'connecting' | 'live' | 'down'; problem: string | null; why?: 'waiting' | 'approval' };
// What main holds: obid's state, the limit decisions, whether its last conversation list was whole
// (complete), and the number of the last message it sent
export type LiveSnapshot = Connection & { seq: number; state: WireState; limits: LimitDecision[]; complete: boolean };
export type LiveMessage = { seq: number } & (
  | ({ kind: 'snapshot' } & Omit<LiveSnapshot, 'seq'>)  // after each sync
  | { kind: 'event'; event: Event }                     // as parsed, applied first when it's the rule's
  | { kind: 'turns'; conversation: string; turns: Turn[] }  // a conversation's, read for the page
  | { kind: 'limits'; limits: LimitDecision[] }         // changed
  | ({ kind: 'connection' } & Connection)
);

// The page's half (preload/index.ts): listening first, then asking for the snapshot; what came
// meanwhile, and everything after, handed on in order once, only past the snapshot's number
export function following(listen: (on: (m: LiveMessage) => void) => () => void, ask: () => Promise<LiveSnapshot | null>,
  onSnapshot: (s: LiveSnapshot) => void, onMessage: (m: LiveMessage) => void): () => void {
  let from: number | null = null, stopped = false;
  const held: LiveMessage[] = [];
  const hand = (m: LiveMessage) => {
    if (from === null) return void held.push(m);
    if (m.seq <= from) return;
    from = m.seq;
    if (m.kind === 'snapshot') onSnapshot(m);
    else onMessage(m);
  };
  const stop = listen(hand);
  void ask().then((s) => {
    if (stopped || !s) return;
    from = s.seq;
    onSnapshot(s);
    held.splice(0).forEach(hand);
  });
  return () => { stopped = true; stop(); };
}
