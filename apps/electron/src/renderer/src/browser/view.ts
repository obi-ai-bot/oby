// The browser panel's view of the browser (docs/contracts/browser-app.md): what each of the design's
// boards draws, built from obid's /v1/browser (shared/obid-wire.ts) on obid and from the demo data
// (demo.tsx) with ?everything. Moved from the lab (tools/obid-spec/lab-ui/browser/src/api/types.ts).
import type { BrowserAskKind, BrowserSettings, BrowserTabState } from '../../../shared/obid-wire';

export type AgentId = 'codex' | 'claude' | 'cursor';
export const AGENT_NAME: Record<AgentId, string> = { codex: 'Codex', claude: 'Claude Code', cursor: 'Cursor' };

/** Who a tab belongs to: an agent, or you. */
export type Owner = { kind: 'agent'; agent: AgentId } | { kind: 'you' };

/** Another computer the browser runs on (14); none: this Mac. */
export type Computer = { name: string; os: string };

export type Tab = {
  id: string;
  owner: Owner;
  /** on obid: the agent's job the tab is for */
  job?: string;
  /** on obid: the page's full address, as obid gave it, for its webview to open */
  src?: string;
  /** on obid: the session obid named for the tab's page (docs/plans/browser-t3.md); none from an obid before the relay */
  partition?: string;
  title: string;
  /** host, then path: drawn as <b>host</b>path; on obid, the tab's own webview's once main says it */
  url: string;
  state: BrowserTabState;
  /** who holds the tab now: its agent, you (Take control), or nobody (idle) */
  holder: 'agent' | 'you' | null;
  /** the page's own question, when it asks one (state dialog) */
  dialog?: string | null;
  /** what the agent is doing, as its log says it ("Clicking Sign in"); none when idle */
  action?: string;
  /** the agent's pointer, in the page's own pixels */
  pointer?: { x: number; y: number; ring?: boolean };
  computer?: Computer;
  /** is the address https (a lock) */
  secure?: boolean;
  /** each hold carries a number: an agent action started before your Take control is refused */
  hold?: number;
  /** the agent ran a script in the page, and has done nothing since (11); how long it took, when known */
  ran?: { ms?: number };
  /** on obid: main's, from the tab's own webview: a page behind this one */
  canGoBack?: boolean;
  /** on obid: main's, from the tab's own webview: a page ahead of this one */
  canGoForward?: boolean;
  /** on obid: main's, from the tab's own webview: its page is loading */
  pageLoading?: boolean;
  /** on obid: main's, from the tab's own webview: the address its last load failed at (obid's `url` is its last good one) */
  failedAt?: string;
};

/** The states a page didn't simply load in: board 10's box over the panel's middle, the page under it. */
export const STOPPED: BrowserTabState[] = ['failed', 'timedOut', 'certificate', 'crashed'];

/** The owner's Back, Forward, Reload and Stop loading on a tab's own page. */
export type NavOp = 'back' | 'forward' | 'reload' | 'stop';

/**
 * An agent's ask (7–9). `reason`: the agent's words, whose first sentence heads its card; `choices`
 * for a choice; `field`: the page's field a secret goes into; `job`: on obid, the agent's job.
 */
export type Ask = { id: string; tab: string; kind: BrowserAskKind; agent: AgentId; host: string; field?: string | null; reason?: string; choices?: string[]; job?: string };

export type Download = {
  id: string;
  name: string;
  size?: string;
  /** an agent's (stays in its browser profile) or yours (in Downloads) */
  by: Owner;
  saved?: boolean;
  /** its name in Downloads, once there (Save a copy may name it apart) */
  savedAs?: string;
};

/** How the demo data draws a tab where the boards differ (obid never sets them). */
export type ScreenHints = { dot?: 'working'; basis?: number; closable?: boolean };

export type BrowserState = {
  tabs: (Tab & ScreenHints)[];
  active: string | null;
  asks: Ask[];
  downloads: Download[];
  settings: BrowserSettings;
  /** an agent using the Mac, and what it's doing (15); desktop control's own contract brings it on obid */
  mac?: { agent: AgentId; action: string } | null;
  /** the first time an agent would use the Mac: who, and what it wants to do (16) */
  macAsk?: { agent: AgentId; wants: string } | null;
};

/** What the panel sends: the same calls on the demo data and on obid. */
export interface BrowserApi {
  take(tab: string): Promise<void>;
  giveBack(tab: string): Promise<void>;
  /** typed in Oby's card, put straight into the page's field (the ask it answers, if any); never logged or kept */
  secret(tab: string, field: string, value: string, ask?: string): Promise<void>;
  /** an ask answered: a choice, or none (Skip) */
  answer(ask: string, choice?: string): Promise<void>;
  saveCopy(download: string): Promise<void>;
  /** Show in Finder: a download in the owner's Downloads */
  reveal(download: string): Promise<void>;
  setSettings(s: Partial<BrowserSettings>): Promise<void>;
  select(tab: string): Promise<void>;
  /** Stop, or Esc twice: the agent no longer has the Mac (15) */
  stopMac(): Promise<void>;
  /** the first-time sheet answered: Continue, or Not now (16) */
  answerMac(go: boolean): Promise<void>;
  /** Back, Forward, Reload or Stop loading on the tab's own page: on obid main moves it, with no obid call */
  nav(tab: string, op: NavOp): Promise<void>;
}
