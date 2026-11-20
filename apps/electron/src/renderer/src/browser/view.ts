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
  title: string;
  /** host, then path: drawn as <b>host</b>path */
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
};

/** An agent's ask (7–9). `field`: the page's field a secret goes into. */
export type Ask = { id: string; tab: string; kind: BrowserAskKind; agent: AgentId; host: string; field?: string | null };

export type Download = {
  id: string;
  name: string;
  size?: string;
  /** an agent's (stays in its browser profile) or yours (in Downloads) */
  by: Owner;
  saved?: boolean;
};

/** How the demo data draws a tab where the boards differ (obid never sets them). */
export type ScreenHints = { dot?: 'working'; basis?: number; closable?: boolean };

export type BrowserState = {
  tabs: (Tab & ScreenHints)[];
  active: string | null;
  asks: Ask[];
  downloads: Download[];
  settings: BrowserSettings;
};

/** What the panel sends: the same calls on the demo data and on obid. */
export interface BrowserApi {
  take(tab: string): Promise<void>;
  giveBack(tab: string): Promise<void>;
  /** typed in Oby's card, put straight into the page's field; never logged or kept */
  secret(tab: string, field: string, value: string): Promise<void>;
  saveCopy(download: string): Promise<void>;
  setSettings(s: Partial<BrowserSettings>): Promise<void>;
  select(tab: string): Promise<void>;
}
