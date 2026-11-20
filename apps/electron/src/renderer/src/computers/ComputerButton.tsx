// The computer button at the mini sidebar's foot, above Settings, and its menu (the approved
// design's screens 01, 06 and 10; docs/contracts/computers-app.md, COMPUTERSAPP-1 to 3, 9 and 10).
// The icon is the computer in use: a laptop for This Mac, a desktop for another Mac, a server for
// Linux; its tooltip that computer's name. The menu is shaped like the profile menu: the computers,
// then Add a computer…; a computer's ⋯ (on hover) has Rename… and Remove….
import { useState } from 'react';
import { DesktopIcon, HardDrivesIcon, LaptopIcon } from '@phosphor-icons/react';
import { Check, Ellipsis, Plus } from 'lucide-react';
import { Menu } from '../Menu';
import { composing } from '../shortcuts';
import { closeMenu, current, currentName, openSheet, rename, showMore, startRename, stopRename, toggleMenu, use, useComputers } from './store';
import { THIS_MAC, buttonLabel, rowAbout, type Computer } from './view';
import { ComputerSheets } from './sheets';
import './computers.css';

export function ComputerIcon({ c, size }: { c: Pick<Computer, 'os'> | undefined; size: number }) {
  const Icon = !c ? LaptopIcon : c.os === 'linux' ? HardDrivesIcon : DesktopIcon;
  return <Icon size={size} aria-hidden="true" />;
}

export function ComputerButton() {
  const s = useComputers();
  const [at, setAt] = useState<DOMRect | null>(null);
  if (!s.offered) return null;
  const inUse = current(s), name = currentName(s);
  // the orange "needs you" dot: only when no route answers
  const lost = inUse?.state === 'lost';
  return (
    <>
      <button
        className={`rail-button rail-computer${s.ui.menu ? ' is-on' : ''}${lost ? ' has-dot is-needs' : ''}`}
        data-tip={name} aria-label={buttonLabel(name)} aria-haspopup="menu" aria-expanded={s.ui.menu}
        onClick={(e) => { setAt(e.currentTarget.getBoundingClientRect()); toggleMenu(); }}
        ref={(el) => { if (el && s.ui.menu && !at) setAt(el.getBoundingClientRect()); }}
      >
        <ComputerIcon c={inUse} size={20} />
      </button>
      {s.ui.menu && at && <ComputerMenu at={at} />}
      <ComputerSheets />
    </>
  );
}

function ComputerMenu({ at }: { at: DOMRect }) {
  const s = useComputers();
  const others = s.list.computers;
  const alone = others.length === 0;
  // a 48px row each (36px for This Mac alone), the rule, the note, Add a computer… and the padding
  const height = 44 + others.length * 50 + (alone ? 48 : 0) + 56;
  return (
    // beside the rail, its foot level with the button's (the design's left 56px, bottom 54px)
    <Menu at={at} onClose={closeMenu} onEscape={() => (s.ui.renaming ? stopRename() : s.ui.more ? showMore(null) : closeMenu())}
      align="left" width={264} height={height} style={{ maxWidth: 264, left: at.right + 16, top: 'auto', bottom: innerHeight - at.bottom }}>
      {/* (display: contents: the menu's own box is Menu's) */}
      <div className="computer-menu" aria-label="Computers" role="group">
        <button role="menuitemradio" aria-checked={s.list.current === 'this'} className="talk-choice" onClick={() => void use('this')}>
          <span className="talk-choice-mark"><ComputerIcon c={undefined} size={16} /></span>
          <span className="talk-choice-text"><span className="talk-choice-name">{THIS_MAC}</span></span>
          {s.list.current === 'this' && <Check size={14} strokeWidth={1.8} className="talk-choice-check" />}
        </button>
        {others.map((c) => <Row key={c.id} c={c} inUse={s.list.current === c.id} more={s.ui.more === c.id} renaming={s.ui.renaming === c.id} />)}
        <hr />
        {alone && <p className="computer-note">Work on another Mac or Linux box from this window.</p>}
        <button role="menuitem" className="talk-choice is-add" onClick={() => openSheet({ kind: 'add', how: 'ssh' })}>
          <span className="talk-choice-mark"><Plus size={14} strokeWidth={1.8} /></span>
          Add a computer…
        </button>
      </div>
    </Menu>
  );
}

function Row({ c, inUse, more, renaming }: { c: Computer; inUse: boolean; more: boolean; renaming: boolean }) {
  const about = rowAbout(c, inUse);
  const mark = <span className="talk-choice-mark"><ComputerIcon c={c} size={16} /></span>;
  const aboutLine = <span className="talk-choice-about">{inUse && c.state === 'connected' && <span className="route-dot" />}{about}</span>;
  if (renaming) {
    // the row's name becomes a field in place, as a conversation's does in the side panel
    return (
      <div className="computer-row">
        <div className="talk-choice is-renaming">
          {mark}
          <span className="talk-choice-text">
            <input className="sidebar-rename" defaultValue={c.name} autoFocus aria-label="Computer name" spellCheck={false}
              // Enter or a click elsewhere keeps the name; Esc (the menu's, above) leaves it as it was
              onFocus={(e) => e.currentTarget.select()}
              onBlur={(e) => void rename(c.id, e.currentTarget.value)}
              onKeyDown={(e) => { if (!composing(e) && e.key === 'Enter') { e.preventDefault(); e.currentTarget.blur(); } }} />
            {aboutLine}
          </span>
        </div>
      </div>
    );
  }
  return (
    <div className={`computer-row${more ? ' is-more' : ''}`}>
      <button role="menuitemradio" aria-checked={inUse} className="talk-choice" onClick={() => void use(c.id)}>
        {mark}
        <span className="talk-choice-text">
          <span className="talk-choice-name">{c.name}</span>
          {aboutLine}
        </span>
        {inUse && <Check size={14} strokeWidth={1.8} className="talk-choice-check" />}
      </button>
      <button className="row-more" aria-label={`More for ${c.name}`} aria-haspopup="menu" aria-expanded={more} onClick={() => showMore(more ? null : c.id)}>
        <Ellipsis size={14} strokeWidth={1.8} />
      </button>
      {more && (
        <div className="computer-more" role="menu" aria-label={`More for ${c.name}`}>
          <button role="menuitem" onClick={() => startRename(c.id)}>Rename…</button>
          <button role="menuitem" onClick={() => openSheet({ kind: 'remove', id: c.id })}>Remove…</button>
        </div>
      )}
    </div>
  );
}
