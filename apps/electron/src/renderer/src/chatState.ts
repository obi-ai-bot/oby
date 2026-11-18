import type { Account } from './SettingsView';
import type { Chat } from './chats';
import type { Task } from './tasks';
import { chosenOf, shownAccount } from './talk';
import { ENDED, agentOf } from './view';

// A conversation's state at a glance (docs/contracts/conversation-states.md): the rules the side
// panel's rows and the line over the message box follow, apart from how they look.

// An account that can take a turn or a task now: signed in, not held full, and obid knows of no reason
// it can't start (`cantRun`, ACCOUNTS-19)
export const canRun = (a?: Account) => !!a && a.signedIn !== false && !a.full && !a.cantRun;

// What stops an account, in one line (CHATSTATE-1), or none while it can start
export function cantRunLine(a?: Account): string | null {
  if (!a) return null;
  const who = `${agentOf(a.agent).name} · ${a.name}`;
  if (a.signedIn === false) return `${who} is signed out`;
  return a.cantRun ? `${who} can’t start: ${a.cantRun}` : null;
}

// The account a conversation's next turn goes to first: the one the owner chose for it (or the
// setting's), which obid tries first whatever its state; on Auto, the one its message box names
export function accountOfChat(accounts: Account[], chat: Pick<Chat, 'talkAccount' | 'talkedOn' | 'movedOff'>, setting: string | null, next: string | null) {
  const chosen = chosenOf(chat.talkAccount, setting);
  return accounts.find((a) => a.id === chosen) ?? shownAccount(accounts, chosen, chat.talkedOn, next, chat.movedOff);
}

// A task's account while that matters to it: waiting to start, or ended failed or stopped (it goes on
// only once its account can start, or moves)
export function accountOfTask(accounts: Account[], task: Task) {
  const j = task.job;
  if (task.unknown || !(j.status === 'queued' || ENDED[j.status])) return undefined;
  return accounts.find((a) => a.id === (j.accountId ?? j.account) || a.name === j.account);
}

// Use another account (CHATSTATE-3): the first that can run, of the same tool first
export function otherAccount(accounts: Account[], from: Account): Account | undefined {
  const others = accounts.filter((a) => a.id !== from.id && canRun(a));
  return others.find((a) => a.agent === from.agent) ?? others[0];
}

// A row's one mark (CHATSTATE-5): needs you, working, unread, then how its latest task ended
export type RowMark = 'needs' | 'working' | 'unread' | 'failed' | 'done' | null;
export function rowMark(row: { needs: boolean; working: boolean; unread: boolean; latest?: Task }): RowMark {
  if (row.needs) return 'needs';
  if (row.working) return 'working';
  if (row.unread) return 'unread';
  const t = row.latest;
  if (!t || t.status !== 'done' || t.unknown) return null;
  return ENDED[t.job.status] ? 'failed' : 'done';
}

// The task a conversation started most recently (created: minutes ago, so the smallest)
export const latestTask = (tasks: Task[], chat: string, chatOf: (task: string) => string | undefined) =>
  tasks.filter((t) => chatOf(t.id) === chat).reduce<Task | undefined>((best, t) => (!best || t.job.created < best.job.created ? t : best), undefined);
