// A typed client of obid's API (docs/runner-api.md), copied from apps/web/src/api.ts and trimmed
// to what this app shows: the status, jobs, the inbox, the conversations and the event stream.
// Every call goes through main (window.app.obid, main/obid.ts), which holds obid's token and adds
// it: the page never sees it.

import type {
  Account,
  Agent,
  Conversation,
  GitHub,
  GitHubWork,
  InboxItem,
  Job,
  LimitDecision,
  PlanAnswer,
  Repo,
  SavedRule,
  SignIn,
  Status,
  TranscriptEntry,
  Tetrix,
  TetrixTarget,
  Turn,
} from '../../shared/obid-wire';

export * from '../../shared/obid-wire';

// POST /v1/pairing/invite (docs/runner-api.md, "LAN gateway"): link is JSON text
export type Invitation = { url: string; link: string; secret: string; code: string; expiresAt: string };
export type PairedDevice = { id: string; name: string; created: string; expiresAt: string };
// What the QR code carries, as the Swift Mac app made it (PairingView's qrText): the iPhone reads
// it (GatewayEnvelope). ASCII only, everything else as JSON's \uXXXX: the QR's bytes are one per
// character, and a Mac's name (Wellington’s MacBook Pro) would lose its apostrophe
export const qrText = (i: Invitation) => JSON.stringify({ url: i.url, link: JSON.parse(i.link), secret: i.secret })
  .replace(/[\u007f-\uffff]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);

// Each tool's model and efforts, as status.models has them (Settings › Agents), and the choices:
// Claude Code's efforts are the tool's, Codex's each model's own; Cursor has a model and no effort
export type Models = {
  claude: { model: string; effort: string; complexEffort: string };
  codex: { model: string; effort: string; complexEffort: string };
  cursor?: { model: string };
};
export type ModelOptions = Record<Agent, { models: { id: string; name: string; efforts?: string[] }[]; efforts?: string[]; complexEfforts: string[] }>;

// {"error": "One line a person can read."} with its status; status 0: obid didn't answer, and
// the message is why (its own reason when it has one, main/obid.ts)
// reason: obid's stable word for why (docs/contracts/sign-in.md), where it gives one
export class ApiError extends Error {
  constructor(readonly status: number, message: string, readonly reason?: string) {
    super(message);
  }
}

type Bridge = NonNullable<Window['app']>['obid'];
async function call<T>(method: string, path: string, body?: unknown, via = (b: Bridge) => b.call(method, path, body)): Promise<T> {
  const bridge = window.app?.obid;
  if (!bridge) throw new ApiError(0, 'Oby’s engine is reached from the Mac app only.');
  const { status, text } = await via(bridge);
  if (status === 0) throw new ApiError(0, text);
  let value: unknown = null;
  try { value = text ? JSON.parse(text) : null; } catch { /* not JSON: said below */ }
  if (status < 200 || status >= 300) {
    const { error: said, reason } = (value ?? {}) as { error?: unknown; reason?: unknown };
    throw new ApiError(status, typeof said === 'string' ? said : `Oby answered ${status}.`, typeof reason === 'string' ? reason : undefined);
  }
  return value as T;
}

const q = (params: Record<string, string | number | undefined>) => {
  const s = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined) s.set(k, String(v));
  const text = s.toString();
  return text ? `?${text}` : '';
};
const id = encodeURIComponent;
// obid's largest file (POST /v1/attachments)
const UPLOAD_MAX = 25_000_000;

export const api = {
  status: () => call<Status>('GET', '/v1/status'),
  jobs: (limit = 50) => call<Job[]>('GET', `/v1/jobs${q({ limit })}`),
  job: (j: string) => call<Job>('GET', `/v1/jobs/${id(j)}`),
  stop: (j: string) => call<Job>('POST', `/v1/jobs/${id(j)}/stop`),
  dismiss: (j: string) => call<Job>('POST', `/v1/jobs/${id(j)}/dismiss`),
  // obid only, the task's menu: rename (whatever it's doing), archive or unarchive (ended, needing
  // nothing), delete (ended, for good: obid's 409 says why not); a job with commits that aren't
  // published isn't deleted yet: obid's Confirm (ask) asks the owner first
  rename: (j: string, title: string) => call<Job>('PATCH', `/v1/jobs/${id(j)}`, { title }),
  archive: (j: string, archived = true) => call<Job>('POST', `/v1/jobs/${id(j)}/archive`, { archived }),
  deleteJob: (j: string) => call<{ deleted: boolean; ask: InboxItem | null }>('POST', `/v1/jobs/${id(j)}/delete`),
  // obid only: the job's session (Full session), the entries that came or changed after entry `after`
  transcript: (j: string, after?: number) => call<{ next: number | null; entries: TranscriptEntry[] }>('GET', `/v1/jobs/${id(j)}/transcript${q({ after })}`),
  // conversation: the words go into the task's conversation too, made now if it has none (a task
  // page's box: docs/contracts/task-conversations.md, TASKCONV-4)
  message: (j: string, text: string, attachments?: string[], conversation = false) => call<Job>('POST', `/v1/jobs/${id(j)}/messages`, { text, ...(attachments?.length ? { attachments } : {}), ...(conversation ? { conversation: true } : {}) }),
  // a job's waiting permission (POST /v1/jobs/{id}/permission)
  permission: (j: string, body: { id: string; decision: 'allow' | 'always' | 'deny'; message?: string }) =>
    call<Job>('POST', `/v1/jobs/${id(j)}/permission`, body),
  // how much it asks from now on (a task's Mode menu)
  mode: (j: string, mode: string) => call<Job>('POST', `/v1/jobs/${id(j)}/mode`, { mode }),
  inbox: () => call<InboxItem[]>('GET', '/v1/inbox'),
  answer: (i: string, body: { answer: string } | { later: string }) => call<InboxItem>('POST', `/v1/inbox/${id(i)}/answer`, body),
  // Confirm on a confirm item: main signs it with this Mac's device key (main/device.ts)
  confirm: (i: string) => call<InboxItem>('POST', `/v1/inbox/${id(i)}/answer`, undefined, (b) => b.confirm(i)),
  // Settings › Runner: pairing an iPhone through obid's Obysphere (main signs the invitation), the
  // paired ones, and Revoke
  pairInvite: () => call<Invitation>('POST', '/v1/pairing/invite', undefined, (b) => b.pairInvite()),
  pairedDevices: () => call<PairedDevice[]>('GET', '/v1/pairing/devices'),
  revokeDevice: (d: string) => call<null>('DELETE', `/v1/pairing/devices/${id(d)}`),
  limitDecisions: () => call<LimitDecision[]>('GET', '/v1/limit-decisions'),
  answerLimit: (decision: LimitDecision, option: LimitDecision['options'][number]) =>
    call<unknown>('POST', `/v1/limit-decisions/${id(decision.id)}/answer`, undefined,
      (b) => b.limitAnswer(decision.id, decision.revision, option.kind, option.kind === 'move' ? option.account : undefined)),
  // one conversation's turns
  conversation: (opts: { id?: string; before?: string; limit?: number } = {}) => call<Turn[]>('GET', `/v1/conversation${q(opts)}`),
  // words to Oby: in a conversation, or starting a new one. with: the words' id (sent again with
  // the same id, nothing happens twice), the files uploaded for them and the tasks attached
  plan: (said: string, answer: { question: string; option: string } | undefined, where: { conversation: string } | { new: true }, folders?: string[],
    with_: { id?: string; attachments?: string[]; jobs?: string[] } = {}) =>
    call<PlanAnswer>('POST', '/v1/plans', {
      said, ...(answer ? { answer } : {}), ...where, ...(folders?.length ? { folders } : {}), ...(with_.id ? { id: with_.id } : {}),
      ...(with_.attachments?.length ? { attachments: with_.attachments } : {}), ...(with_.jobs?.length ? { jobs: with_.jobs } : {}),
    }),
  // a file picked or pasted, kept by obid for the words it goes with: its path there; one over
  // obid's 25 MB isn't read at all, and says so in obid's words
  upload: async (file: File, name: string) => {
    if (file.size > UPLOAD_MAX) throw new ApiError(413, `${name} is larger than 25 MB, so Oby can't take it.`);
    return call<{ path: string; name: string }>('POST', `/v1/attachments${q({ name })}`, new Uint8Array(await file.arrayBuffer()));
  },
  // one uploaded for words that never went: it goes (obid keeps one that words or a task use)
  unload: (path: string) => call<null>('DELETE', `/v1/attachments${q({ path })}`),
  repos: () => call<{ repos: Repo[] }>('GET', '/v1/repos'),
  // Settings › Permissions: the mode new jobs get, and the rules Always allow saved
  permissions: () => call<{ mode: string }>('GET', '/v1/settings/permissions'),
  setPermissions: (mode: string) => call<{ mode: string }>('PUT', '/v1/settings/permissions', { mode }),
  rules: () => call<SavedRule[]>('GET', '/v1/permissions/rules'),
  forgetRule: (r: string) => call<SavedRule[]>('DELETE', `/v1/permissions/rules/${id(r)}`),
  // obid only: the owner's own check for a repository, or null to start over: obid asks for it
  // (ask, a set_check confirm item) unless there's nothing to keep
  changeCheck: (repo: string, check: string | null) =>
    call<{ repo: Repo; ask: InboxItem | null }>('POST', '/v1/repos/check', { repo, check }),
  // obid only: GitHub work, and starting work on an item (the owner's tap)
  githubWork: () => call<GitHubWork>('GET', '/v1/github/work'),
  startGitHubWork: (i: string) => call<Job>('POST', `/v1/github/work/${id(i)}/start`),
  // obid only: accounts, and signing them in with the tool's own login
  addAccount: (body: { agent: Agent; default: true } | { agent: Agent; name: string; payer?: string }) => call<Account>('POST', '/v1/accounts', body),
  removeAccount: (a: string) => call<null>('DELETE', `/v1/accounts/${id(a)}`),
  // obid only, Settings › Agents: an account's model and efforts (null: the tool's own), the
  // choices, and the account new tasks go to (null: routing's own order)
  tuneAccount: (a: string, change: { model?: string | null; effort?: string | null; complexEffort?: string | null }) =>
    call<Account>('PATCH', `/v1/accounts/${id(a)}`, change),
  modelOptions: () => call<ModelOptions>('GET', '/v1/settings/models/options'),
  setDefaultAccount: (account: string | null) =>
    call<{ defaultAccount: string | null }>('PUT', '/v1/settings/jobs', { defaultAccount: account }),
  signIn: (a: string) => call<SignIn>('GET', `/v1/accounts/${id(a)}/sign-in`),
  signInCode: (a: string, code: string) => call<Account>('POST', `/v1/accounts/${id(a)}/sign-in`, { code }),
  endSignIn: (a: string) => call<null>('DELETE', `/v1/accounts/${id(a)}/sign-in`),
  // obid only: GitHub's sign-in (gh's device flow), and Tetrix's
  github: () => call<GitHub>('GET', '/v1/github'),
  githubSignIn: () => call<SignIn>('POST', '/v1/github/sign-in'),
  githubCancel: () => call<null>('DELETE', '/v1/github/sign-in'),
  tetrix: () => call<Tetrix>('GET', '/v1/tetrix'),
  tetrixSignIn: (to: TetrixTarget) => call<SignIn>('POST', '/v1/tetrix/sign-in', to),
  tetrixCancel: () => call<null>('DELETE', '/v1/tetrix/sign-in'),
  // signs Oby out of Tetrix (its refresh token revoked where the server offers it)
  tetrixDisconnect: () => call<null>('DELETE', '/v1/tetrix'),
  conversations: (opts: { after?: string; limit?: number } = {}) => call<Conversation[]>('GET', `/v1/conversations${q(opts)}`),
  editConversation: (c: string, change: { title?: string; pinned?: boolean; folders?: string[] }) => call<Conversation>('PATCH', `/v1/conversations/${id(c)}`, change),
  deleteConversation: (c: string) => call<null>('DELETE', `/v1/conversations/${id(c)}`),
};
