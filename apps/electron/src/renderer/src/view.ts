import { BRANDS, lettered, type Brand } from './brands';
import { isAgent, type AskMark } from '../../shared/obid-wire';

// The app's view of Oby's work: the shapes its screens draw, their fixed words and the
// helpers that read them. The demo data built from these is in mock.ts.

export type Agent = 'claude' | 'codex' | 'cursor';
export const AGENTS: Record<Agent, { name: string; brand: Brand }> = {
  claude: { name: 'Claude Code', brand: BRANDS.claude },
  codex: { name: 'Codex', brand: BRANDS.openai },
  cursor: { name: 'Cursor', brand: BRANDS.cursor },
};
// An agent by obid's id: one this build doesn't know (docs/contracts/wire.md, WIRE-4) by that id,
// with the gray circle lettered with it (docs/contracts/providers.md, PROVIDERS-5)
export const agentOf = (id: string): { name: string; brand: Brand } => (isAgent(id) ? AGENTS[id] : { name: id, brand: lettered(id) });

// Job.status on the Mac. queued and running show as running here; a job
// with an ask needs you whatever its status.
export type JobStatus = 'queued' | 'running' | 'done' | 'failed' | 'stopped' | 'interrupted';

// JobStage (Engine/Pipeline.swift): prepare → work → check → publish → review → merged
export type StageId = 'prepare' | 'work' | 'check' | 'publish' | 'review' | 'merged';
export type StageState = 'done' | 'current' | 'upcoming' | 'failed' | 'skipped';
// On the task's page, a stage also says how long it took ("3m 19s") or should
// take ("≈ 3m"), what it's doing in a plain sentence (sub), and what hangs
// under it on the timeline: the job's activity (under Work) or its checks.
export type Stage = {
  id: StageId; title: string; detail?: string; state: StageState | (string & {});
  took?: string; sub?: string; activity?: boolean; checks?: CheckRow[];
};

// The job page's own data (obid's job fields, obi-ai-bot/obi#739); every part
// is optional, and a part with nothing to show isn't drawn.
// A tool call: the verb and what it touched ("Running" `xcodebuild test`).
export type ToolCall = { verb: string; path?: string; text?: string; took?: string; running?: boolean };
// A few plain groups of the agent's work; only the running one opens.
export type ToolGroup = { title: string; state: 'done' | 'running' | 'failed' | (string & {}); took?: string; meta?: string; tools?: ToolCall[] };
// A check (or the secret scan, or the push) under its stage.
export type CheckRow = {
  name: string; mono?: boolean; state: 'done' | 'running' | 'upcoming' | 'failed' | 'skipped';
  result?: string; took?: string; progress?: number;
};

export type Page = {
  mode?: string;  // Ask, Accept edits, Plan, Bypass
  // the time track: how far in (done), when it should be done (from–to), as
  // % of the track; paused while it waits on you; basis: what the range is from
  time?: { aside: string; done?: number; from?: number; to?: number; paused?: boolean; basis?: string };
  counts?: [number, string][];   // tools by kind: [23, 'read']
  groups?: ToolGroup[];
  // this job's tokens and value, and its share of the account's windows
  usage?: { tokens: string; cost: string; fiveHour: number; job5: string; week: number; jobWeek: string };
  outgoing?: { text: string; at: string }[];  // what went out, every outgoing action
};

// What the job needs from you (InboxItem + Question + Permission). Buttons
// per kind follow Inbox.swift; every ask also has Later.
// back: a job that was on someone else's Mac came back to yours (Sharing, a preview); check: which
// command proves it done (obid's set_check); remember: may Oby keep what it learned (a workflow)
export type AskKind = 'question' | 'permission' | 'review' | 'failed' | 'limit' | 'start' | 'merge' | 'back' | 'check' | 'remember' | 'confirm';
export type Ask = {
  kind: AskKind;
  headline: string;   // Oby's plain-words rewrite, e.g. "Pick one vault"
  text: string;       // what happened, or the agent's question
  options: string[];
  recommended?: number;
  detail?: string;    // the command, the diff stat, the plan
  label?: string;     // over the headline, in place of the kind's (a merge card's own: Ready to merge)
  always?: string;    // what Always allow saves, in obid's words: on its button's hover (proposed, Sangam's to change)
  repos?: { offered: string[]; chosen: string[] };  // the repositories to tick, on the card that asks to share (PEER-38)
  // Oby's escalation (orchestrator-): whole text on the stack (TASKS-17); with a job, the box
  // that steers it (never a button of the model's recommendation)
  fullText?: boolean;
  steer?: boolean;
  // obid's kind this build doesn't know (docs/contracts/wire.md, WIRE-2): its kind as the label, gray, nothing to answer
  unknown?: boolean;
};
// a Question's own words for the agent, in place of its options (as on his "Go with this plan?")
export const TELL = 'Tell it something';
// Oby's escalation: the owner's words to that task, as its page's box (TASKS-17)
export const STEER = 'Tell it what to do';
export const ASK_LABEL: Record<AskKind, string> = {
  question: 'Question', permission: 'Permission', review: 'Out of reviews', failed: 'Failed',
  limit: 'Out of usage', start: 'Start it?', merge: 'Not merged', back: 'Came back', check: 'Check', remember: 'Remember?',
  // obid's: what Confirm does is the card's title (live.ts)
  confirm: 'Confirm?',
};

// One line of the agent's session: a file it read, a command it ran, what it said.
export type LogLine = { tool: 'Read' | 'Edit' | 'Bash' | 'Grep' | 'Say'; text: string; ago: number };

// what a queued job waits for: `kind` as the runner's (cap, update, schedule, limit), `text`
// what follows "Waiting"
export type Waiting = { kind: string; text: string };

export type Job = {
  id: string;
  type: string;        // which chip / side-panel group it's in (TYPES in tasks.ts)
  title: string;       // the brain's short title
  who?: string;        // the person it's for, shown before the title
  from?: Brand;        // where it came from (GitHub, Slack, Gmail…); none when you asked Oby
  request: string;     // your own words, or the message it came from
  agent: Agent | (string & {});  // obid's; one this build doesn't know shows by agentOf
  account: string;     // which of the accounts it runs on (Personal, Deskree, or someone's shared one)
  on?: string;         // whose Mac it runs on, when not this one ("Wellington’s Mac")
  model: string;
  repo: string;
  branch: string;
  base: string;
  check?: string;      // "Done when": the command that has to pass
  repoCheck?: string | null;  // obid only: its repository's check now, which Change check starts from
  status: JobStatus | (string & {});
  step: string;        // the live line, e.g. "Running swift test"
  waiting?: Waiting;   // why a queued job hasn't started, as the runner says it (#238)
  round?: [number, number];
  stages: Stage[];
  ask?: Ask;
  summary?: string;    // the agent's report once it's done
  reason?: string;     // obid's: why it stopped, in its words
  canTryAgain?: boolean;  // failed or interrupted, not let go, nothing carrying it on: its menu's Try again and Stop here (TASKS-25)
  commits?: number;
  published?: string;  // the pull request it opened
  publish?: { label: string; anyway?: true; why?: string };  // obid only: Publish by hand (jobPage.ts, publishOf)
  nothingDone?: boolean;  // obid's: it ended with nothing done (TASKS-12)
  log?: LogLine[];
  page?: Page;         // the job page's extra data, once obid serves it
  conversation?: string;  // obid's: the conversation whose words started it
  noConversation?: boolean;  // obid's: no conversation to hold its card (never had one, or it was deleted)
  archived?: boolean;  // obid's: archived (POST /v1/jobs/{id}/archive), in Archive whenever it ended
  created: number;     // minutes ago
  finished?: number;   // minutes ago
};

// An ended task's mark when it isn't a finished one's check (Sangam's board "Stuck and failed")
export const ENDED: Partial<Record<Job['status'], string>> = { failed: 'Failed', stopped: 'Stopped', interrupted: 'Stopped' };
// A failed or interrupted task's plain line (TASKS-25): the first line of obid's reason, else of
// its summary, else its step
export const stoppedLine = (j: Job): string | undefined =>
  j.status === 'failed' || j.status === 'interrupted' ? (j.reason || j.summary || '').split('\n')[0] || j.step : undefined;

// GitHub's work no job has yet (obid's GET /v1/github/work): a review asked of
// you, an issue assigned to you, each with its project board's column when it's
// on one. Under To do in Review PR and Issues until you start it or put it off.
export type GitHubItem = {
  id: string;
  type: 'review' | 'issues';
  who?: string;        // a review's author, before the title
  title: string;       // "#412 retry jitter"
  repo: string;        // "tetrix-ee"
  request: string;     // the headline
  action?: 'Review it' | 'Fix it';  // none where Oby can't start on it (live.ts)
  label?: string;      // over the headline, when it isn't "Start it?"
  note?: string;       // under it: why Oby can't start on it
  url?: string;        // its page on GitHub
  board?: { project: string; column: string };
  updated: number;     // minutes ago
};

// Done and out of Home: the archive (Home's Archived), most recent first.
export type Archived = { id: string; type: string; title: string; who?: string; agent: Agent | (string & {}); finished: number };

// The conversation with Oby (Turn): your words and Oby's replies, a day at a
// time. jobs: what that message started, shown as job cards under it.
export type Turn = { id: string; who: 'user' | 'oby'; text: string; at: number; jobs?: string[] };

// A task's own conversation, oldest first: what started it (your words, or the
// message it came in on), Oby's replies, and what you told the agent since.
// who: you, oby, the source it came in on (GitHub, Slack…), or peer: someone
// else's Oby joining in (by: whose). jobs: the tasks that message started (or
// tasks attached to it), shown as cards under it. steps: what Oby did on the
// way, folded over its words (looked: how many were lookups; asked: whose Obys
// it asked); streaming: its words still coming in. files: what went with yours
// (a photo with a preview of itself); obys: the Obys it @mentions
export type Message = { who: string; text: string; ago: number; jobs?: string[]; by?: string; steps?: string[]; looked?: number; asked?: string[]; streaming?: boolean; setup?: SetupStep; offer?: Offer; confirm?: { id: string; ask: Ask }; askCard?: AskMark } & Attachments;
// askCard (obid only): a question to another Oby, one card that updates in place (AskCard.tsx)
// confirm (obid only): a Confirm Oby's answer made (a merge, a write on GitHub), answered here or
// on Home, one item (#1680)
// Work Oby offers to start, on obid only (OfferCard.tsx): its title, each job in obid's words with
// the task Oby planned, and whether Oby had read something untrusted first
export type Offer = { id: string; title: string; jobs: { what: string; task: string }[]; untrusted: boolean };
// a step of setting up, in Oby's first conversation (onboarding.ts): its card under Oby's words.
// sphere to knowledge: the Obysphere tour (convo: a sample teammate's conversation)
export type SetupStep = 'agents' | 'github' | 'extras' | 'first' | 'sphere' | 'showme' | 'sample' | `convo:${'Ernesto' | 'Dima' | 'Wellington'}` | 'knowledge';
// What went with a message from the message box besides its words
// repo: the repository picked for it (obid's, Composer's repository tag)
// folders: the working folders picked (folders.ts: obid's once it takes them, else the app's)
// a file's file: what was picked or pasted; path: where obid keeps it, once uploaded (live.ts)
export type Attachments = { files?: { name: string; preview?: string; file?: File; path?: string }[]; obys?: string[]; jobs?: string[]; on?: string; repo?: string; folders?: string[] };
export const hasAttachments = (a?: Attachments) => !!(a?.files?.length || a?.obys?.length || a?.jobs?.length);

// The Obysphere: the other Obys in your workspace (whose they are), and your conversations with them, each about a task. who: 'you',
// or the owner of the Oby that said it; with: the Obys in it (more than one: a
// group); task: the task a message is about, shown as its card under it.

export type Peer = { name: string };

// Where a job can run (the message box's Run on): this Mac, or the Mac of
// someone in your Obysphere who shares theirs. free: it would take a job now;
// about: why, or why not, in a few words (free or busy, never whether its
// owner is at it). A job there still runs on your own
// accounts: your login stays on this Mac, and their Mac reaches your AI
// account only through your Oby, while the job runs.
export type Place = { name: string; owner?: string; about: string; free: boolean };

// A part of someone's usage shared with you: an account their company pays by
// use (a personal plan or a Team seat can't be shared), how much of its month
// is yours (%), and how much of it you've used. Jobs on it run on their Mac:
// its key never leaves it.
export type Share = { owner: string; agent: Agent; account: string; share: number; used: number; resets: string };

// who: 'you', 'oby' (your Oby, answering for you when another Oby asks it
// something it can answer itself), or the person whose Oby said it
export type PeerMessage = { who: string; text: string; ago: number; task?: string; streaming?: boolean } & Attachments;
// unread: something in it you haven't opened yet (a blue dot on it, and on its Obys)
export type PeerConvo = { id: string; title: string; with: string[]; messages: PeerMessage[]; unread?: boolean };

// how long ago: now, minutes under an hour, hours under a day, then days
export const ago = (min: number) => (min < 1 ? 'now' : min < 60 ? `${Math.round(min)}m` : min < 24 * 60 ? `${Math.floor(min / 60)}h` : `${Math.floor(min / 1440)}d`);
