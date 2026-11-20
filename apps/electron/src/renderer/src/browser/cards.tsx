// The cards over the message box (DecisionBar's .decision-card) the browser adds, and the strips
// that say who has a tab, drawn on their own for the sheets (9, 10). Words: boards 7, 8, 9. Moved
// from the lab (tools/obid-spec/lab-ui/browser/src/cards.tsx) as it is.
import { useState, type ReactNode } from 'react';
import { Hand, KeyRound, Lock, ShieldCheck } from 'lucide-react';
import { Logo, Sheen, agentName } from './pieces';
import { YouMark } from './Panel';
import type { AgentId } from './view';

export function Card({ label, kind, blue, head, from, count, children }: { label: string; kind: string; blue?: boolean; head: ReactNode; from?: string; count?: string; children?: ReactNode }) {
  return (
    <div className="decision-card" role="group" aria-label={label} data-browser="card">
      <div className="tc-ask-line">
        <span className={`tc-ask-kind${blue ? ' is-blue' : ''}`}>{kind}</span>
        <p className="tc-ask-head">{head}{from && <> <span className="tc-ask-from">· {from}</span></>}</p>
        {count && <span className="decision-count">{count}</span>}
      </div>
      {children}
    </div>
  );
}

export const Text = ({ children }: { children: ReactNode }) => <p className="tc-ask-text">{children}</p>;
export const Options = ({ children }: { children: ReactNode }) => <div className="tc-options">{children}</div>;
export const Note = ({ children }: { children: ReactNode }) => <p className="secret-note"><ShieldCheck size={12} strokeWidth={1.6} />{children}</p>;

/** 7: the agent asks you to sign in. */
export function SignInCard({ agent, onTake, onType, onSkip }: { agent: AgentId; onTake?: () => void; onType?: () => void; onSkip?: () => void }) {
  const who = agentName(agent);
  return (
    <Card label={`${who} is asking`} kind="Sign in" head="Sign in to the staging admin" from={who} count="1 of 1">
      <Text>The mail log is behind a sign-in. Take control and sign in yourself, or type the password here: it goes straight into the page, never to {who}.</Text>
      <Options>
        <button className="tc-option is-primary has-icon" onClick={onTake}><Hand size={14} strokeWidth={1.6} />Take control</button>
        <button className="tc-option has-icon" onClick={onType}><KeyRound size={13} strokeWidth={1.6} />Type the password</button>
        <button className="tc-option is-quiet" onClick={onSkip}>Skip</button>
      </Options>
    </Card>
  );
}

/**
 * 8: the password card. What is typed lives only in this field (the DOM input) until Fill in sends
 * it, once, to the page's field (POST secret); the card keeps no copy after that and draws dots.
 */
export function PasswordCard({ agent, host, onFill, onCancel, preview = 0 }: { agent: AgentId; host: string; onFill: (value: string) => void; onCancel: () => void; preview?: number }) {
  const [value, setValue] = useState('');
  const shown = value.length || preview;
  return (
    <Card label="Password for the page" kind="Password" blue head={<>For <b style={{ fontWeight: 500 }}>{host}</b></>} from={`asked by ${agentName(agent)}`}>
      <div className="secret-row">
        <label className="secret-field" aria-label="Password, goes into the page only" style={{ position: 'relative' }}>
          <Lock size={14} strokeWidth={1.6} />
          <span aria-hidden="true">{'•'.repeat(shown)}</span>
          <span className="caret" />
          <input
            type="password" autoComplete="off" spellCheck={false} data-browser="secret-input" value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && value) { onFill(value); setValue(''); } }}
            style={{ position: 'absolute', inset: 0, opacity: 0, width: '100%', border: 0, padding: 0, background: 'none', color: 'transparent' }}
            aria-label="Password"
          />
        </label>
        <button className="tc-option is-primary" onClick={() => { if (value) { onFill(value); setValue(''); } }}>Fill in</button>
        <button className="tc-option is-quiet" onClick={() => { setValue(''); onCancel(); }}>Cancel</button>
      </div>
      <Note>It goes straight into the page’s Password field. {agentName(agent)} never sees it, and Oby doesn’t keep it.</Note>
    </Card>
  );
}

/** The strip under the address, on its own (9's sheet): each way a tab can be held. */
export function Strip({ kind, agent, action }: { kind: 'using' | 'asks' | 'held' | 'yours' | 'idle'; agent?: AgentId; action?: string }) {
  const who = agent ? agentName(agent) : '';
  const logo = agent && <Logo agent={agent} size={14} />;
  const take = <button className="tc-option is-sm">Take control</button>;
  switch (kind) {
    case 'using': return <div className="br-control"><span className="br-control-text">{logo}<span><b>{who}</b> is using this tab · <Sheen>{action}</Sheen></span></span>{take}</div>;
    case 'asks': return <div className="br-control is-needs"><span className="br-control-text">{logo}<span><b>{who}</b> asks you to sign in here</span></span>{take}</div>;
    case 'held': return <div className="br-control is-held"><span className="br-control-text"><YouMark /><span><b>You have this tab.</b> {who} waits until you give it back.</span></span><button className="tc-option is-sm is-primary">Give back</button></div>;
    case 'yours': return <div className="br-control"><span className="br-control-text"><YouMark /><span><b>Your tab.</b> No agent uses it unless you ask.</span></span></div>;
    case 'idle': return <div className="br-control"><span className="br-control-text">{logo}<span><b>{who}</b> opened this tab · idle</span></span>{take}</div>;
  }
}
