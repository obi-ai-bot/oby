// The browser on obid (docs/contracts/browser-app.md, BROWSERAPP-8 to 10), moved from the lab
// (tools/obid-spec/lab-ui/browser/src/live.ts): its tabs from GET /v1/browser and every event after
// it, and what an agent is doing from its `action` events; the page itself is the tab's own <webview>
// (Webviews.tsx). Main holds obid's token and follows the events (window.app.browser); calls go
// through window.app.obid, the owner's acts signed by main on their own click (shared/words.ts).
import {
  BROWSER_DOWNLOADS, BROWSER_PATH, BROWSER_SETTINGS, browserAskPath, browserSaveCopyPath, browserTabPath,
  type BrowserAsk, type BrowserDownload, type BrowserDownloadsAnswer, type BrowserEvent,
  type BrowserSettings, type BrowserTab, type BrowserTabsAnswer,
} from '../../../shared/obid-wire';
import { OWN_THREAD, STOPPED, type AgentId, type BrowserState, type Download, type Tab } from './view';

export type Live = {
  /** true once obid took the tab (a 2xx), false when it refused */
  take(tab: string): Promise<boolean>;
  /** true once obid gave it back (a 2xx), false when it refused */
  giveBack(tab: string): Promise<boolean>;
  /** a password or code into the page's field, through main on your click; true once filled */
  fill(tab: string, field: string, value: string, ask?: string): Promise<boolean>;
  /** an ask answered: a choice, or none (Skip) */
  answer(ask: string, choice?: string): Promise<void>;
  /** an agent's download copied into the owner's Downloads (POST …/saveCopy) */
  saveCopy(download: string): Promise<void>;
  /** one of Settings › General's switches (PUT /v1/browser/settings, one key) */
  setSettings(s: Partial<BrowserSettings>): Promise<void>;
  close(): void;
};

/** main's word on a tab's own page (`browser-nav-state`) */
export type PageState = { url: string; canGoBack: boolean; canGoForward: boolean; loading: boolean; failed: string | null };

export type Sinks = {
  /** whether this obid has the browser at all (GET /v1/browser answered) */
  offered: (on: boolean) => void;
  state: (s: BrowserState) => void;
  /** why the last call didn't go (obid's words), or null */
  problem: (why: string | null) => void;
  /** main's word on an owner's own tab's page (keyed by OWN_THREAD): the window's, never obid's */
  own?: (p: PageState & { tabId: string }) => void;
};

type Calls = {
  call: (method: string, path: string, body?: unknown) => Promise<{ status: number; text: string }>;
  follow: (fn: (m: { name: string; data: string }) => void) => () => void;
  /** main fills the field on the owner's click (main/browser.ts): null once filled, else why not */
  fill: (tab: string, field: string, value: string, ask?: string) => Promise<string | null>;
  /** each tab's own page state from main (main/browserGuest.ts): a function that stops listening */
  navState: (fn: (s: { threadId: string; tabId: string; url: string; canGoBack: boolean; canGoForward: boolean; loading: boolean; failed: string | null }) => void) => () => void;
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
  // what Save a copy saved, by download: its name in Downloads (SaveCopyAnswer.savedAs); the
  // agent's file stays in its profile, so obid keeps listing it there
  const saved = new Map<string, string>();
  // the tabs whose agent ran a script and has done nothing since (11)
  const ran = new Set<string>();
  // The last event each list reflects: a list answer older than an event already applied isn't
  // taken (it would roll the screen back), and an event a newer answer reflects isn't applied again
  // (LAB-UI-LIVE-6, docs/api/browser.md `seq`)
  let tabsSeq = 0;
  let downloadsSeq = 0;
  // main's last word on each tab's own page, by job and tab: its address (obid's stays at its own
  // last command's), whether it has a page behind, one ahead, is loading, and where its last load failed
  const pages = new Map<string, { url: string; canGoBack: boolean; canGoForward: boolean; loading: boolean; failed: string | null }>();

  const call = async <T>(method: string, path: string, body?: unknown): Promise<T> => {
    const a = await calls.call(method, path, body);
    if (a.status < 200 || a.status >= 300) {
      let why = a.text.slice(0, 200);
      try { const e = JSON.parse(a.text) as { error?: string }; if (e.error) why = e.error; } catch { /* not JSON */ }
      throw new Refused(a.status, why || `obid answered ${a.status}`);
    }
    return (a.text ? JSON.parse(a.text) : null) as T;
  };

  const askOf = (t: BrowserTab, a: BrowserAsk) => ({ id: a.id, tab: t.id, kind: a.kind, agent: t.owner.kind === 'agent' ? agentOf(t.owner.agent) : 'codex' as AgentId, host: shown(t.url).split('/')[0], field: a.field, reason: a.reason, choices: a.choices, job: a.job });
  const view = (): BrowserState => {
    const jobAgent = new Map(tabs.flatMap((t) => (t.owner.kind === 'agent' ? [[t.owner.job, agentOf(t.owner.agent)] as const] : [])));
    const mine: Tab[] = tabs.map((t) => {
      const d = doing.get(t.id);
      const p = t.owner.kind === 'agent' ? pages.get(`${t.owner.job}\n${t.id}`) : undefined;
      const at = p?.url || t.url;
      return {
        id: t.id,
        owner: t.owner.kind === 'agent' ? { kind: 'agent', agent: agentOf(t.owner.agent) } : { kind: 'you' },
        job: t.owner.kind === 'agent' ? t.owner.job : undefined,
        src: t.url,
        partition: t.partition,
        title: t.title || shown(t.url),
        url: shown(at),
        secure: at.startsWith('https:'),
        // obid's stopped state stays until its agent's next command: once main says the tab's own
        // page loaded since, the page shows again, as T3's own webview's load does (BROWSERAPP-34)
        state: p && !p.loading && p.failed === null && STOPPED.includes(t.state) ? 'ready' : t.state,
        failedAt: p?.failed ?? undefined,
        holder: t.heldBy ? 'you' : t.owner.kind === 'agent' ? 'agent' : null,
        hold: t.holdGeneration,
        dialog: t.dialog,
        computer: t.computer.id === 'local' ? undefined : { name: t.computer.name, os: '' },
        action: t.heldBy ? undefined : d?.text,
        pointer: t.heldBy ? undefined : d?.pointer ? { ...d.pointer, ring: true } : undefined,
        ran: ran.has(t.id) && !d ? {} : undefined,
        canGoBack: p?.canGoBack ?? false,
        canGoForward: p?.canGoForward ?? false,
        pageLoading: p?.loading ?? false,
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
      savedAs: saved.get(d.id) ?? (d.place === 'delivered' ? d.name : undefined),
    }));
    return { tabs: mine, active, asks, downloads: dl, settings };
  };
  const publish = () => { if (!closed) sinks.state(view()); };
  const unNav = calls.navState((s) => {
    if (s.threadId === OWN_THREAD) { sinks.own?.({ tabId: s.tabId, url: s.url, canGoBack: s.canGoBack, canGoForward: s.canGoForward, loading: s.loading, failed: s.failed }); return; }
    pages.set(`${s.threadId}\n${s.tabId}`, { url: s.url, canGoBack: s.canGoBack, canGoForward: s.canGoForward, loading: s.loading, failed: s.failed });
    publish();
  });

  let following: (() => void) | null = null;
  const refresh = async () => {
    try {
      const [b, d] = await Promise.all([call<BrowserTabsAnswer>('GET', BROWSER_PATH), call<BrowserDownloadsAnswer>('GET', BROWSER_DOWNLOADS)]);
      // an obid from before `seq` (none on the answer): taken as it comes, the watermark unmoved
      if (b.seq === undefined || b.seq >= tabsSeq) { tabs = b.tabs; settings = b.settings; tabsSeq = b.seq ?? tabsSeq; }
      if (d.seq === undefined || d.seq >= downloadsSeq) { downloads = d.downloads; downloadsSeq = d.seq ?? downloadsSeq; }
      sinks.offered(true);
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
          if (e.outcome === 'started') { doing.set(tab, mark); if (e.tool !== 'oby_browser_evaluate') ran.delete(tab); }
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
      case 'scriptRun': ran.add(e.tabId); break;
      case 'settings': settings = e.settings; break;
      default: return;
    }
    publish();
  };
  void refresh().then((has) => { if (has && !closed) following = calls.follow(on); });

  const act = async (path: string) => {
    let done = false;
    try { await call('POST', path, {}); sinks.problem(null); done = true; } catch (e) { sinks.problem((e as Error).message); }
    await refresh();
    return done;
  };
  return {
    take: (t) => act(browserTabPath(t, 'take')),
    giveBack: (t) => act(browserTabPath(t, 'giveBack')),
    fill: async (t, field, value, ask) => {
      const why = await calls.fill(t, field, value, ask);
      sinks.problem(why);
      await refresh();
      return why === null;
    },
    answer: async (a, choice) => {
      try { await call('POST', browserAskPath(a), choice === undefined ? {} : { choice }); sinks.problem(null); } catch (e) { sinks.problem((e as Error).message); }
      await refresh();
    },
    saveCopy: async (d) => {
      try {
        const a = await call<{ savedAs?: string }>('POST', browserSaveCopyPath(d), {});
        saved.set(d, (a?.savedAs ?? '').split('/').pop() || (downloads.find((x) => x.id === d)?.name ?? ''));
        sinks.problem(null);
      } catch (e) { sinks.problem((e as Error).message); }
      await refresh();
    },
    setSettings: async (x) => {
      try { settings = (await call<BrowserSettings>('PUT', BROWSER_SETTINGS, x)) ?? settings; sinks.problem(null); publish(); } catch (e) { sinks.problem((e as Error).message); }
      await refresh();
    },
    close: () => { closed = true; following?.(); unNav(); },
  };
}
