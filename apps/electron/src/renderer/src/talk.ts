// Which account a conversation talks through (docs/contracts/conversation-agent.md), as the chat
// window's pill and its menu show it: what obid says, in words. obid decides; this only reads.
import { AGENTS, type Agent } from './mock';
import { fullness, type Account } from './SettingsView';

// obid's word for "whichever has room"
export const AUTO = 'auto';
// where the mock keeps Settings' choice for new conversations; obid keeps its own
export const TALK_DEFAULT = 'talk.default';
const TOOLS: Agent[] = ['claude', 'codex', 'cursor'];

// the fuller of its week and its 5-hour session, the limit that hits first; none: it reports none,
// and no number is made up
export const percent = (a: Account) => (a.used == null && a.session == null ? null : Math.max(a.used ?? 0, a.session ?? 0));
export const usageWords = (a: Account) => {
  if (a.full) return 'Full';
  const used = percent(a);
  return used == null ? 'No usage' : `${used}%`;
};
// What a person calls the tool, in the message box's picker
export const TOOL_NAME: Record<Agent, string> = { claude: 'Claude', codex: 'Codex', cursor: 'Cursor' };
export const PILL_TOOL_NAME: Record<Agent, string> = { ...TOOL_NAME, claude: 'Claude Code' };
// Until obid has read the account's own list (`read`, AGENTS-6) a saved model isn't judged: it runs,
// as obid runs it, so the row doesn't call it unavailable.
export function modelChoice(saved: string, fallback: string, options: readonly { id: string; name: string }[], read = true) {
  const offered = options.find((m) => m.id === saved) ?? (read ? undefined : { id: saved, name: saved });
  return { id: offered ? saved : fallback, name: offered?.name ?? options.find((m) => m.id === fallback)?.name ?? fallback, unavailable: !offered };
}
// The message-box model menu: this account's own list when status sent one (it differs from the
// tool options), else the tool's options (CONVERSATION-AGENT-22).
export function accountModels(a: Account, toolOptions: readonly { id: string; name: string }[] | undefined) {
  return a.models?.length ? a.models : toolOptions ?? [];
}
export function modelUsage(a: Account) {
  if (a.full) return 'Full';
  const lines = [];
  if (a.session != null) lines.push(`${a.session}% used in 5h`);
  if (a.used != null) lines.push(`${a.used}% used${a.weekly ? ' this week' : ''}`);
  return lines.join(' · ') || 'No usage';
}
// An account by its tool; by its name too once its tool has more than one
export const rowTitle = (a: Account, accounts: Account[]) =>
  accounts.filter((x) => x.agent === a.agent && x.signedIn !== false).length > 1 ? `${TOOL_NAME[a.agent]} · ${a.name}` : TOOL_NAME[a.agent];
// What the picker's list says of an account: each window it has, the 5-hour session and the week
// (an Ultra plan has only the week; Cursor's period may not be a week, so its figure has no name).
// Full when obid holds it full; "No usage" with no figure, never a made-up one.
export const usageLines = (a: Account) => {
  if (a.full) return [{ words: 'Full', cls: 'is-over' }];
  const lines = [];
  if (a.session != null) lines.push({ words: `5h ${a.session}%`, cls: fullness(a.session) });
  if (a.used != null && (a.weekly || a.session == null)) lines.push({ words: a.weekly ? `Week ${a.used}%` : `${a.used}%`, cls: fullness(a.used) });
  return lines.length ? lines : [{ words: 'No usage', cls: '' }];
};
// the dot: past 80% yellow, past 90% red, as the message box's gauge; red when obid holds it full
export const dot = (a: Account) => (a.full ? 'is-over' : fullness(percent(a) ?? 0));

// the owner's choice for it, the setting's when it made none; none is Auto
export const chosenOf = (own: string | null | undefined, setting: string | null | undefined) => {
  const id = own ?? setting ?? AUTO;
  return id === AUTO ? null : id;
};

// The account the pill names: the chosen one while it can run and its turns haven't moved off it;
// else, Auto or moved off it, the one its last answer came from, else the one obid says a first turn
// would use, each while it can run; none can, where it last talked.
export function shownAccount(accounts: Account[], chosen: string | null, talkedOn?: string | null, next?: string | null, movedOff?: string | null) {
  const find = (id?: string | null) => accounts.find((a) => a.id === id);
  const can = (a?: Account) => !!a && a.signedIn !== false && !a.full;
  return [chosen === movedOff ? undefined : find(chosen), find(talkedOn), find(next)].find(can) ?? accounts.find(can) ?? find(talkedOn);
}

// The picker's rows: every connected account, tool by tool
export const list = (accounts: Account[]) => TOOLS.flatMap((agent) => accounts.filter((a) => a.agent === agent && a.signedIn !== false));

// Settings' list: every connected account under its tool
export const groups = (accounts: Account[]) => TOOLS
  .map((agent) => ({ agent, accounts: accounts.filter((a) => a.agent === agent && a.signedIn !== false) }))
  .filter((g) => g.accounts.length);

// Settings' choice of who a new conversation talks through: the menu's, in a Select's shape. The
// words are proposed: Sangam's to change.
export const talkChoices = (accounts: Account[]) => [
  [AUTO, 'Auto · Whichever has room'] as const,
  ...groups(accounts).flatMap((g) => g.accounts.map((a) => [a.id, `${a.name} · ${AGENTS[g.agent].name}`] as const)),
];
