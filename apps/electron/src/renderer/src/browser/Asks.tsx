// What an agent asks of you in the browser, over the message box as the conversation's other
// decisions are (docs/contracts/browser-app.md, BROWSERAPP-11 to 13; the design's boards 7, 8, 9):
// a sign-in, the password card that fills the page's field, a CAPTCHA, a choice, a one-time code, a
// passkey. One at a time with "1 of n", as the decision bar shows them. Words are the boards'; an
// agent's own words (its ask's reason) head its card.
import { useState, type ReactNode } from 'react';
import { ChevronRight, Hand } from 'lucide-react';
import { Card, Note, Options, PasswordCard, SignInCard, Text, headOf } from './cards';
import { agentName } from './pieces';
import { composing } from '../shortcuts';
import { useBrowser } from './store';
import type { Ask } from './view';

/** 9: a one-time code, typed into the boxes and straight into the page. */
function CodeCard({ ask, count, onFill, onCancel }: { ask: Ask; count?: ReactNode; onFill: (code: string) => void; onCancel: () => void }) {
  const [code, setCode] = useState('');
  const boxes = Array.from({ length: 6 }, (_, i) => code[i] ?? '');
  const fill = () => { if (code) { onFill(code); setCode(''); } };
  return (
    <Card label="A code for the page" kind="Code" blue head={<>The code sent to your phone, for <b style={{ fontWeight: 500 }}>{ask.host}</b></>} count={count}>
      <div className="secret-row">
        <label className="code-boxes" aria-label="Six-digit code" style={{ position: 'relative' }}>
          {boxes.map((d, i) => <span key={i} className={i === code.length ? 'is-on' : undefined}>{d}</span>)}
          <input
            inputMode="numeric" autoComplete="one-time-code" maxLength={6} value={code} data-browser="code-input" aria-label="Code"
            onChange={(e) => setCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
            onKeyDown={(e) => { if (!composing(e) && e.key === 'Enter') fill(); }}
            style={{ position: 'absolute', inset: 0, opacity: 0, width: '100%', border: 0, padding: 0, background: 'none', color: 'transparent' }}
          />
        </label>
        <button className="tc-option is-primary" style={{ marginLeft: 'auto' }} onClick={fill}>Fill in</button>
        <button className="tc-option is-quiet" onClick={() => { setCode(''); onCancel(); }}>Cancel</button>
      </div>
      <Note>It goes straight into the page. {agentName(ask.agent)} never sees it.</Note>
    </Card>
  );
}

/** The asks of these jobs (none given: every ask, as on the demo data), over the message box. */
export function BrowserAsks({ jobs }: { jobs?: string[] }) {
  const { offered, s, ui, setUi, api } = useBrowser();
  const [at, setAt] = useState(0);
  if (!offered) return null;
  const asks = s.asks.filter((a) => !jobs || !a.job || jobs.includes(a.job));
  if (!asks.length && ui.card !== 'password') return null;
  const i = Math.min(at, Math.max(0, asks.length - 1)), ask = asks[i];
  const n = asks.length;
  const next = n > 1 && <button type="button" className="decision-next" aria-label="Next decision" onClick={() => setAt((i + 1) % n)}><ChevronRight size={14} strokeWidth={1.8} /></button>;
  const count = n ? <>{i + 1} of {n}{next}</> : undefined;
  const who = ask ? agentName(ask.agent) : '';
  const take = () => { if (ask) void api.take(ask.tab); };
  const skip = () => { if (ask) void api.answer(ask.id); };
  const card = (() => {
    if (!ask) return null;
    if (ui.card === 'password' && ask.kind === 'signIn') {
      return (
        <PasswordCard
          agent={ask.agent} host={ask.host} preview={ui.typed}
          onCancel={() => setUi({ card: null, typed: 0 })}
          onFill={(v) => { void api.secret(ask.tab, ask.field ?? 'Password', v, ask.id); }}
        />
      );
    }
    switch (ask.kind) {
      case 'signIn':
        return <SignInCard agent={ask.agent} reason={ask.reason} host={ask.host} count={count} onTake={take} onType={() => setUi({ card: 'password' })} onSkip={skip} />;
      case 'captcha':
        return (
          <Card label={`${who} is asking`} kind="Check" head="The page wants to know you’re a person" from={who} count={count}>
            <Text>{who} can’t answer this. Take control and solve it; {who} goes on when you give the tab back.</Text>
            <Options><button className="tc-option is-primary" onClick={take}>Take control</button><button className="tc-option is-quiet" onClick={skip}>Skip this page</button></Options>
          </Card>
        );
      case 'choice': {
        const { head, lead } = headOf(ask.reason, 'Which one?');
        return (
          <Card label={`${who} is asking`} kind="Question" head={head} from={who} count={count}>
            {lead && <Text>{lead}</Text>}
            <Options>
              {(ask.choices ?? []).map((c, j) => <button key={c} className={`tc-option${j === 0 ? ' is-primary' : ''}`} onClick={() => void api.answer(ask.id, c)}>{c}</button>)}
              <button className="tc-option is-quiet" onClick={take}>I’ll choose in the page</button>
            </Options>
          </Card>
        );
      }
      case 'code':
        return <CodeCard ask={ask} count={count} onFill={(code) => void api.secret(ask.tab, ask.field ?? '', code, ask.id)} onCancel={skip} />;
      case 'passkey':
        return (
          <Card label={`${who} is asking`} kind="Passkey" head={`${ask.host} asks for your passkey`} from={who} count={count}>
            <Text>Only you can do this. Take control; your Mac asks for Touch ID.</Text>
            <Options><button className="tc-option is-primary has-icon" onClick={take}><Hand size={14} strokeWidth={1.6} />Take control</button><button className="tc-option is-quiet" onClick={skip}>Skip</button></Options>
          </Card>
        );
      default:
        return null;
    }
  })();
  if (!card) return null;
  return (
    <div className="decision-bar" role="group" aria-label="Waiting for your decision" data-browser="asks">
      {card}
    </div>
  );
}
