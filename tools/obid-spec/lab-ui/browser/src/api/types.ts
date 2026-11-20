// The screens' view of the browser: what each board draws, built from lane A's contract
// (../contract.ts, owned by lane A and never edited here) in live mode, and from fixtures.ts-like
// data (boards.tsx) in fixtures mode. Where the contract names a thing, this file uses its type.
import type { Settings as ContractSettings, TabState as ContractTabState, AskKind as ContractAskKind } from '../contract';

export type AgentId = 'codex' | 'claude' | 'cursor';
export const AGENT_NAME: Record<AgentId, string> = { codex: 'Codex', claude: 'Claude Code', cursor: 'Cursor' };

/** Who a tab belongs to: an agent, or you. */
export type Owner = { kind: 'agent'; agent: AgentId } | { kind: 'you' };

/** GET /v1/browser: a tab's state (the contract's). */
export type TabState = ContractTabState;

/** Another computer the browser runs on (14); none: this Mac. */
export type Computer = { name: string; os: string };

export type Tab = {
  id: string;
  owner: Owner;
  title: string;
  /** host, then path: drawn as <b>host</b>path */
  url: string;
  state: TabState;
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

/** GET /v1/browser/events: an ask (7–9) */
export type AskKind = ContractAskKind;
/** `field`: the page's field a secret goes into (the contract's Ask.field) */
export type Ask = { id: string; tab: string; kind: AskKind; agent: AgentId; host: string; field?: string | null };

export type Download = {
  id: string;
  name: string;
  size?: string;
  /** an agent's (stays in its browser profile) or yours (in Downloads) */
  by: Owner;
  saved?: boolean;
};

/** settings: both on by default (13), the contract's */
export type Settings = ContractSettings;

/** How a fixture draws a tab where the boards differ (not the contract's: live mode never sets them). */
export type ScreenHints = { dot?: 'working'; basis?: number; closable?: boolean };

export type BrowserState = {
  tabs: (Tab & ScreenHints)[];
  active: string | null;
  asks: Ask[];
  downloads: Download[];
  settings: Settings;
};

/** What the screens send: the same calls in fixtures and live. */
export interface BrowserApi {
  take(tab: string): Promise<void>;
  giveBack(tab: string): Promise<void>;
  /** POST secret: typed in Oby's card, put straight into the page's field; never logged or kept */
  secret(tab: string, field: string, value: string): Promise<void>;
  saveCopy(download: string): Promise<void>;
  setSettings(s: Partial<Settings>): Promise<void>;
  select(tab: string): Promise<void>;
  /** live only: your mouse and keys on the page while you hold it (the contract's StreamInput) */
  input?(tab: string, e: import('../contract').StreamInput): Promise<void>;
}
