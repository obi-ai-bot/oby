import { useState } from 'react';
import { Check, ChevronDown, ChevronRight, ShieldCheck } from 'lucide-react';
import { Menu } from './Menu';
import { BrandMark } from './brands';
import { MODES } from './modes';
import { EFFORT_NAMES, useAccounts, useModels } from './SettingsView';
import { agentOf, type Agent } from './view';
import { isAgent } from '../../shared/obid-wire';
import { useChips, type Chips } from './chips';

// The message box's chips (docs/contracts/composer.md, COMPOSER-1 and 2), as T3 Code's composer has
// them: the mode the tasks these words start run in, and the tool, model and effort they run on.
// Each is a default for this box's words alone, kept with its draft (chips.ts): Default and Auto
// leave it to the owner's Settings and to Oby, which still picks the tool where the words name one.
// Nothing here writes to Settings or to an account.

const Mark = ({ agent, size }: { agent: string; size: number }) => <BrandMark brand={agentOf(agent).brand} size={size} mono={agent !== 'claude'} />;

export default function TaskChips({ box }: { box: string | undefined }) {
  const [chips, setChips] = useChips(box);
  return (
    <span className="task-chips">
      <ModeChip chips={chips} onChange={setChips} />
      <ModelChip chips={chips} onChange={setChips} />
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

// The tools the owner has an account on (Settings › Agents), each with its models and, beside a
// model, its efforts; Auto first: Oby picks, on each account's own model and effort
function ModelChip({ chips, onChange }: { chips: Chips; onChange: (c: Chips) => void }) {
  const [accounts] = useAccounts();
  const [, options] = useModels();
  const [at, setAt] = useState<DOMRect | null>(null);
  const [efforts, setEfforts] = useState<{ agent: Agent; model: string; top: number } | null>(null);
  const tools = [...new Set(accounts.map((a) => a.agent).filter(isAgent))] as Agent[];
  const modelsOf = (agent: Agent) => options[agent]?.models ?? [];
  const effortsOf = (agent: Agent, model: string) => (agent === 'cursor' ? [] : modelsOf(agent).find((m) => m.id === model)?.efforts ?? options[agent]?.efforts ?? []);
  const chosen = chips.agent && isAgent(chips.agent) ? modelsOf(chips.agent).find((m) => m.id === chips.model) : undefined;
  const words = chips.agent ? `${chosen?.name ?? chips.model ?? agentOf(chips.agent).name}${chips.effort ? ` · ${EFFORT_NAMES[chips.effort] ?? chips.effort}` : ''}` : 'Auto';
  const close = () => { setAt(null); setEfforts(null); };
  const pick = (c: Chips) => { onChange({ ...chips, ...c }); close(); };
  return <>
    <button type="button" className={`talk-pick task-chip${at ? ' is-on' : ''}${chips.agent ? '' : ' is-default'}`} aria-haspopup="menu" aria-expanded={!!at}
      aria-label={`Model for tasks: ${chips.agent ? `${agentOf(chips.agent).name} ${words}` : 'Auto'}. Change`} title={chips.agent ? `${agentOf(chips.agent).name} · ${words}` : 'Oby picks the tool, on each account’s own model'}
      onMouseDown={(e) => e.preventDefault()} onClick={(e) => (at ? close() : setAt(e.currentTarget.getBoundingClientRect()))}>
      {chips.agent && <Mark agent={chips.agent} size={15} />}
      <span>{words}</span>
      <ChevronDown size={13} strokeWidth={1.8} />
    </button>
    {at && (
      <Menu at={at} onClose={close} align="left" width={240} height={40 + tools.reduce((n, t) => n + 26 + modelsOf(t).length * 30, 0)} scrollPart>
        <div className="menu-scroll task-models" role="group" aria-label="Models">
          <button role="menuitemradio" aria-checked={!chips.agent} onPointerEnter={() => setEfforts(null)} onClick={() => pick({ agent: undefined, model: undefined, effort: undefined })}>
            Auto<span className="menu-sub-value">Oby picks</span>{!chips.agent && <Check size={14} strokeWidth={1.8} className="menu-check" />}
          </button>
          {tools.map((tool) => (
            <span key={tool} role="group" aria-label={agentOf(tool).name}>
              <span className="menu-label task-tool"><Mark agent={tool} size={13} />{agentOf(tool).name}</span>
              {modelsOf(tool).map((m) => {
                const on = chips.agent === tool && chips.model === m.id, more = effortsOf(tool, m.id);
                return (
                  <span key={m.id} className="menu-sub-anchor">
                    <button role="menuitemradio" aria-checked={on} aria-haspopup={more.length ? 'menu' : undefined}
                      onPointerEnter={(e) => setEfforts(more.length ? { agent: tool, model: m.id, top: e.currentTarget.offsetTop } : null)}
                      // the keys too, as the talk pill's models open: Right opens the efforts beside it
                      onKeyDown={(e) => { if (e.key === 'ArrowRight' && more.length) { e.preventDefault(); setEfforts({ agent: tool, model: m.id, top: e.currentTarget.offsetTop }); } }}
                      onClick={() => pick({ agent: tool, model: m.id, effort: on ? chips.effort : undefined })}>
                      {m.name}{on && <Check size={14} strokeWidth={1.8} className="menu-check" />}
                      {more.length > 0 && <ChevronRight size={12} strokeWidth={1.8} className="task-more" />}
                    </button>
                  </span>
                );
              })}
            </span>
          ))}
        </div>
        {efforts && (
          <span className="row-menu row-submenu is-scroll task-efforts" style={{ top: efforts.top }} role="menu" aria-label="Effort">
            <button role="menuitemradio" aria-checked={!(chips.agent === efforts.agent && chips.model === efforts.model && chips.effort)} onClick={() => pick({ agent: efforts.agent, model: efforts.model, effort: undefined })}>
              Account’s effort{!(chips.agent === efforts.agent && chips.model === efforts.model && chips.effort) && <Check size={14} strokeWidth={1.8} className="menu-check" />}
            </button>
            {effortsOf(efforts.agent, efforts.model).map((e) => {
              const on = chips.agent === efforts.agent && chips.model === efforts.model && chips.effort === e;
              return (
                <button key={e} role="menuitemradio" aria-checked={on} onClick={() => pick({ agent: efforts.agent, model: efforts.model, effort: e })}>
                  {EFFORT_NAMES[e] ?? e}{on && <Check size={14} strokeWidth={1.8} className="menu-check" />}
                </button>
              );
            })}
          </span>
        )}
      </Menu>
    )}
  </>;
}
