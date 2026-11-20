import { useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { BrandMark } from './brands';
import { Menu } from './Menu';
import { everything } from './lean';
import { EFFORT_NAMES, useAccounts, useModels, useTalkDefault, type Account } from './SettingsView';
import { chooseTalk, lookAtUsage, useTalkSetting, type Chat } from './chats';
import { AUTO, MENU_WIDTH, PILL_TOOL_NAME, accountModels, chosenOf, dot, hasModelSearch, list, menuModels, modelChoice, modelMenuPlace, modelUsage, percent, rowTitle, shownAccount, toolName, usageLines } from './talk';
import { agentOf, type Agent } from './view';
import { isAgent } from '../../shared/obid-wire';

// Claude's mark in its own colour; OpenAI's and Cursor's are one colour, drawn in the text's (a tool
// this build doesn't know: agentOf's, gray with its letter, docs/contracts/providers.md)
const Mark = ({ agent, size }: { agent: string; size: number }) => <BrandMark brand={agentOf(agent).brand} size={size} mono={agent !== 'claude'} />;

// The message box's account and model picker (CONVERSATION-AGENT-19 to 21). obid keeps
// models per account; choosing one also chooses who the conversation talks through next.
export default function TalkPill({ chat }: { chat?: Chat }) {
  const [accounts] = useAccounts();
  const [tuning, options, tune, defaults] = useModels();
  const { account: setting, next } = useTalkSetting();
  const [, setDefault] = useTalkDefault();
  const [at, setAt] = useState<DOMRect | null>(null);
  // the account's models beside the menu (modelMenuPlace): its top from its row's when it opened, less
  // how far the accounts have scrolled since (scrolled, from `from`); `query` is what its search holds
  const [sub, setSub] = useState<(ReturnType<typeof modelMenuPlace> & { account: Account; button: HTMLButtonElement; from: number; query: string; search: boolean }) | null>(null);
  const [scrolled, setScrolled] = useState(0);
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const models = useRef<HTMLSpanElement>(null);
  const searching = useRef<HTMLInputElement>(null);
  const pill = useRef<HTMLButtonElement>(null);
  const chosen = chosenOf(chat?.talkAccount, setting);
  const on = shownAccount(accounts, chosen, chat?.talkedOn, next, chat?.movedOff);
  // as the usage panel: the menu shows use read in the last 10 minutes. Asked once the menu is in
  // (its 160ms fade), so obid's answer doesn't land in the middle of it opening
  useEffect(() => {
    if (!at) return;
    const t = setTimeout(lookAtUsage, 250);
    return () => clearTimeout(t);
  }, [at]);
  const rows = list(accounts);
  const subAccount = sub && rows.find((a) => a.id === sub.account.id);
  const close = () => { setAt(null); setSub(null); setScrolled(0); setError(''); pill.current?.focus(); };
  const closeSub = () => { sub?.button.focus(); setSub(null); };
  const modelsOf = (a: Account) => accountModels(a, options[a.agent as Agent]?.models);
  const modelOf = (a: Account) => modelChoice(tuning(a).model, defaults[a.agent as Agent]?.model ?? 'auto', modelsOf(a), !!(a.modelsRead || a.models?.length));
  const menuOf = (a: Account) => menuModels(modelsOf(a), modelOf(a).id, options[a.agent as Agent]?.groups);
  // its model, after its name: none for a tool this build doesn't know, which it has no list of
  const modelWords = (a: Account) => (isAgent(a.agent) ? ` · ${modelOf(a).name}` : '');
  // the efforts its model takes, under its models (COMPOSER-17): Cursor's are in its model's name
  // (AGENTS-11), so none; and the one it runs on, as Settings › Accounts has it
  const effortsOf = (a: Account): string[] => {
    if (a.agent === 'cursor' || !isAgent(a.agent)) return [];
    const tool = options[a.agent];
    return tool?.models.find((m) => m.id === modelOf(a).id)?.efforts ?? tool?.efforts ?? [];
  };
  const effortOf = (a: Account) => tuning(a).effort ?? (defaults[a.agent as Agent] as { effort?: string } | undefined)?.effort;
  const pick = async (account: string, model?: string, effort?: string) => {
    if (saving) return;
    setSaving(true);
    setError('');
    try {
      if (model != null) await tune(account, { model });
      if (effort != null) await tune(account, { effort });
      if (chat) await chooseTalk(chat.id, account); else await setDefault(account);
      close();
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setSaving(false); }
  };
  // beside the menu, where it is: the menu's own size and place don't depend on it (modelMenuPlace)
  const openModels = (a: Account, button: HTMLButtonElement) => {
    const m = (button.closest('.row-menu') ?? button).getBoundingClientRect(), r = button.getBoundingClientRect();
    const count = menuOf(a).length;
    setSub({ account: a, button, from: scrolled, query: '', search: hasModelSearch(count),
      ...modelMenuPlace({ menuLeft: m.left, menuRight: m.right, menuTop: m.top, rowTop: r.top, count, efforts: effortsOf(a).length, windowWidth: innerWidth, windowHeight: innerHeight }) });
  };
  useEffect(() => {
    if (at) (menu.current?.querySelector<HTMLButtonElement>(':scope > button[aria-checked="true"], :scope > .menu-sub-anchor > button[aria-checked="true"]:not(:disabled)') ?? menu.current?.querySelector('button'))?.focus();
  }, [at]);
  // opening a list: its search takes the keys, if it has one, else its current model; typing in the
  // search is the list changing, not opening, so it keeps the focus
  const subId = sub?.account.id;
  useEffect(() => {
    if (subId) (searching.current ?? models.current?.querySelector<HTMLButtonElement>('[aria-checked="true"]') ?? models.current?.querySelector('button'))?.focus();
  }, [subId]);
  const move = (e: React.KeyboardEvent, root: HTMLElement, nested = false) => {
    const typing = (e.target as HTMLElement).tagName === 'INPUT';
    if (e.key === 'ArrowLeft') { if (typing) return; e.preventDefault(); e.stopPropagation(); if (sub) closeSub(); return; }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault(); e.stopPropagation();
    const buttons = Array.from(root.querySelectorAll<HTMLElement>(nested ? 'input, button' : ':scope > button, :scope > .menu-sub-anchor > button:not(:disabled)'));
    const index = buttons.indexOf(document.activeElement as HTMLElement);
    buttons[(index + (e.key === 'ArrowDown' ? 1 : buttons.length - 1)) % buttons.length]?.focus();
  };
  const used = on ? percent(on) : null;
  const full = on ? dot(on) : '';
  // Keep both menus inside the Mac window, including its 420px minimum width.
  // The menu has one width, whether a model list is open beside it or not.
  const menuWidth = Math.min(MENU_WIDTH, innerWidth - 24);
  const shown = sub && subAccount ? (() => {
    const q = sub.query.trim().toLowerCase();
    return menuOf(subAccount).filter((m) => m.name.toLowerCase().includes(q));
  })() : [];
  // Escape clears the search first, then closes the list, then the menu
  const escape = () => { if (!sub) close(); else if (sub.query) { setSub({ ...sub, query: '' }); searching.current?.focus(); } else closeSub(); };
  return (
    <>
      <button
        ref={pill}
        type="button"
        className={`talk-pick${at ? ' is-on' : ''}`}
        aria-haspopup="menu"
        aria-expanded={!!at}
        title={on ? `${chosen ? '' : 'Auto · '}${rowTitle(on, accounts)}${modelWords(on)} · ${usageLines(on).map((l) => l.words).join(' · ')}` : 'Add an account'}
        aria-label={on ? `Talking through ${rowTitle(on, accounts)}${modelWords(on)}${on.full ? ', full' : used == null ? '' : `, ${used}% used`}. Change` : 'Choose who Oby talks through'}
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => { if (at) close(); else setAt(e.currentTarget.getBoundingClientRect()); }}
      >
        {on ? <>
          {chosen ? <Mark agent={on.agent} size={16} /> : <span className="talk-pick-auto">Auto</span>}
          {chosen && <span>{toolName(on.agent, PILL_TOOL_NAME)}{modelWords(on)}</span>}
          {full && <span className={`usage-dot ${full}`} aria-hidden="true" />}
        </> : <Plus size={16} strokeWidth={1.6} />}
        <ChevronDown size={13} strokeWidth={1.8} />
      </button>
      {at && (
        <Menu at={at} onClose={close} onEscape={escape} width={menuWidth} style={{ width: menuWidth }} height={110 + rows.length * 46} scrollPart>
          {/* the accounts scroll, the models beside them don't: a scrolling box clips what's outside it */}
          <div ref={menu} className="talk-menu menu-scroll" onKeyDown={(e) => move(e, e.currentTarget)} onScroll={(e) => setScrolled(e.currentTarget.scrollTop)} aria-busy={saving}>
            <button role="menuitemradio" aria-checked={!chosen} className="talk-choice" onFocus={() => setSub(null)} onClick={() => void pick(AUTO)}>
              <span className="talk-choice-mark" />
              <span className="talk-choice-text">
                <span className="talk-choice-name">Auto</span>
                <span className="talk-choice-about">Whichever has room</span>
              </span>
              {!chosen && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
            </button>
            {rows.map((a) => (isAgent(a.agent) ? (
              <span key={a.id} className="menu-sub-anchor">
              <button role="menuitemradio" aria-checked={chosen === a.id} aria-haspopup="menu" aria-expanded={sub?.account.id === a.id} className="talk-choice"
                onFocus={() => { if (sub?.account.id !== a.id) setSub(null); }}
                onKeyDown={(e) => { if (e.key === 'ArrowRight') { e.preventDefault(); openModels(a, e.currentTarget); } }}
                onClick={(e) => openModels(a, e.currentTarget)}>
                <span className="talk-choice-mark"><Mark agent={a.agent} size={18} /></span>
                <span className="talk-choice-text">
                  <span className="talk-choice-name">{rowTitle(a, accounts)}</span>
                  <span className="talk-choice-about" title={`${modelOf(a).name} · ${modelUsage(a)}`}>{modelOf(a).name} · {modelUsage(a)}</span>
                  {modelOf(a).unavailable && <span className="talk-choice-about">Previous model unavailable · using default</span>}
                </span>
                {chosen === a.id && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
                <ChevronRight size={13} strokeWidth={1.8} />
              </button>
              </span>
            ) : (
              // a tool this build doesn't know: listed, but nothing to choose (docs/contracts/providers.md, PROVIDERS-6)
              <span key={a.id} className="menu-sub-anchor">
              <button role="menuitemradio" aria-checked={chosen === a.id} disabled aria-disabled="true" className="talk-choice">
                <span className="talk-choice-mark"><Mark agent={a.agent} size={18} /></span>
                <span className="talk-choice-text">
                  <span className="talk-choice-name">{rowTitle(a, accounts)}</span>
                  <span className="talk-choice-about" title={modelUsage(a)}>{modelUsage(a)}</span>
                </span>
                {chosen === a.id && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
              </button>
              </span>
            )))}
            <hr />
            {error && <span role="alert" className="talk-choice-about">{error}</span>}
            <button role="menuitem" className="talk-choice is-add" onFocus={() => setSub(null)} onClick={() => { close(); dispatchEvent(new CustomEvent('oby:open-setting', { detail: everything ? 'accounts' : 'agents' })); }}>
              <span className="talk-choice-mark"><Plus size={16} strokeWidth={1.6} /></span>
              <span className="talk-choice-text"><span className="talk-choice-name">Add account</span></span>
            </button>
          </div>
          {/* beside the menu, a list of rows of one line each, its search above them when it's long */}
          {sub && subAccount && <span ref={models} role="menu" aria-label={`${rowTitle(subAccount, accounts)} models`} className={`row-menu row-submenu model-menu is-${sub.side}`}
            style={{ top: sub.top - (scrolled - sub.from), width: sub.width, height: sub.height, ...(sub.side === 'right' ? { left: 'calc(100% + 4px)' } : sub.side === 'left' ? { left: 'auto', right: 'calc(100% + 4px)' } : { left: 'auto', right: 8 }) }}
            onKeyDown={(e) => move(e, e.currentTarget, true)}>
            {sub.search && <input ref={searching} type="search" className="model-search" role="searchbox" aria-label="Search models" placeholder="Search models" value={sub.query} spellCheck={false} autoComplete="off"
              onChange={(e) => setSub({ ...sub, query: e.target.value })}
              onKeyDown={(e) => { if (e.key === 'Enter' && shown[0]) { e.preventDefault(); void pick(subAccount.id, shown[0].id); } }} />}
            <span className="model-list">
              {shown.length ? shown.map((m) => <button key={m.id} className="model-row" role="menuitemradio" aria-checked={m.on} title={m.name} disabled={saving} onClick={() => void pick(subAccount.id, m.id)}>
                <span className="model-name">{m.name}</span>{m.on && <Check size={14} strokeWidth={1.8} className="menu-check" />}
              </button>) : <span className="model-empty">No models match</span>}
            </span>
            {/* the effort it runs on, under its models (COMPOSER-17), kept on the account as its model is */}
            {effortsOf(subAccount).length > 0 && <span className="model-efforts" role="group" aria-label="Effort">
              <span className="model-efforts-label">Effort</span>
              <span className="model-efforts-pills">
                {effortsOf(subAccount).map((e) => <button key={e} className="effort-pill" role="menuitemradio" aria-checked={e === effortOf(subAccount)} disabled={saving}
                  onClick={() => void pick(subAccount.id, undefined, e)}>{EFFORT_NAMES[e] ?? e}</button>)}
              </span>
            </span>}
          </span>}
        </Menu>
      )}
    </>
  );
}
