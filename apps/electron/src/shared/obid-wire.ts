// Shared wire types and Server-Sent Events framing for obid's API (docs/runner-api.md),
// used by both Electron's main process (main/notify.ts) and renderer (renderer/src/obid.ts).

export type Agent = 'claude' | 'codex' | 'cursor';
export type JobStatus = 'queued' | 'running' | 'done' | 'failed' | 'stopped' | 'interrupted';
export type StageState = 'done' | 'current' | 'upcoming' | 'failed' | 'skipped';
export type Stage = { id: string; title: string; state: StageState; detail?: string | null };

// "Job": the keys this app reads
export type Job = {
  id: string;
  request: string;
  agent: Agent;
  account: string;
  repo: string;
  branch: string;
  base: string | null;
  issue: number | null;
  kind: string;
  status: JobStatus;
  step: string;
  summary: string;
  created: string;
  finished: string | null;
  commits: number | null;
  published: string | null;
  rounds: number;
  round: number;
  check: string | null;
  model: string | null;
  dismissed: string | null;
  permission: { id: string; tool: string; title: string; detail: string; rule: string | null; always: string | null } | null;
  pipeline: { number: number; total: number; stages: Stage[] } | null;
  title: string;
  waiting?: { kind: string; text: string } | null;
  // The account-wide question is shown once. Its settled choice remains authoritative while
  // the job is stopped, so an ordinary interruption card must not ask the owner again.
  accountLimit?: { decision: string | null; state: 'pending' | 'resume' | 'stop' | 'move'; resetAt: string | null } | null;
  // obid only: the conversation whose words started it (#493), and the user's turn there whose
  // words started it or offered it (oby_offer)
  conversation?: string | null;
  turn?: string | null;
};

// "Inbox item"
export type InboxItem = {
  id: string;
  kind: string;
  job: string;
  // obid only: how it's sorted (what needs the owner first), the GitHub item a github offer is
  // about (its title quoted, people's words), and the review a listen item posts on Approve
  triage?: 'needs_you' | 'fyi' | 'ignore' | 'pending';
  github?: { list: string; repo: string; number: number; author: string | null; url: string; quoted?: { title?: string } | null } | null;
  watch?: { review?: string | null } | null;
  title: string;
  text: string;
  options: string[];
  details: string[] | null;
  created: string;
  state: 'open' | 'later' | 'done';
  later: string | null;
  headline: string | null;
  recommended: number | null;
  // obid only: the conversation a start item was offered in, and the offer (oby_offer): the card
  // it's on there (the user's turn it was offered in), obid's words for the job it starts, the
  // task it planned, and whether the turn had read something untrusted first
  conversation?: string | null;
  offer?: { id: string; what: string; task: string; untrusted: boolean } | null;
  // obid only: what a confirm item's Confirm carries out; set_check's target is the repository,
  // its payload the command (null: take `was` out) and the check it has now (Oby's ask, #1062)
  confirm?: { action: string; target: string; payload: { check?: string | null; was?: string | null } | null } | null;
};

// One owner decision for all jobs stopped by the same account's limit (#1563).
export type LimitDecision = {
  id: string; revision: number; account: string; agent: string; resetAt: number | null;
  jobs: { id: string; title: string }[];
  options: ({ kind: 'move'; account: string; agent: string; name: string } | { kind: 'resume' | 'stop' })[];
};

// "GET /v1/conversation"
// ended: the turn that tells how a job ended; routine: a routine's run's (neither answers you)
export type Turn = { id: string; who: 'user' | 'oby'; text: string; at: string; jobs: string[]; conversation?: string; ended?: boolean; routine?: unknown };
// obid only: conversations
// tasks: the jobs it started
export type Conversation = { id: string; title: string; pinned: boolean; created: string; lastActivity: string; cursor: string; tasks?: string[] };

// obid only: "GET /v1/github/work", what GitHub wants from the owner, the keys this app reads.
// Everything in an item is GitHub's data: shown as text, never acted on.
export type GitHubWorkItem = {
  id: string; list: 'reviews' | 'assigned' | 'mine'; repo: string; number: number; title: string; url: string; author: string | null;
  updated: string; draft: boolean | null; checks: string | null; reviewDecision: string | null; fork?: boolean | null;
  board: { project: string; column: string } | null; known: boolean; job: string | null;
};
export type GitHubWork = { signedIn: boolean; error: string | null; reviews: GitHubWorkItem[]; assigned: GitHubWorkItem[]; mine: GitHubWorkItem[] };

// "Account" and "GET /v1/status", the keys this app reads
export type Account = {
  id: string;
  agent: Agent;
  name: string;
  payer?: string | null;
  signedIn: boolean;
  // what the tool says about the login; signed out, what obid says to do ("Run codex once")
  label?: string | null;
  fiveHour: number | null;
  sevenDay: number | null;
  sevenDayResets: string | null;
  running: number;
};
export type Status = {
  runner: { version: string; host: string; engine?: string };
  accounts: Account[];
  // obid only: the tools' own logins found on this Mac that aren't accounts yet
  availableLogins?: { agent: Agent; id: string; name: string }[];
  models: Record<string, { model: string }>;
  maxRunning: number;
  running: number;
};

// obid only: "GET /v1/repos", the repositories a job may start in: the listed ones first, then
// GitHub's (newest push first)
// check: the one its jobs run (none: null); noCheck: the owner said No check (#1062)
export type Repo = { repo: string; defaultBranch: string; listed: boolean; check?: string | null; noCheck?: boolean };

// A rule Always allow saved (or a deny), for a repository or `conversations` ("GET /v1/permissions/rules")
export type SavedRule = { id: string; rule: string; repo: string; decision: 'allow' | 'deny' };

// A sign-in under way: the page to open, and the code to type there (Codex's, GitHub's) or, with
// pasteCode, the code that page shows, to send back (Claude Code's)
export type SignIn = { account?: string; url: string; userCode?: string | null; pasteCode?: boolean; expiresAt?: string };

// obid only: "GET /v1/github" (its gh, and as whom it's signed in) and "GET /v1/tetrix" (the server
// Oby signs in to, kept after a sign-out; error: why the last sign-in or refresh failed), the keys
// this app reads. No token is in either.
export type GitHub = { gh: boolean; signedIn: boolean; user: string | null; signIn: SignIn | null };
export type Tetrix = {
  connected: boolean;
  url: string | null;
  preset: string | null;
  presets: { id: string; name: string; url: string }[];
  connectedAt: string | null;
  // the sign-in under way: its page, and the sign-in server it's on
  signIn: { url: string; server: string } | null;
  error: string | null;
};
export type TetrixTarget = { preset: string } | { url: string };

// "POST /v1/plans"
export type Question = { text: string; options: string[]; recommended: number | null };
export type PlanAnswer = (
  | { kind: 'reply'; text: string; jobs: Job[] }
  | { kind: 'question'; said: string; question: Question; jobs: Job[] }
  | { kind: 'none'; jobs: Job[] }
) & { conversation?: string };

// "GET /v1/events": what a turn of the conversation is doing as it happens. step: an action of
// the turn (`turn` is the user's turn); thinking: its brain at work; delta: a piece of its words
// as they're written (obid only: the conversation harness)
export type Step = { turn: string; conversation?: string; index: number; text: string; state: 'running' | 'done' | 'failed' };
export type Delta = { turn: string; conversation: string; text: string };

// The events this app follows; any other is ignored.
export type Event =
  | { kind: 'job'; job: Job }
  | { kind: 'jobGone'; id: string }
  | { kind: 'inbox'; item: InboxItem }
  | { kind: 'turn'; turn: Turn }
  | { kind: 'step'; step: Step }
  | { kind: 'delta'; delta: Delta }
  | { kind: 'accounts'; accounts: Account[] }
  | { kind: 'conversation'; conversation: Conversation }
  | { kind: 'conversationGone'; id: string }
  | { kind: 'github'; work: GitHubWork };

// Server-Sent Events, as the stream's text comes in a piece at a time: each event's name and data
// to `onEvent`; comments (`: ping`) and other fields are skipped (apps/web/src/api.ts).
export function sseReader(onEvent: (name: string, data: string) => void) {
  let buffer = '', name = '', data: string[] = [];
  const line = (text: string) => {
    if (text === '') {
      if (data.length) onEvent(name || 'message', data.join('\n'));
      name = ''; data = [];
      return;
    }
    if (text.startsWith(':')) return;
    const at = text.indexOf(':');
    const field = at < 0 ? text : text.slice(0, at);
    let value = at < 0 ? '' : text.slice(at + 1);
    if (value.startsWith(' ')) value = value.slice(1);
    if (field === 'event') name = value;
    else if (field === 'data') data.push(value);
  };
  return (chunk: string) => {
    buffer += chunk;
    // a \r at the very end may be the first half of \r\n: wait for the next piece
    const lines = buffer.split(/\r\n|\r(?!$)|\n/);
    buffer = lines.pop() ?? '';
    lines.forEach(line);
  };
}

// One event from the stream, or null for one this app doesn't follow (or not the shape the API says)
export function parseEvent(name: string, data: string): Event | null {
  let value: unknown;
  try { value = JSON.parse(data); } catch { return null; }
  if (!value || typeof value !== 'object') return null;
  const v = value as Record<string, unknown>;
  switch (name) {
    case 'job':
      if (typeof v.id !== 'string') return null;
      if (v.deleted === true) return { kind: 'jobGone', id: v.id };
      return typeof v.title === 'string' && typeof v.status === 'string' ? { kind: 'job', job: value as Job } : null;
    case 'inbox': return typeof v.id === 'string' ? { kind: 'inbox', item: value as InboxItem } : null;
    case 'turn': return typeof v.id === 'string' ? { kind: 'turn', turn: value as Turn } : null;
    // a step of the turn, and its brain's thinking: both say what it's on
    case 'step': case 'thinking': return typeof v.text === 'string' && typeof v.turn === 'string' ? { kind: 'step', step: value as Step } : null;
    case 'delta': return typeof v.text === 'string' && typeof v.conversation === 'string' ? { kind: 'delta', delta: value as Delta } : null;
    case 'accounts': return Array.isArray(value) ? { kind: 'accounts', accounts: value as Account[] } : null;
    case 'conversation':
      if (typeof v.id !== 'string') return null;
      if (v.deleted === true) return { kind: 'conversationGone', id: v.id };
      return typeof v.title === 'string' && typeof v.lastActivity === 'string' ? { kind: 'conversation', conversation: value as Conversation } : null;
    case 'github': return typeof v.signedIn === 'boolean' && Array.isArray(v.reviews) ? { kind: 'github', work: value as GitHubWork } : null;
    default: return null;
  }
}
