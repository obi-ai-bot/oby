import type { LoadWhat } from '../../../shared/decode';
import { synced, type ObidState } from '../../../shared/live-state';
import { wired, type LiveMessage, type LiveSnapshot } from '../../../shared/live-wire';
import { parseEvent, type Conversation, type GitHubWork, type InboxItem, type Job, type LimitDecision, type Status } from '../../../shared/obid-wire';

// Main's one follower of obid's events (main/live.ts) as the page meets it through the preload
// (window.app.obid.live and load): the test hands the page main's snapshot after a sync and each
// message main sends. A load reads a conversation's turns through `call` (obid as the test answers
// it) and sends them on before it answers, as main does; anything else it answers true.

const EMPTY: ObidState = { jobs: {}, gone: new Set(), status: null, github: null, inbox: [], conversations: [], turns: {}, goneConversations: new Set(), settledInbox: {} };
type Unnumbered = LiveMessage extends infer M ? (M extends LiveMessage ? Omit<M, 'seq'> : never) : never;
// What obid listed, as a first sync of main's reads it
export type Listed = { status?: unknown; jobs?: unknown[]; inbox?: unknown[]; conversations?: unknown[]; github?: unknown; limits?: unknown[]; complete?: boolean };

type Call = (method: string, path: string) => Promise<{ status: number; text: string }>;
export function liveBridge(call?: Call) {
  let seq = 0;
  let onSnapshot: (s: LiveSnapshot) => void = () => {};
  let onMessage: (m: LiveMessage) => void = () => {};
  const fake = {
    loads: [] as LoadWhat[],
    loading: async (what: LoadWhat): Promise<true | string> => {
      if (!call || !('conversation' in what)) return true;
      const a = await call('GET', `/v1/conversation?id=${what.conversation}&limit=100`);
      if (a.status !== 200) return (JSON.parse(a.text) as { error: string }).error;
      fake.send({ kind: 'turns', conversation: what.conversation, turns: JSON.parse(a.text) });
      return true;
    },
    // what the preload exposes, for window.app.obid
    obid: {
      live: (s: typeof onSnapshot, m: typeof onMessage) => { onSnapshot = s; onMessage = m; return () => {}; },
      load: async (what: LoadWhat) => { fake.loads.push(what); return fake.loading(what); },
    },
    // main synced with obid holding this: its snapshot, live
    sync(l: Listed = {}) {
      const state = synced(EMPTY, EMPTY, {
        status: (l.status ?? { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0 }) as Status,
        jobs: (l.jobs ?? []) as Job[], inbox: (l.inbox ?? []) as InboxItem[], conversations: (l.conversations ?? []) as Conversation[],
        complete: l.complete ?? true, github: (l.github ?? null) as GitHubWork | null,
      });
      onSnapshot({ seq: ++seq, connection: 'live', problem: null, state: wired(state), limits: (l.limits ?? []) as LimitDecision[], complete: l.complete ?? true });
    },
    // one of obid's events, as main parsed and passed it on
    event(name: string, data: unknown) {
      const event = parseEvent(name, JSON.stringify(data));
      if (event) onMessage({ seq: ++seq, kind: 'event', event });
    },
    send(m: Unnumbered) { onMessage({ ...m, seq: ++seq } as LiveMessage); },
  };
  return fake;
}
