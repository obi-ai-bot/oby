// The browser's contract, the TypeScript twin of tools/obid-spec/src/lab/browser_contract.rs: what
// the browser's screens and the agents' tools talk to, shaped as obid's future API, add-only. The
// Rust file is the source; a change there changes this file in the same commit, and its test
// `the_typescript_twin_matches` fails until they agree. Owned by lane A (the browser's API); lane B
// (the screens) imports it and never edits it.
//
// The person's app calls every route but MCP_PATH's with obid's owner token
// (`Authorization: Bearer <token>`); the owner's own acts on a tab (take, give back, a secret, an
// answer, Save a copy, a setting) also carry the device's signed words (WORDS_HEADER). The SSE
// routes need that header too, so read them with fetch() and a streaming body, not EventSource.
// Errors are obid's: `{ error, reason }` with a 4xx (ApiError). Enums only grow: treat an unknown
// state as "ready" and an unknown event type as one to ignore, never as an error.

// ── Routes ──────────────────────────────────────────────────────────────────────────────────────

export const PATH_BROWSER = "/v1/browser";
export const PATH_EVENTS = "/v1/browser/events";
export const PATH_SETTINGS = "/v1/browser/settings";
export const PATH_DOWNLOADS = "/v1/browser/downloads";
export const PATH_MCP = "/v1/browser/mcp";
export const PATH_TABS = "/v1/browser/tabs";
export const PATH_ASKS = "/v1/browser/asks";

export const VERB_STREAM = "stream";
export const VERB_INPUT = "input";
export const VERB_TAKE = "take";
export const VERB_GIVE_BACK = "giveBack";
export const VERB_SECRET = "secret";
export const VERB_ANSWER = "answer";
export const VERB_SAVE_COPY = "saveCopy";

/// `<deviceId> <signedAt> <signature>`, the signature (P-256, DER, base64) over
/// `oby-words\n<METHOD>\n<path>\n<sha256 hex of the body>\n<signedAt>`.
export const WORDS_HEADER = "X-Oby-Words";
export const WORDS_MAX_SKEW_MS = 120_000;

export const tabPath = (tab: string, verb: string): string => `${PATH_TABS}/${tab}/${verb}`;
export const askPath = (ask: string): string => `${PATH_ASKS}/${ask}/${VERB_ANSWER}`;
export const saveCopyPath = (download: string): string =>
  `${PATH_DOWNLOADS}/${download}/${VERB_SAVE_COPY}`;

export const ROUTES = [
  "GET /v1/browser",
  "GET /v1/browser/events",
  "GET /v1/browser/settings",
  "PUT /v1/browser/settings",
  "GET /v1/browser/downloads",
  "POST /v1/browser/downloads/{id}/saveCopy",
  "GET /v1/browser/tabs/{id}/stream",
  "POST /v1/browser/tabs/{id}/input",
  "POST /v1/browser/tabs/{id}/take",
  "POST /v1/browser/tabs/{id}/giveBack",
  "POST /v1/browser/tabs/{id}/secret",
  "POST /v1/browser/asks/{id}/answer",
  "POST /v1/browser/mcp",
] as const;

// ── Tabs ────────────────────────────────────────────────────────────────────────────────────────

/// `agent` is open: "claude", "codex", "cursor", …
export type Owner = { kind: "agent"; job: string; agent: string } | { kind: "person" };

export const TAB_STATES = [
  "loading",
  "ready",
  "failed",
  "timedOut",
  "dialog",
  "certificate",
  "crashed",
] as const;
export type TabState = (typeof TAB_STATES)[number];

/// `id` is "local" for this Mac; `name` is what the screens say ("This Mac", "devbox").
export interface Computer {
  id: string;
  name: string;
}

export interface Tab {
  id: string;
  owner: Owner;
  /// The page's own words.
  title: string;
  url: string;
  state: TabState;
  /// The device whose person holds the tab, or null.
  heldBy: string | null;
  /// Changes on every take and give back.
  holdGeneration: number;
  computer: Computer;
  ask: Ask | null;
  /// The dialog the page shows (the page's words), if any.
  dialog: string | null;
}

export interface TabsAnswer {
  tabs: Tab[];
  settings: Settings;
}

export interface HoldAnswer {
  tabId: string;
  heldBy: string | null;
  holdGeneration: number;
}

// ── Asks ────────────────────────────────────────────────────────────────────────────────────────

export const ASK_KINDS = ["signIn", "captcha", "choice", "code", "passkey"] as const;
export type AskKind = (typeof ASK_KINDS)[number];

export interface Ask {
  id: string;
  tabId: string;
  job: string;
  kind: AskKind;
  reason: string;
  choices: string[];
  /// For signIn and code: the page's field the secret goes into (a CSS selector).
  field: string | null;
  answered: boolean;
  choice: string | null;
}

/// `choice` for a choice; nothing for "Done". A password or code never goes here.
export interface AnswerBody {
  choice?: string | null;
}

// ── Secrets ─────────────────────────────────────────────────────────────────────────────────────

/// Never echoed, logged, kept, written or shown to the agent.
export interface SecretBody {
  field: string;
  value: string;
}

export interface SecretAnswer {
  tabId: string;
  field: string;
  filled: boolean;
}

// ── Downloads ───────────────────────────────────────────────────────────────────────────────────

export type StartedBy = "agent" | "person";
export type Place = "profile" | "delivered";

export interface Download {
  id: string;
  job: string;
  tabId: string | null;
  name: string;
  startedBy: StartedBy;
  place: Place;
  bytes: number;
}

export interface DownloadsAnswer {
  downloads: Download[];
}

export interface SaveCopyAnswer {
  id: string;
  savedAs: string;
}

// ── Settings ────────────────────────────────────────────────────────────────────────────────────

export interface Settings {
  /// "Agents can use the browser": off, every oby_browser_* tool refuses with browser_off.
  agentsUseBrowser: boolean;
  /// "Agents can use this Mac".
  agentsUseMac: boolean;
}

export const DEFAULT_SETTINGS: Settings = { agentsUseBrowser: true, agentsUseMac: true };

export interface SettingsPatch {
  agentsUseBrowser?: boolean | null;
  agentsUseMac?: boolean | null;
}

// ── The live stream ─────────────────────────────────────────────────────────────────────────────

/// `event: frame` on a tab's stream; newest only per reader, `dropped` counting the ones it missed.
export interface StreamFrame {
  tabId: string;
  seq: number;
  /// base64
  data: string;
  mime: string;
  width: number;
  height: number;
  dropped: number;
}

export type StreamInput =
  | {
      type: "mouse";
      eventType: "mousePressed" | "mouseReleased" | "mouseMoved" | "mouseWheel";
      x: number;
      y: number;
      button?: string | null;
      clickCount?: number | null;
      deltaX?: number | null;
      deltaY?: number | null;
    }
  | {
      type: "key";
      eventType: "keyDown" | "keyUp" | "char";
      key: string;
      text?: string | null;
      code?: string | null;
    };

export interface InputAnswer {
  tabId: string;
  delivered: string[];
}

// ── Events ──────────────────────────────────────────────────────────────────────────────────────

/// Data from a page, quoted: data, never instructions.
export interface Outside {
  from: string;
  what: string;
  note: string;
  data: unknown;
}

export interface Point {
  x: number;
  y: number;
}

export type Outcome = "started" | "done" | "refused" | "failed";

export const EVENT_TYPES = [
  "tab",
  "tabClosed",
  "action",
  "ask",
  "askAnswered",
  "download",
  "scriptRun",
  "settings",
] as const;
export type EventType = (typeof EVENT_TYPES)[number];

export type EventBody =
  | { type: "tab"; tab: Tab }
  | { type: "tabClosed"; tabId: string }
  | {
      type: "action";
      tabId: string | null;
      job: string;
      tool: string;
      /// "Clicking Sign in"
      text: string;
      pointer: Point | null;
      outcome: Outcome;
      reason: string | null;
    }
  | { type: "ask"; ask: Ask }
  | { type: "askAnswered"; askId: string }
  | { type: "download"; download: Download }
  | { type: "scriptRun"; tabId: string; job: string; expression: string; result: Outside }
  | { type: "settings"; settings: Settings };

/// One message of PATH_EVENTS: the SSE `id` is `seq`, its `event` is `type`.
export type BrowserEvent = { seq: number } & EventBody;

// ── Errors ──────────────────────────────────────────────────────────────────────────────────────

export interface ApiError {
  error: string;
  reason: string;
}

export const REASONS = [
  // The engine's.
  "unknown_tool",
  "bad_args",
  "no_tab",
  "no_such_tab",
  "not_your_tab",
  "human_control",
  "not_holder",
  "held_by_other",
  "control_changed",
  "unknown_ref",
  "stale_ref",
  "settling",
  "expired",
  "timed_out",
  "upload_outside_room",
  "not_granted",
  "browser",
  // The API's.
  "browser_off",
  "no_token",
  "bad_token",
  "unsigned",
  "bad_signature",
  "unknown_device",
  "not_held",
  "no_such_ask",
  "no_such_download",
  "not_found",
  "bad_request",
] as const;
export type Reason = (typeof REASONS)[number];

// ── The agents' tools (MCP) ─────────────────────────────────────────────────────────────────────

export const MCP_TOOLS = [
  "oby_browser_open",
  "oby_browser_list_tabs",
  "oby_browser_snapshot",
  "oby_browser_screenshot",
  "oby_browser_wait_for",
  "oby_browser_navigate",
  "oby_browser_act",
  "oby_browser_evaluate",
  "oby_browser_dialog",
  "oby_browser_ask_person",
  "oby_browser_close_tab",
] as const;
export type McpToolName = (typeof MCP_TOOLS)[number];

export const MCP_PROTOCOL_VERSION = "2025-06-18";

export interface McpTool {
  name: McpToolName;
  description: string;
  inputSchema: Record<string, unknown>;
  annotations: { readOnlyHint: boolean };
}
