import type { ReactNode } from 'react';
import { Check } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import { ObyFace } from './ObyFace';
import { AGENTS, type Agent, type SetupStep } from './mock';
import { FIRST_TASK, connectTetrix, joinSphere, pick, signInAgent, signInGitHub, useSetup } from './onboarding';

// A step of setting up, under Oby's words in its first conversation
// (onboarding.ts): a row for each thing to connect, with its button or how it
// stands, then the picks that carry on (only while this step waits on you).
export default function SetupCard({ step }: { step: SetupStep }) {
  const s = useSetup();
  const waiting = s.at === step;
  const mark = (brand: Brand) => <BrandMark brand={brand} size={22} />;

  let rows: ReactNode = null, picks: { label: string; quiet?: boolean; ok?: boolean }[] = [];
  if (step === 'agents') {
    const AGENT_SUB: Record<Agent, [ready: string, out: string]> = {
      claude: ['Signed in · Personal', 'On this Mac, not signed in'],
      codex: ['Signed in · Deskree', 'On this Mac, not signed in'],
      cursor: ['Signed in · Personal', 'On this Mac, not signed in'],
    };
    rows = (Object.keys(AGENTS) as Agent[]).map((a, i) => {
      const st = s.found <= i ? 'looking' : s.agents[a];
      return <Row key={a} mark={mark(AGENTS[a].brand)} name={AGENTS[a].name} state={st} act="Sign in" onAct={() => signInAgent(a)}
        sub={st === 'looking' ? 'Looking' : st === 'waiting' ? `Finish on ${AGENTS[a].name}’s sign-in page` : AGENT_SUB[a][st === 'in' ? 0 : 1]} />;
    });
    picks = [{ label: 'Continue', ok: Object.values(s.agents).includes('in') && s.found >= 3 }];
  } else if (step === 'github') {
    rows = <Row mark={mark(BRANDS.github)} name="GitHub" state={s.github} act="Sign in" onAct={signInGitHub}
      sub={s.github === 'in' ? 'well-prado · 189 repositories' : s.github === 'waiting' ? 'Finish in your browser' : 'Not signed in'} />;
    picks = [{ label: 'Continue', ok: s.github === 'in' }, { label: 'Not now', quiet: true }];
  } else if (step === 'extras') {
    rows = <>
      <Row mark={mark(BRANDS.tetrix)} name="Tetrix" state={s.tetrix} act="Connect" onAct={connectTetrix}
        sub={s.tetrix === 'in' ? 'Connected · every agent gets your code graph' : s.tetrix === 'waiting' ? 'Finish in your browser' : 'Your code as a graph, for every agent'} />
      <Row mark={<ObyFace size={22} />} name="Obysphere" state={s.sphere} act="Join" onAct={joinSphere}
        sub={s.sphere === 'in' ? 'Joined Deskree · 5 members' : s.sphere === 'waiting' ? 'Joining with your invite' : 'Your team’s Obys review each other’s work'} />
    </>;
    picks = s.tetrix === 'in' || s.sphere === 'in' ? [{ label: 'Continue' }] : [{ label: 'Skip for now', quiet: true }];
  } else {
    picks = [{ label: FIRST_TASK }, { label: 'Review my open pull requests' }];
  }

  return (
    <div className="setup">
      {rows && <ul className="setup-rows">{rows}</ul>}
      {waiting && (
        <div className="tc-options">
          {picks.map((p) => (
            <button key={p.label} className={`tc-option${p.quiet ? ' is-quiet' : p.ok !== undefined ? ' is-primary' : ''}`} disabled={p.ok === false} onClick={() => pick(p.label)}>{p.label}</button>
          ))}
        </div>
      )}
    </div>
  );
}

// one thing to connect: its mark, its name and how it stands, and its button until it's ready
const Row = ({ mark, name, sub, state, act, onAct }: { mark: ReactNode; name: string; sub: string; state: 'out' | 'waiting' | 'in' | 'looking'; act: string; onAct: () => void }) => (
  <li className="setup-row">
    {mark}
    <span className="wl-two">
      <span className="wl-name">{name}</span>
      <span className={`wl-sub${state === 'looking' ? ' msg-step' : ''}`}>{sub}</span>
    </span>
    {state === 'in' ? <span className="wl-ready"><Check size={14} strokeWidth={2} />Ready</span>
      : state !== 'looking' && <button className="set-button" disabled={state === 'waiting'} onClick={onAct}>{state === 'waiting' ? 'Waiting…' : act}</button>}
  </li>
);
