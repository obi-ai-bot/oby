import { useEffect, useState } from 'react';
import { Check, ChevronDown, Plus } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import { Menu } from './Menu';
import { everything } from './lean';
import { useAccounts, useTalkDefault } from './SettingsView';
import { chooseTalk, lookAtUsage, useTalkSetting, type Chat } from './chats';
import { AUTO, TOOL_NAME, chosenOf, dot, list, percent, rowTitle, shownAccount, usageLines } from './talk';
import type { Agent } from './mock';

const MARKS: Record<Agent, Brand> = { claude: BRANDS.claude, codex: BRANDS.openai, cursor: BRANDS.cursor };
// Claude's mark in its own colour; OpenAI's and Cursor's are one colour, drawn in the text's
const Mark = ({ agent, size }: { agent: Agent; size: number }) => <BrandMark brand={MARKS[agent]} size={size} mono={agent !== 'claude'} />;

// In the message box, by send: the icon of the account chosen and an arrow, or the word Auto (no icon,
// as Cursor's picker); a click opens the list:
// Auto, then every connected account, the chosen one checked, then Add account. In a conversation
// (chat) a choice is that conversation's and takes effect at its next turn; on Home it's the
// setting a new conversation follows (docs/contracts/conversation-agent.md). The words are
// proposed: Sangam's to change.
export default function TalkPill({ chat }: { chat?: Chat }) {
  const [accounts] = useAccounts();
  const { account: setting, next } = useTalkSetting();
  const [, setDefault] = useTalkDefault();
  const [at, setAt] = useState<DOMRect | null>(null);
  const chosen = chosenOf(chat?.talkAccount, setting);
  const on = shownAccount(accounts, chosen, chat?.talkedOn, next, chat?.movedOff);
  // as the usage panel: the menu shows use read in the last 10 minutes
  useEffect(() => { if (at) lookAtUsage(); }, [at]);
  const rows = list(accounts);
  const pick = (account: string) => {
    if (chat) chooseTalk(chat.id, account); else void setDefault(account);
    setAt(null);
  };
  const used = on ? percent(on) : null;
  const full = on ? dot(on) : '';
  return (
    <>
      <button
        type="button"
        className={`talk-pick${at ? ' is-on' : ''}`}
        aria-haspopup="menu"
        aria-expanded={!!at}
        title={on ? `${chosen ? '' : 'Auto · '}${rowTitle(on, accounts)} · ${usageLines(on).map((l) => l.words).join(' · ')}` : 'Add an account'}
        aria-label={on ? `Talking through ${rowTitle(on, accounts)}${on.full ? ', full' : used == null ? '' : `, ${used}% used`}. Change` : 'Choose who Oby talks through'}
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => setAt(at ? null : e.currentTarget.getBoundingClientRect())}
      >
        {on ? <>
          {chosen ? <Mark agent={on.agent} size={16} /> : <span className="talk-pick-auto">Auto</span>}
          {full && <span className={`usage-dot ${full}`} aria-hidden="true" />}
        </> : <Plus size={16} strokeWidth={1.6} />}
        <ChevronDown size={13} strokeWidth={1.8} />
      </button>
      {at && (
        <Menu at={at} onClose={() => setAt(null)} width={300} height={100 + rows.length * 52}>
          <div className="talk-menu">
            <button role="menuitemradio" aria-checked={!chosen} className="talk-choice" onClick={() => pick(AUTO)}>
              <span className="talk-choice-mark" />
              <span className="talk-choice-text">
                <span className="talk-choice-name">Auto</span>
                <span className="talk-choice-about">Whichever has room</span>
              </span>
              {!chosen && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
            </button>
            {rows.map((a) => (
              <button key={a.id} role="menuitemradio" aria-checked={chosen === a.id} className="talk-choice" onClick={() => pick(a.id)}>
                <span className="talk-choice-mark"><Mark agent={a.agent} size={18} /></span>
                <span className="talk-choice-text">
                  <span className="talk-choice-name">{rowTitle(a, accounts)}</span>
                  <span className="talk-choice-about">{a.who || TOOL_NAME[a.agent]}</span>
                </span>
                <span className="talk-choice-usage">
                  {usageLines(a).map((l) => <span key={l.words} className={`talk-choice-used ${l.cls}`.trim()}>{l.words}</span>)}
                </span>
                {chosen === a.id && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
              </button>
            ))}
            <hr />
            <button role="menuitem" className="talk-choice is-add" onClick={() => { setAt(null); dispatchEvent(new CustomEvent('oby:open-setting', { detail: everything ? 'accounts' : 'agents' })); }}>
              <span className="talk-choice-mark"><Plus size={16} strokeWidth={1.6} /></span>
              <span className="talk-choice-text"><span className="talk-choice-name">Add account</span></span>
            </button>
          </div>
        </Menu>
      )}
    </>
  );
}
