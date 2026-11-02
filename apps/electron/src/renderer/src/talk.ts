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
  return used == null ? 'No usage reported' : `${used}%`;
};
// the dot: past 80% yellow, past 90% red, as the message box's gauge; red when obid holds it full
export const dot = (a: Account) => (a.full ? 'is-over' : fullness(percent(a) ?? 0));

// the owner's choice for it, the setting's when it made none; none is Auto
export const chosenOf = (own: string | null | undefined, setting: string | null | undefined) => {
  const id = own ?? setting ?? AUTO;
  return id === AUTO ? null : id;
};

// The account the pill names: the chosen one while it can run; else, Auto or moved off it, the one
// its last answer came from, else the one obid says a first turn would use, each while it can run;
// none can, where it last talked.
export function shownAccount(accounts: Account[], chosen: string | null, talkedOn?: string | null, next?: string | null) {
  const find = (id?: string | null) => accounts.find((a) => a.id === id);
  const can = (a?: Account) => !!a && a.signedIn !== false && !a.full;
  return [find(chosen), find(talkedOn), find(next)].find(can) ?? accounts.find(can) ?? find(talkedOn);
}

// The menu's groups: every connected account under its tool
export const groups = (accounts: Account[]) => TOOLS
  .map((agent) => ({ agent, accounts: accounts.filter((a) => a.agent === agent && a.signedIn !== false) }))
  .filter((g) => g.accounts.length);

// Settings' choice of who a new conversation talks through: the menu's, in a Select's shape. The
// words are proposed: Sangam's to change.
export const talkChoices = (accounts: Account[]) => [
  [AUTO, 'Auto · Whichever has room'] as const,
  ...groups(accounts).flatMap((g) => g.accounts.map((a) => [a.id, `${a.name} · ${AGENTS[g.agent].name}`] as const)),
];
