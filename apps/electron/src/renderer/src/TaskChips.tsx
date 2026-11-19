import { useState } from 'react';
import { Check, ChevronDown, ShieldCheck } from 'lucide-react';
import { Menu } from './Menu';
import { MODES } from './modes';
import { useChips, type Chips } from './chips';

// The message box's mode chip (docs/contracts/composer.md, COMPOSER-1), as T3 Code's composer has
// it: the mode the tasks these words start run in, a default for this box's words alone, kept with
// its draft (chips.ts). Default leaves it to the owner's Settings; nothing here writes to Settings.
// The model and effort are the talk pill's, on each account's own menu (COMPOSER-17): one Auto.

export default function TaskChips({ box }: { box: string | undefined }) {
  const [chips, setChips] = useChips(box);
  return (
    <span className="task-chips">
      <ModeChip chips={chips} onChange={setChips} />
    </span>
  );
}

function ModeChip({ chips, onChange }: { chips: Chips; onChange: (c: Chips) => void }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  const mode = MODES.find((m) => m.id === chips.mode);
  const pick = (id?: string) => { onChange({ ...chips, mode: id }); setAt(null); };
  return <>
    <button type="button" className={`talk-pick task-chip${at ? ' is-on' : ''}${mode ? '' : ' is-default'}`} aria-haspopup="menu" aria-expanded={!!at}
      aria-label={`Mode for tasks: ${mode?.label ?? 'Default'}. Change`} title={mode?.help ?? 'The mode in Settings'}
      onMouseDown={(e) => e.preventDefault()} onClick={(e) => (at ? setAt(null) : setAt(e.currentTarget.getBoundingClientRect()))}>
      <ShieldCheck size={15} strokeWidth={1.6} />
      <span>{mode?.label ?? 'Default'}</span>
      <ChevronDown size={13} strokeWidth={1.8} />
    </button>
    {at && (
      <Menu at={at} onClose={() => setAt(null)} align="left" width={280} height={5 * 46 + 30}>
        <span className="menu-label">Tasks from these words</span>
        <button role="menuitemradio" aria-checked={!mode} className="talk-choice" onClick={() => pick(undefined)}>
          <span className="talk-choice-text"><span className="talk-choice-name">Default</span><span className="talk-choice-about">The mode in Settings</span></span>
          {!mode && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
        </button>
        {MODES.map((m) => (
          <button key={m.id} role="menuitemradio" aria-checked={m.id === chips.mode} className="talk-choice" onClick={() => pick(m.id)}>
            <span className="talk-choice-text"><span className="talk-choice-name">{m.label}</span><span className="talk-choice-about">{m.help}</span></span>
            {m.id === chips.mode && <Check size={15} strokeWidth={1.8} className="talk-choice-check" />}
          </button>
        ))}
      </Menu>
    )}
  </>;
}
