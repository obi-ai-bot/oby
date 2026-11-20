// The browser on obid (docs/contracts/browser-app.md, BROWSERAPP-8 to 10), moved from the lab
// (tools/obid-spec/lab-ui/browser/src/live.ts): its tabs from GET /v1/browser and every event after
// it, what an agent is doing from its `action` events, and the page from the tab's stream, newest
// frame only. Main holds obid's token and follows the streams (window.app.browser); calls go through
// window.app.obid, the owner's acts signed by main on their own click (shared/words.ts).
import {
  BROWSER_DOWNLOADS, BROWSER_PATH, browserTabPath,
  type BrowserAsk, type BrowserDownload, type BrowserDownloadsAnswer, type BrowserEvent, type BrowserFrame, type BrowserInput,
  type BrowserSettings, type BrowserTab, type BrowserTabsAnswer,
} from '../../../shared/obid-wire';
import type { AgentId, BrowserState, Download, Tab } from './view';

export type Live = {
  take(tab: string): Promise<void>;
  giveBack(tab: string): Promise<void>;
  /** your mouse and keys on the page while you hold the tab (POST …/input), sent in order */
  input(tab: string, e: BrowserInput): Promise<void>;
  close(): void;
};

export type Sinks = {
  /** whether this obid has the browser at all (GET /v1/browser answered) */
  offered: (on: boolean) => void;
  state: (s: BrowserState) => void;
  frame: (tab: string, url: string, size: { width: number; height: number }) => void;
  /** why the last call didn't go (obid's words), or null */
  problem: (why: string | null) => void;
};

type Calls = {
  call: (method: string, path: string, body?: unknown) => Promise<{ status: number; text: string }>;
  follow: (fn: (m: { name: string; data: string }) => void) => () => void;
  watch: (tab: string, fn: (data: string) => void) => () => void;
};

const AGENTS: AgentId[] = ['codex', 'claude', 'cursor'];
/** how long what the agent did stays on screen after it ends: an act runs in about 40 ms */
export const LINGER_MS = 1500;
const agentOf = (a: string): AgentId => (AGENTS.includes(a as AgentId) ? (a as AgentId) : 'codex');

/** "http://127.0.0.1:5123/login" → "127.0.0.1:5123/login", as the address shows it. */
export const shown = (url: string) => url.replace(/^[a-z]+:\/\//, '').replace(/\/$/, '') || url;

class Refused extends Error {
  constructor(readonly status: number, message: string) { super(message); }
}

export function connectLive(sinks: Sinks, calls: Calls): Live {
  let closed = false;
  let tabs: BrowserTab[] = [];
  let downloads: BrowserDownload[] = [];
  let settings: BrowserSettings = { agentsUseBrowser: true, agentsUseMac: true };
  let active: string | null = null;
  const doing = new Map<string, { text: string; pointer: { x: number; y: number } | null; seq: number }>();
  const streams = new Map<string, () => void>();
  // The last event each list reflects: a list answer older than an event already applied isn't
  // taken (it would roll the screen back), and an event a newer answer reflects isn't applied again
  // (LAB-UI-LIVE-6, docs/api/browser.md `seq`)
  let tabsSeq = 0;
  let downloadsSeq = 0;

  const call = async <T>(method: string, path: string, body?: unknown): Promise<T> => {
    const a = await calls.call(method, path, body);
    if (a.status < 200 || a.status >= 300) {
      let why = a.text.slice(0, 200);
      try { const e = JSON.parse(a.text) as { error?: string }; if (e.error) why = e.error; } catch { /* not JSON */ }
      throw new Refused(a.status, why || `obid answered ${a.status}`);
    }
    return (a.text ? JSON.parse(a.text) : null) as T;
  };

  const askOf = (t: BrowserTab, a: BrowserAsk) => ({ id: a.id, tab: t.id, kind: a.kind, agent: t.owner.kind === 'agent' ? agentOf(t.owner.agent) : 'codex' as AgentId, host: shown(t.url).split('/')[0], field: a.field });
  const view = (): BrowserState => {
    const jobAgent = new Map(tabs.flatMap((t) => (t.owner.kind === 'agent' ? [[t.owner.job, agentOf(t.owner.agent)] as const] : [])));
    const mine: Tab[] = tabs.map((t) => {
      const d = doing.get(t.id);
      return {
        id: t.id,
        owner: t.owner.kind === 'agent' ? { kind: 'agent', agent: agentOf(t.owner.agent) } : { kind: 'you' },
        job: t.owner.kind === 'agent' ? t.owner.job : undefined,
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
      saved: d.place === 'delivered',
    }));
    return { tabs: mine, active, asks, downloads: dl, settings };
  };
  const publish = () => { if (!closed) sinks.state(view()); };

  // The page: `event: frame` on the tab's stream, the newest one drawn.
  const watch = (tab: string) => {
    streams.set(tab, calls.watch(tab, (data) => {
      try {
        const f = JSON.parse(data) as BrowserFrame;
        sinks.frame(tab, `data:${f.mime || 'image/jpeg'};base64,${f.data}`, { width: f.width, height: f.height });
      } catch { /* a frame that isn't JSON: dropped */ }
    }));
  };
  const unwatch = (tab: string) => { streams.get(tab)?.(); streams.delete(tab); };
  const watchAll = () => {
    for (const t of tabs) if (!streams.has(t.id)) watch(t.id);
    for (const id of [...streams.keys()]) if (!tabs.some((t) => t.id === id)) unwatch(id);
  };

  let following: (() => void) | null = null;
  const refresh = async () => {
    try {
      const [b, d] = await Promise.all([call<BrowserTabsAnswer>('GET', BROWSER_PATH), call<BrowserDownloadsAnswer>('GET', BROWSER_DOWNLOADS)]);
      // an obid from before `seq` (none on the answer): taken as it comes, the watermark unmoved
      if (b.seq === undefined || b.seq >= tabsSeq) { tabs = b.tabs; settings = b.settings; tabsSeq = b.seq ?? tabsSeq; }
      if (d.seq === undefined || d.seq >= downloadsSeq) { downloads = d.downloads; downloadsSeq = d.seq ?? downloadsSeq; }
      sinks.offered(true);
      watchAll();
      publish();
      return true;
    } catch (e) {
      // an obid without the browser: not offered, and nothing followed
      if (e instanceof Refused && e.status === 404) { sinks.offered(false); return false; }
      sinks.problem((e as Error).message);
      return true;
    }
  };

  // Every event: what changed, applied; an unknown type ignored (enums only grow).
  const on = ({ name, data }: { name: string; data: string }) => {
    // the stream (re)connected: the lists read again for what it missed
    if (name === 'open') { void refresh(); return; }
    let e: BrowserEvent;
    try { e = JSON.parse(data); } catch { return; }
    if (e.type === 'download') { if (e.seq <= downloadsSeq) return; downloadsSeq = e.seq; }
    else if (e.type !== 'scriptRun') { if (e.seq <= tabsSeq) return; tabsSeq = e.seq; }
    switch (e.type) {
      case 'tab': tabs = [...tabs.filter((t) => t.id !== e.tab.id), e.tab]; break;
      case 'tabClosed': tabs = tabs.filter((t) => t.id !== e.tabId); break;
      case 'action':
        // what the agent did stays on screen LINGER_MS after it ends, unless it does something else first
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
    watchAll();
    publish();
  };
  void refresh().then((has) => { if (has && !closed) following = calls.follow(on); });

  // one at a time, so a press never overtakes the move before it
  let inputs: Promise<unknown> = Promise.resolve();
  const act = async (path: string) => {
    try { await call('POST', path, {}); sinks.problem(null); } catch (e) { sinks.problem((e as Error).message); }
    await refresh();
  };
  return {
    take: (t) => act(browserTabPath(t, 'take')),
    giveBack: (t) => act(browserTabPath(t, 'giveBack')),
    input: (t, e) => {
      inputs = inputs.catch(() => undefined).then(() => call('POST', browserTabPath(t, 'input'), e)).catch((err) => { sinks.problem((err as Error).message); });
      return inputs.then(() => undefined);
    },
    close: () => { closed = true; following?.(); for (const stop of streams.values()) stop(); streams.clear(); },
  };
}
