import { BrandMark } from './brands';
import { signInFromChat, useAccounts } from './SettingsView';
import { chooseTalk, useTalkSetting, type Chat } from './chats';
import { moveTask, type Task } from './tasks';
import { accountOfChat, accountOfTask, cantRunLine, otherAccount } from './chatState';
import { agentOf } from './view';

// One line over the message box while the account a conversation or task goes to can't run
// (docs/contracts/conversation-states.md, CHATSTATE-1 to 3), as T3 Code's provider banner: why, in
// obid's words, then Sign in (that account's own sign-in, as Settings starts it) and Use another
// account (the first that can run, the same tool's first). Nothing while it can run.
export default function CantRunLine({ chat, task }: { chat?: Chat; task?: Task }) {
  const [accounts] = useAccounts();
  const { account: setting, next } = useTalkSetting();
  const account = chat ? accountOfChat(accounts, chat, setting, next) : task ? accountOfTask(accounts, task) : undefined;
  const line = cantRunLine(account);
  if (!account || !line) return null;
  const other = otherAccount(accounts, account);
  const use = () => {
    if (!other) return;
    if (chat) void chooseTalk(chat.id, other.id);
    else if (task) moveTask(task.id, other);
  };
  return (
    <div className="cant-run" role="status">
      <BrandMark brand={agentOf(account.agent).brand} size={14} mono={account.agent !== 'claude'} />
      <span className="cant-run-text" title={line}>{line}</span>
      <button type="button" className="cant-run-action" onClick={() => signInFromChat(account.id)}>Sign in</button>
      {other && <button type="button" className="cant-run-action" title={`${agentOf(other.agent).name} · ${other.name}`} onClick={use}>Use another account</button>}
    </div>
  );
}
