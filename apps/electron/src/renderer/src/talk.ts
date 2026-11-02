// Which account a conversation talks through (docs/contracts/conversation-agent.md), as the chat
// window's pill and its menu show it: what obid says, in words. obid decides; this only reads.
import type { Agent } from './mock';
import { fullness, type Account } from './SettingsView';

// obid's word for "whichever has room"
export const AUTO = 'auto';
const TOOLS: Agent[] = ['claude', 'codex', 'cursor'];

// its share of the week (else of its 5-hour session); none: it reports none, and no number is made up
export const percent = (a: Account) => a.used ?? a.session ?? null;
export const usageWords = (a: Account) => {
  const used = percent(a);
  return used == null ? 'No usage reported' : `${used}%`;
};
// the dot: past 80% yellow, past 90% red, of its session or its week, as the message box's gauge
export const dot = (a: Account) => fullness(Math.max(a.used ?? 0, a.session ?? 0));

// the owner's choice for it, the setting's when it made none; none is Auto
export const chosenOf = (own: string | null | undefined, setting: string | null | undefined) => {
  const id = own ?? setting ?? AUTO;
  return id === AUTO ? null : id;
};

// The account the pill names: the chosen one while it can run; else, Auto or moved off it, the one
// its last answer came from, else the one obid says a first turn would use.
export function shownAccount(accounts: Account[], chosen: string | null, talkedOn?: string | null, next?: string | null) {
  const find = (id?: string | null) => accounts.find((a) => a.id === id);
  const picked = find(chosen);
  if (picked && picked.signedIn !== false && !picked.full) return picked;
  return find(talkedOn) ?? find(next) ?? accounts.find((a) => a.signedIn !== false);
}

// The menu's groups: every connected account under its tool
export const groups = (accounts: Account[]) => TOOLS
  .map((agent) => ({ agent, accounts: accounts.filter((a) => a.agent === agent && a.signedIn !== false) }))
  .filter((g) => g.accounts.length);
