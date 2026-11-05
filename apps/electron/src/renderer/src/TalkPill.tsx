import { useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import { Menu } from './Menu';
import { everything } from './lean';
import { useAccounts, useModels, useTalkDefault, type Account } from './SettingsView';
import { chooseTalk, lookAtUsage, useTalkSetting, type Chat } from './chats';
import { AUTO, PILL_TOOL_NAME, accountModels, chosenOf, dot, list, modelChoice, modelUsage, percent, rowTitle, shownAccount, usageLines } from './talk';
import type { Agent } from './mock';

const MARKS: Record<Agent, Brand> = { claude: BRANDS.claude, codex: BRANDS.openai, cursor: BRANDS.cursor };
// Claude's mark in its own colour; OpenAI's and Cursor's are one colour, drawn in the text's
const Mark = ({ agent, size }: { agent: Agent; size: number }) => <BrandMark brand={MARKS[agent]} size={size} mono={agent !== 'claude'} />;

// The message box's account and model picker (CONVERSATION-AGENT-19 to 21). obid keeps
// models per account; choosing one also chooses who the conversation talks through next.
export default function TalkPill({ chat }: { chat?: Chat }) {
  const [accounts] = useAccounts();
  const [tuning, options, tune, defaults] = useModels();
  const { account: setting, next } = useTalkSetting();
  const [, setDefault] = useTalkDefault();
  const [at, setAt] = useState<DOMRect | null>(null);
  const [sub, setSub] = useState<{ account: Account; button: HTMLButtonElement; top: number; maxHeight: number } | null>(null);
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const models = useRef<HTMLSpanElement>(null);
  const pill = useRef<HTMLButtonElement>(null);
  const chosen = chosenOf(chat?.talkAccount, setting);
  const on = shownAccount(accounts, chosen, chat?.talkedOn, next, chat?.movedOff);
  // as the usage panel: the menu shows use read in the last 10 minutes
  useEffect(() => { if (at) lookAtUsage(); }, [at]);
  const rows = list(accounts);
  const close = () => { setAt(null); setSub(null); setError(''); pill.current?.focus(); };
  const closeSub = () => { sub?.button.focus(); setSub(null); };
  const modelsOf = (a: Account) => accountModels(a, options[a.agent]?.models);
  const modelOf = (a: Account) => modelChoice(tuning(a).model, defaults[a.agent]?.model ?? 'auto', modelsOf(a));
  const pick = async (account: string, model?: string) => {
    if (saving) return;
    setSaving(true);
    setError('');
    try {
      if (model != null) await tune(account, { model });
      if (chat) await chooseTalk(chat.id, account); else await setDefault(account);
      close();
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setSaving(false); }
  };
  const openModels = (a: Account, button: HTMLButtonElement) => {
    // Keep the submenu inside the window at its smallest size: scroll when the list is long.
    const room = Math.max(120, innerHeight - button.getBoundingClientRect().top - 12);
    const tall = Math.min(modelsOf(a).length * 30 + 14, room);
    setSub({ account: a, button, top: Math.min(-8, innerHeight - 12 - tall - button.getBoundingClientRect().top), maxHeight: tall });
  };
  useEffect(() => {
    if (at) (menu.current?.querySelector<HTMLButtonElement>(':scope > button[aria-checked="true"], :scope > .menu-sub-anchor > button[aria-checked="true"]') ?? menu.current?.querySelector('button'))?.focus();
  }, [at]);
  useEffect(() => {
    if (sub) (models.current?.querySelector<HTMLButtonElement>('[aria-checked="true"]') ?? models.current?.querySelector('button'))?.focus();
  }, [sub]);
  const move = (e: React.KeyboardEvent, root: HTMLElement, nested = false) => {
    if (e.key === 'ArrowLeft') { e.preventDefault(); e.stopPropagation(); if (sub) closeSub(); return; }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault(); e.stopPropagation();
    const buttons = Array.from(root.querySelectorAll<HTMLButtonElement>(nested ? 'button' : ':scope > button, :scope > .menu-sub-anchor > button'));
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    buttons[(index + (e.key === 'ArrowDown' ? 1 : buttons.length - 1)) % buttons.length]?.focus();
  };
  const used = on ? percent(on) : null;
  const full = on ? dot(on) : '';
  // Keep both menus inside the Mac window, including its 420px minimum width.
  const menuWidth = at ? Math.min(360, innerWidth - (sub ? 232 : 24)) : 360;
  const placement = at && sub ? { width: menuWidth, right: Math.max(8, Math.min(innerWidth - at.right, innerWidth - menuWidth - 220)) } : undefined;
  return (
    <>
      <button
        ref={pill}
        type="button"
        className={`talk-pick${at ? ' is-on' : ''}`}
        aria-haspopup="menu"
        aria-expanded={!!at}
        title={on ? `${chosen ? '' : 'Auto · '}${rowTitle(on, accounts)} · ${modelOf(on).name} · ${usageLines(on).map((l) => l.words).join(' · ')}` : 'Add an account'}
        aria-label={on ? `Talking through ${rowTitle(on, accounts)} · ${modelOf(on).name}${on.full ? ', full' : used == null ? '' : `, ${used}% used`}. Change` : 'Choose who Oby talks through'}
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => { if (at) close(); else setAt(e.currentTarget.getBoundingClientRect()); }}
      >
        {on ? <>
          {chosen ? <Mark agent={on.agent} size={16} /> : <span className="talk-pick-auto">Auto</span>}
          {chosen && <span>{PILL_TOOL_NAME[on.agent]} · {modelOf(on).name}</span>}
          {full && <span className={`usage-dot ${full}`} aria-hidden="true" />}
        </> : <Plus size={16} strokeWidth={1.6} />}
        <ChevronDown size={13} strokeWidth={1.8} />
      </button>
      {at && (
        <Menu at={at} onClose={close} onEscape={sub ? closeSub : close} width={menuWidth} style={placement} height={100 + rows.length * 68}>
          <div ref={menu} className="talk-menu" onKeyDown={(e) => move(e, e.currentTarget)} aria-busy={saving}>
            <button role="menuitemradio" aria-checked={!chosen} className="talk-choice" onFocus={() => setSub(null)} onClick={() => void pick(AUTO)}>
              <span className="talk-choice-mark" />
              <span className="talk-choice-text">
                <span className="talk-choice-name">Auto</span>
                <span className="talk-choice-about">Whichever has room</span>
              </span>
              {!chosen && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
            </button>
            {rows.map((a) => (
              <span key={a.id} className="menu-sub-anchor">
              <button role="menuitemradio" aria-checked={chosen === a.id} aria-haspopup="menu" aria-expanded={sub?.account.id === a.id} className="talk-choice"
                onFocus={() => { if (sub?.account.id !== a.id) setSub(null); }}
                onKeyDown={(e) => { if (e.key === 'ArrowRight') { e.preventDefault(); openModels(a, e.currentTarget); } }}
                onClick={(e) => openModels(a, e.currentTarget)}>
                <span className="talk-choice-mark"><Mark agent={a.agent} size={18} /></span>
                <span className="talk-choice-text">
                  <span className="talk-choice-name">{rowTitle(a, accounts)}</span>
                  <span className="talk-choice-about" style={{ whiteSpace: 'normal' }}>{modelOf(a).name} · {modelUsage(a)}</span>
                  {modelOf(a).unavailable && <span className="talk-choice-about" style={{ whiteSpace: 'normal' }}>Previous model unavailable · using default</span>}
                </span>
                {chosen === a.id && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
                <ChevronRight size={13} strokeWidth={1.8} />
              </button>
              {sub?.account.id === a.id && <span ref={models} role="menu" aria-label={`${rowTitle(a, accounts)} models`} className="row-menu row-submenu is-left is-scroll" style={{ top: sub.top, minWidth: 200, maxHeight: sub.maxHeight }} onKeyDown={(e) => move(e, e.currentTarget, true)}>
                {modelsOf(a).map((m) => <button key={m.id} role="menuitemradio" aria-checked={modelOf(a).id === m.id} disabled={saving} onClick={() => void pick(a.id, m.id)}>
                  {m.name}{modelOf(a).id === m.id && <Check size={14} strokeWidth={1.8} className="menu-check" />}
                </button>)}
              </span>}
              </span>
            ))}
            <hr />
            {error && <span role="alert" className="talk-choice-about">{error}</span>}
            <button role="menuitem" className="talk-choice is-add" onFocus={() => setSub(null)} onClick={() => { close(); dispatchEvent(new CustomEvent('oby:open-setting', { detail: everything ? 'accounts' : 'agents' })); }}>
              <span className="talk-choice-mark"><Plus size={16} strokeWidth={1.6} /></span>
              <span className="talk-choice-text"><span className="talk-choice-name">Add account</span></span>
            </button>
          </div>
        </Menu>
      )}
    </>
  );
}
