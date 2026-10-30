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
  Tetrix,
  TetrixTarget,
  Turn,
} from '../../shared/obid-wire';

export * from '../../shared/obid-wire';

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
export class ApiError extends Error {
  constructor(readonly status: number, message: string) {
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
    const said = (value as { error?: unknown } | null)?.error;
    throw new ApiError(status, typeof said === 'string' ? said : `Oby answered ${status}.`);
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

export const api = {
  status: () => call<Status>('GET', '/v1/status'),
  jobs: (limit = 50) => call<Job[]>('GET', `/v1/jobs${q({ limit })}`),
  job: (j: string) => call<Job>('GET', `/v1/jobs/${id(j)}`),
  stop: (j: string) => call<Job>('POST', `/v1/jobs/${id(j)}/stop`),
  dismiss: (j: string) => call<Job>('POST', `/v1/jobs/${id(j)}/dismiss`),
  message: (j: string, text: string) => call<Job>('POST', `/v1/jobs/${id(j)}/messages`, { text }),
  // a job's waiting permission (POST /v1/jobs/{id}/permission)
  permission: (j: string, body: { id: string; decision: 'allow' | 'always' | 'deny'; message?: string }) =>
    call<Job>('POST', `/v1/jobs/${id(j)}/permission`, body),
  inbox: () => call<InboxItem[]>('GET', '/v1/inbox'),
  answer: (i: string, body: { answer: string } | { later: string }) => call<InboxItem>('POST', `/v1/inbox/${id(i)}/answer`, body),
  // Confirm on a confirm item: main signs it with this Mac's device key (main/device.ts)
  confirm: (i: string) => call<InboxItem>('POST', `/v1/inbox/${id(i)}/answer`, undefined, (b) => b.confirm(i)),
  limitDecisions: () => call<LimitDecision[]>('GET', '/v1/limit-decisions'),
  answerLimit: (decision: LimitDecision, option: LimitDecision['options'][number]) =>
    call<unknown>('POST', `/v1/limit-decisions/${id(decision.id)}/answer`, undefined,
      (b) => b.limitAnswer(decision.id, decision.revision, option.kind, option.kind === 'move' ? option.account : undefined)),
  // one conversation's turns
  conversation: (opts: { id?: string; before?: string; limit?: number } = {}) => call<Turn[]>('GET', `/v1/conversation${q(opts)}`),
  // words to Oby: in a conversation, or starting a new one
  plan: (said: string, answer: { question: string; option: string } | undefined, where: { conversation: string } | { new: true }, folders?: string[]) =>
    call<PlanAnswer>('POST', '/v1/plans', { said, ...(answer ? { answer } : {}), ...where, ...(folders?.length ? { folders } : {}) }),
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
  signIn: (a: string) => call<SignIn>('GET', `/v1/accounts/${id(a)}/sign-in`),
  signInCode: (a: string, code: string) => call<Account>('POST', `/v1/accounts/${id(a)}/sign-in`, { code }),
  endSignIn: (a: string) => call<null>('DELETE', `/v1/accounts/${id(a)}/sign-in`),
  // obid only: sets a tool up on this Mac (Cursor's agent), in the background; status.tools says how it stands
  installTool: (tool: Agent) => call<NonNullable<Status['tools']>>('POST', `/v1/tools/${id(tool)}/install`),
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
