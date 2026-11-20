// Live mode: lane A's browser API (contract.ts, its routes and shapes), reached through this app's
// own server, which adds the owner's token and signs the owner's acts (vite.config.ts, live()): the
// page has neither. Tabs come from GET /v1/browser and every event after it; what the agent is
// doing ("Clicking Sign in") from its `action` events; the page itself from agent-browser's live
// stream, newest frame only (a slow screen drops frames, never queues them).
import {
  PATH_BROWSER, PATH_DOWNLOADS, PATH_EVENTS, PATH_SETTINGS, VERB_GIVE_BACK, VERB_INPUT, VERB_SECRET, VERB_STREAM, VERB_TAKE,
  askPath, saveCopyPath, tabPath,
  type Ask as CAsk, type BrowserEvent, type Download as CDownload, type DownloadsAnswer, type Settings, type StreamFrame, type StreamInput, type Tab as CTab, type TabsAnswer,
} from './contract';
import type { AgentId, BrowserState, Download, Tab } from './api/types';

export type Live = {
  take(tab: string): Promise<void>;
  giveBack(tab: string): Promise<void>;
  /** the password card's value, into the ask's field; the hold taken for it given back after */
  secret(tab: string, field: string, value: string): Promise<void>;
  saveCopy(download: string): Promise<void>;
  setSettings(s: Partial<Settings>): Promise<void>;
  /** your mouse and keys on the page while you hold the tab (POST …/input), sent in order */
  input(tab: string, e: StreamInput): Promise<void>;
  close(): void;
};

type Sinks = {
  state: (s: BrowserState) => void;
  frame: (tab: string, url: string, size: { width: number; height: number }) => void;
  problem: (why: string | null) => void;
};

const AGENTS: AgentId[] = ['codex', 'claude', 'cursor'];
/** how long what the agent did stays on screen after it ends */
export const LINGER_MS = 1500;
const agentOf = (a: string): AgentId => (AGENTS.includes(a as AgentId) ? (a as AgentId) : 'codex');

/** "http://127.0.0.1:5123/login" → "127.0.0.1:5123/login", as the address shows it. */
const shown = (url: string) => url.replace(/^[a-z]+:\/\//, '').replace(/\/$/, '') || url;

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const r = await fetch(path, { method, headers: body === undefined ? undefined : { 'Content-Type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await r.text();
  if (!r.ok) {
    let reason = text.slice(0, 200);
    try { const e = JSON.parse(text); reason = `${e.reason}: ${e.error}`; } catch { /* not JSON */ }
    throw new Error(`${method} ${path}: ${r.status} ${reason}`);
  }
  return (text ? JSON.parse(text) : null) as T;
}

export function connectLive(_base: string, sinks: Sinks): Live {
  let closed = false;
  let tabs: CTab[] = [];
  let downloads: CDownload[] = [];
  let settings: Settings = { agentsUseBrowser: true, agentsUseMac: true };
  let active: string | null = null;
  const doing = new Map<string, { text: string; pointer: { x: number; y: number } | null; seq: number }>();
  const streams = new Map<string, EventSource>();
  // Save a copy copies: the agent's file stays in its profile (the API keeps listing it so), the copy
  // is in Downloads; what this screen saved, as SaveCopyAnswer said
  const saved = new Set<string>();

  const view = (): BrowserState => {
    const jobAgent = new Map(tabs.flatMap((t) => (t.owner.kind === 'agent' ? [[t.owner.job, agentOf(t.owner.agent)] as const] : [])));
    const mine: Tab[] = tabs.map((t) => {
      const d = doing.get(t.id);
      return {
        id: t.id,
        owner: t.owner.kind === 'agent' ? { kind: 'agent', agent: agentOf(t.owner.agent) } : { kind: 'you' },
        title: t.title || shown(t.url),
        url: shown(t.url),
        secure: t.url.startsWith('https:'),
        state: t.state,
        holder: t.heldBy ? 'you' : t.owner.kind === 'agent' ? 'agent' : null,
        hold: t.holdGeneration,
        dialog: t.dialog,
        computer: t.computer.id === 'local' ? undefined : { name: t.computer.name, os: '' },
        action: t.heldBy ? undefined : d?.text,
        pointer: t.heldBy ? undefined : d?.pointer ? { ...d.pointer, ring: true } : undefined,
      };
    });
    if (!active || !mine.some((t) => t.id === active)) active = mine[0]?.id ?? null;
    const asks = tabs.flatMap((t) => (t.ask && !t.ask.answered ? [askOf(t, t.ask)] : []));
    const dl: Download[] = downloads.map((d) => ({
      id: d.id,
      name: d.name,
      size: `${Math.max(1, Math.round(d.bytes / 1024))} KB`,
      by: d.startedBy === 'agent' ? { kind: 'agent', agent: jobAgent.get(d.job) ?? 'codex' } : { kind: 'you' },
      saved: d.place === 'delivered' || saved.has(d.id),
    }));
    return { tabs: mine, active, asks, downloads: dl, settings };
  };
  const askOf = (t: CTab, a: CAsk) => ({ id: a.id, tab: t.id, kind: a.kind, agent: t.owner.kind === 'agent' ? agentOf(t.owner.agent) : 'codex' as AgentId, host: shown(t.url).split('/')[0], field: a.field });
  const publish = () => { if (!closed) sinks.state(view()); };

  // The last event each list reflects: the tabs' (tabs, asks, settings, what an agent does) and the
  // downloads'. A list answer reflects up to its `seq`; one older than an event already applied
  // isn't taken (it would roll the screen back), and an event a newer answer already reflects isn't
  // applied again. So the events stream alone keeps the screen current: no re-read on a timer.
  let tabsSeq = 0;
  let downloadsSeq = 0;

  const refresh = async () => {
    try {
      const [b, d] = await Promise.all([call<TabsAnswer>('GET', PATH_BROWSER), call<DownloadsAnswer>('GET', PATH_DOWNLOADS)]);
      // an API from before `seq` (none on the answer): taken as it comes, the watermark unmoved
      if (b.seq === undefined || b.seq >= tabsSeq) { tabs = b.tabs; settings = b.settings; tabsSeq = b.seq ?? tabsSeq; }
      if (d.seq === undefined || d.seq >= downloadsSeq) { downloads = d.downloads; downloadsSeq = d.seq ?? downloadsSeq; }
      for (const t of tabs) if (!streams.has(t.id)) watch(t.id);
      sinks.problem(null);
      publish();
    } catch (e) {
      sinks.problem(`live: ${(e as Error).message}`);
    }
  };

  // The page: `event: frame` on the tab's stream, the newest one drawn.
  const watch = (tab: string) => {
    const es = new EventSource(tabPath(tab, VERB_STREAM));
    streams.set(tab, es);
    es.addEventListener('frame', (m) => {
      try {
        const f = JSON.parse((m as MessageEvent).data) as StreamFrame;
        sinks.frame(tab, `data:${f.mime || 'image/jpeg'};base64,${f.data}`, { width: f.width, height: f.height });
      } catch { /* a frame that isn't JSON: dropped */ }
    });
  };

  // Every event: what changed, applied; an unknown type ignored (the contract: enums only grow).
  const events = new EventSource(PATH_EVENTS);
  const on = (raw: string) => {
    let e: BrowserEvent;
    try { e = JSON.parse(raw); } catch { return; }
    if (e.type === 'download') { if (e.seq <= downloadsSeq) return; downloadsSeq = e.seq; }
    else if (e.type !== 'scriptRun') { if (e.seq <= tabsSeq) return; tabsSeq = e.seq; }
    switch (e.type) {
      case 'tab': tabs = [...tabs.filter((t) => t.id !== e.tab.id), e.tab]; if (!streams.has(e.tab.id)) watch(e.tab.id); break;
      case 'tabClosed': tabs = tabs.filter((t) => t.id !== e.tabId); streams.get(e.tabId)?.close(); streams.delete(e.tabId); break;
      case 'action':
        // An act the engine runs in 40 ms would never be seen: what the agent did stays on screen
        // LINGER_MS after it ends, unless it does something else first (a lab choice: README).
        if (e.tabId) {
          const tab = e.tabId;
          const mark = { text: e.text, pointer: e.pointer, seq: e.seq };
          if (e.outcome === 'started') doing.set(tab, mark);
          else {
            if (!doing.has(tab)) doing.set(tab, mark);
            const seen = doing.get(tab)!.seq;
            setTimeout(() => { if (doing.get(tab)?.seq === seen) { doing.delete(tab); publish(); } }, LINGER_MS);
          }
        }
        break;
      case 'ask': tabs = tabs.map((t) => (t.id === e.ask.tabId ? { ...t, ask: e.ask } : t)); break;
      case 'askAnswered': tabs = tabs.map((t) => (t.ask?.id === e.askId ? { ...t, ask: { ...t.ask, answered: true } } : t)); break;
      case 'download': downloads = [...downloads.filter((d) => d.id !== e.download.id), e.download]; break;
      case 'settings': settings = e.settings; break;
      default: return;
    }
    publish();
  };
  for (const t of ['tab', 'tabClosed', 'action', 'ask', 'askAnswered', 'download', 'scriptRun', 'settings']) {
    events.addEventListener(t, (m) => on((m as MessageEvent).data));
  }
  events.onmessage = (m) => on(m.data);
  events.onerror = () => { if (!closed) sinks.problem('live: the events stream dropped; trying again'); };
  void refresh();

  const post = (path: string, body: unknown = {}) => call('POST', path, body);
  // one at a time, so a press never overtakes the move before it
  let inputs: Promise<unknown> = Promise.resolve();
  return {
    take: async (t) => { await post(tabPath(t, VERB_TAKE)); await refresh(); },
    giveBack: async (t) => { await post(tabPath(t, VERB_GIVE_BACK)); await refresh(); },
    secret: async (t, field, value) => {
      const tab = tabs.find((x) => x.id === t);
      const sel = tab?.ask?.field ?? field;
      if (!tab?.heldBy) await post(tabPath(t, VERB_TAKE));
      await post(tabPath(t, VERB_SECRET), { field: sel, value });
      if (tab?.ask && !tab.ask.answered) await post(askPath(tab.ask.id));
      await post(tabPath(t, VERB_GIVE_BACK));
      await refresh();
    },
    saveCopy: async (d) => { await post(saveCopyPath(d)); saved.add(d); await refresh(); },
    setSettings: async (s) => { await call('PUT', PATH_SETTINGS, s); await refresh(); },
    input: (t, e) => {
      inputs = inputs.catch(() => undefined).then(() => post(tabPath(t, VERB_INPUT), e)).catch((err) => { sinks.problem(`live: ${(err as Error).message}`); });
      return inputs.then(() => undefined);
    },
    close: () => { closed = true; events.close(); for (const es of streams.values()) es.close(); },
  };
}
