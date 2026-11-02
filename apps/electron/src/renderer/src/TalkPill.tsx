import { useEffect, useState } from 'react';
import { Check, ChevronDown } from 'lucide-react';
import { BrandMark } from './brands';
import { Menu } from './Menu';
import { AGENTS } from './mock';
import { everything } from './lean';
import { useAccounts, fullness } from './SettingsView';
import { chooseTalk, lookAtUsage, useTalkSetting, type Chat } from './chats';
import { AUTO, chosenOf, dot, groups, percent, shownAccount, usageWords } from './talk';

// The chat window's way to which agent it talks through (docs/contracts/conversation-agent.md), at
// the right of its title bar: the agent, the account and how full it is; a click opens the menu:
// Auto first, then every connected account under its tool, each with its week and a meter, the
// chosen one checked, and Add account last. A choice takes effect at the next turn. The words are
// proposed: Sangam's to change.
export default function TalkPill({ chat }: { chat: Chat }) {
  const [accounts] = useAccounts();
  const { account: setting, next } = useTalkSetting();
  const [at, setAt] = useState<DOMRect | null>(null);
  const chosen = chosenOf(chat.talkAccount, setting);
  const on = shownAccount(accounts, chosen, chat.talkedOn, next);
  // as the usage panel: the menu shows use read in the last 10 minutes
  useEffect(() => { if (at) lookAtUsage(); }, [at]);
  const list = groups(accounts);
  const rows = list.reduce((n, g) => n + 1 + g.accounts.length, 0);
  const pick = (account: string) => { chooseTalk(chat.id, account); setAt(null); };
  const used = on ? percent(on) : null;
  const full = on ? dot(on) : '';
  return (
    <>
      <button
        className={`bar-convo talk-pill${at ? ' is-on' : ''}`}
        aria-haspopup="menu"
        aria-expanded={!!at}
        aria-label={on ? `Talking through ${AGENTS[on.agent].name} · ${on.name}${used == null ? '' : `, ${used}% used`}. Change` : 'Choose who Oby talks through'}
        onClick={(e) => setAt(at ? null : e.currentTarget.getBoundingClientRect())}
      >
        {on ? <>
          <BrandMark brand={AGENTS[on.agent].brand} size={14} mono />
          <span className="talk-name">{AGENTS[on.agent].name} · {on.name}</span>
          {used != null && <span className={`talk-used ${fullness(used)}`.trim()}>{used}%</span>}
          {full && <span className={`talk-dot ${full}`} aria-hidden="true" />}
        </> : <span className="talk-name">Add an account</span>}
        <ChevronDown size={14} strokeWidth={1.6} />
      </button>
      {at && (
        <Menu at={at} onClose={() => setAt(null)} width={264} height={92 + rows * 46}>
          <button role="menuitemradio" aria-checked={!chosen} className="talk-row" onClick={() => pick(AUTO)}>
            <span className="talk-row-text">
              <span className="talk-row-line">Auto{!chosen && <Check size={14} strokeWidth={1.8} className="menu-check" />}</span>
              <span className="talk-row-about">Whichever has room</span>
            </span>
          </button>
          {list.map((g) => (
            <div key={g.agent} role="group" aria-label={AGENTS[g.agent].name}>
              <span className="menu-label talk-group"><BrandMark brand={AGENTS[g.agent].brand} size={14} mono />{AGENTS[g.agent].name}</span>
              {g.accounts.map((a) => {
                const share = percent(a);
                return (
                  <button key={a.id} role="menuitemradio" aria-checked={chosen === a.id} className="talk-row" onClick={() => pick(a.id)}>
                    <span className="talk-row-text">
                      <span className="talk-row-line">
                        {a.name}
                        <span className={`talk-row-used ${dot(a)}`.trim()}>{usageWords(a)}</span>
                        {chosen === a.id && <Check size={14} strokeWidth={1.8} className="menu-check" />}
                      </span>
                      {share != null && (
                        <span className="set-meter" role="meter" aria-valuenow={share} aria-valuemin={0} aria-valuemax={100} aria-label={`${share}% used`}>
                          <span className={fullness(share)} style={{ width: `${share}%` }} />
                        </span>
                      )}
                    </span>
                  </button>
                );
              })}
            </div>
          ))}
          <hr />
          <button role="menuitem" onClick={() => { setAt(null); dispatchEvent(new CustomEvent('oby:open-setting', { detail: everything ? 'accounts' : 'agents' })); }}>Add account</button>
        </Menu>
      )}
    </>
  );
}
