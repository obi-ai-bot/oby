import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Check } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import SphereTile from './SphereTile';
import { SAMPLE, memberCount } from './spheres';
import { tetrixFor } from './knowledge';
import { SAMPLE_TALK, Sphere, SphereHowTo, still } from './Welcome';
import { AGENTS, type Agent, type SetupStep } from './mock';
import { FIRST_TASK, accountOf, cancelAgent, connectTetrix, pick, sceneDone, sceneReplayed, sendAgentCode, signInAgent, signInGitHub, useSetup, type Code } from './onboarding';
import { onObid } from './lean';
import { PasteCode } from './SettingsView';
import { say, useLive } from './live';
import { api, type Tetrix } from './obid';
import { TetrixWindow } from './Connectors';

// a sign-in's code to type on its page, after what the row says
const typed = (c?: Code) => (c?.userCode ? ` · enter ${c.userCode}` : '');

// A step of setting up, under Oby's words in its first conversation
// (onboarding.ts): a row for each thing to connect, with its button or how it
// stands, then the picks that carry on (only while this step waits on you).
// The Obysphere tour's cards (the sample's) become one line once it's put away.
export default function SetupCard({ step }: { step: SetupStep }) {
  const s = useSetup();
  const repos = useLive().repos.length;
  const waiting = s.at === step;
  const mark = (brand: Brand) => <BrandMark brand={brand} size={22} />;
  // a scene played to its end (Show me, a sample conversation): what to click next shows then.
  // Kept apart from the card, which the conversation may draw again, so it doesn't play twice
  const [heard, setHeard] = useState(() => PLAYED.has(step));
  const ended = () => { PLAYED.add(step); setHeard(true); setReplaying(false); };
  // Show me: Replay plays it again (a new key) without moving the tour; Skip goes to its end
  const [run, setRun] = useState(0), [skipped, setSkipped] = useState(false), [replaying, setReplaying] = useState(false);
  // (picks come in under the conversation's end, so they're brought into view)
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => { if (waiting) end.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }); }, [waiting, heard]);

  if (s.sphere === 'gone' && SAMPLE_STEPS.some((x) => step.startsWith(x))) return <p className="setup-label setup-gone">Sample · put away</p>;
  let rows: ReactNode = null, body: ReactNode = null, cue = '', picks: { label: string; quiet?: boolean; ok?: boolean }[] = [];
  if (step === 'agents') {
    rows = <AgentRows />;
    picks = [{ label: 'Continue', ok: Object.values(s.agents).includes('in') && s.found >= 3 }];
  } else if (step === 'github') {
    rows = <Row mark={mark(BRANDS.github)} name="GitHub" state={s.github} act="Sign in" onAct={signInGitHub}
      sub={s.github === 'in' ? (onObid ? `${s.githubUser ?? 'Signed in'}${repos ? ` · ${repos} repositories` : ''}` : 'well-prado · 189 repositories')
        : s.github === 'waiting' ? `Finish in your browser${typed(s.codes.github)}` : 'Not signed in'} />;
    picks = [{ label: 'Continue', ok: s.github === 'in' }, { label: 'Not now', quiet: true }];
  } else if (step === 'extras') {
    rows = <>
      <TetrixRow bare={false}
        sub={s.tetrix === 'in' ? 'Connected · every agent gets your code graph' : s.tetrix === 'waiting' ? 'Finish in your browser' : 'Your code as a graph, for every agent'} />
    </>;
    picks = s.tetrix === 'in' ? [{ label: 'Continue' }] : [{ label: 'Skip for now', quiet: true }];
  } else if (step === 'sphere') {
    picks = [{ label: 'Show me', ok: true }, { label: 'Skip', quiet: true }];
  } else if (step === 'showme') {
    // it goes on by itself once played; with reduced motion, its frames wait for Continue
    const calm = still();
    body = <>
      <p className="setup-label">Example</p>
      <SphereHowTo key={run} skip={skipped || (heard && !replaying)} onEnd={() => { ended(); if (!calm) sceneDone(); }} />
      {!calm && <div className="tc-options">
        {heard && !replaying
          ? <button className="tc-option is-quiet" onClick={() => { sceneReplayed(); setRun(run + 1); setSkipped(false); setReplaying(true); }}>Replay</button>
          : <button className="tc-option is-quiet" onClick={() => setSkipped(true)}>Skip</button>}
      </div>}
    </>;
    if (calm) picks = [{ label: 'Continue', ok: true }];
  } else if (step === 'sample') {
    rows = <Row mark={<span className="is-sample"><SphereTile name={SAMPLE.name} brand={SAMPLE.brand} /></span>} name={SAMPLE.name} state="out"
      sub={`Sample · ${memberCount(SAMPLE)} members`} act={waiting ? 'Take a look' : undefined} onAct={() => pick('Take a look')} />;
    cue = 'It’s on the left, in the side panel.';
    picks = [{ label: 'Not now', quiet: true }];
  } else if (step.startsWith('convo:')) {
    const who = step.slice(6), { title, talk } = SAMPLE_TALK[who];
    body = <><p className="setup-label">Sample conversation · {title}</p><Sphere talk={talk} peer={who} loop={false} skip={heard} onEnd={ended} /></>;
    if (heard) {
      cue = CUES[s.call ?? ''] ?? '';
      picks = [{ label: s.call === 'Knowledge' ? 'Knowledge' : `${s.call}’s`, ok: true }, { label: 'Done looking', quiet: true }];
    }
  } else if (step === 'knowledge') {
    const tetrix = tetrixFor('deskree-inc', 'Wellington');
    rows = <Row mark={mark(BRANDS.tetrix)} name={`Tetrix · ${tetrix.org}`} state="out" sub="Sample · connected by Wellington · every member’s Oby can use it" />;
    cue = tetrix.tools.filter((t) => t.on).map((t) => t.name).join(', ');
    picks = [{ label: 'Continue', ok: true }];
  } else {
    picks = [{ label: FIRST_TASK }, { label: 'Review my open pull requests' }];
  }

  return (
    <div className="setup">
      {rows && <ul className="setup-rows">{rows}</ul>}
      {body}
      {waiting && cue && <p className="setup-label setup-cue">{cue}</p>}
      {waiting && picks.length > 0 && (
        <div ref={end} className="tc-options">
          {picks.map((p) => (
            <button key={p.label} className={`tc-option${p.quiet ? ' is-quiet' : p.ok !== undefined ? ' is-primary' : ''}`} disabled={p.ok === false} onClick={() => pick(p.label)}>{p.label}</button>
          ))}
        </div>
      )}
    </div>
  );
}

// The tools on this Mac, one row each as Oby looks at them (onboarding.ts's found): how each
// stands and its sign-in. Here, and on the agents step of the new onboarding (FirstRun.tsx)
const AGENT_SUB: Record<Agent, [ready: string, out: string]> = {
  claude: ['Signed in · Personal', 'On this Mac, not signed in'],
  codex: ['Signed in · Deskree', 'On this Mac, not signed in'],
  cursor: ['Signed in · Personal', 'On this Mac, not signed in'],
};
// bare (the new onboarding): the name alone, nothing under it; how it stands is on the right: a
// spinner while Oby looks, or while a sign-in is under way (with the code to type on its page,
// or Claude Code's field for the one its page shows), the button, or a check once it's ready
export function AgentRows({ bare = false }: { bare?: boolean }) {
  const s = useSetup();
  return <>{(Object.keys(AGENTS) as Agent[]).map((a, i) => {
    const st = s.found <= i ? 'looking' : s.agents[a];
    // on obid: the account obid signed in, the code its page wants, Claude Code's code to paste
    const sub = bare ? undefined : st === 'looking' ? 'Looking' : st === 'waiting' ? `Finish on ${AGENTS[a].name}’s sign-in page${typed(s.codes[a])}`
      : onObid && st === 'in' ? `Signed in · ${accountOf(a) ?? AGENTS[a].name}` : AGENT_SUB[a][st === 'in' ? 0 : 1];
    return <Row key={a} mark={<BrandMark brand={AGENTS[a].brand} size={22} />} name={AGENTS[a].name} state={st} act={bare ? 'Connect' : 'Sign in'} onAct={() => signInAgent(a)}
      sub={sub} why={s.problems[a]} code={bare ? s.codes[a]?.userCode ?? undefined : undefined} bare={bare}
      onCancel={bare && st === 'waiting' ? () => cancelAgent(a) : undefined} canCancel={!!s.codes[a]}>
      {st === 'waiting' && s.codes[a]?.paste ? <PasteCode onCode={(code) => sendAgentCode(a, code)} /> : undefined}
    </Row>;
  })}</>;
}

const PLAYED = new Set<SetupStep>();
// the tour's cards with the sample in them; and what to click next, under a sample conversation
const SAMPLE_STEPS = ['showme', 'sample', 'convo:', 'knowledge'];
const CUES: Record<string, string> = {
  Ernesto: 'Ernesto’s Oby asked yours for a review too.',
  Dima: 'Dima’s Oby asked yours something too.',
  Wellington: 'Wellington’s asked for something only you can OK.',
  Knowledge: 'Last one: Knowledge, what Deskree’s Obys can look up.',
};

// under way: the code to type on its page, if any, and a spinner
const busy = (name: string, state: 'looking' | 'waiting', code?: string) => <>
  {code && <span className="wl-code" aria-label={`Enter ${code} on its page`}>{code}</span>}
  <Spinner label={state === 'looking' ? `Looking for ${name}` : `Signing in to ${name}`} />
</>;
// one thing to connect: its mark, its name and how it stands (and anything it needs from you
// there, why its last try didn't take), and its button until it's ready (none: nothing to do
// there); under way, its Cancel where the button was (on once there's something to cancel)
const Row = ({ mark, name, sub, why, code, state, act, onAct, onCancel, canCancel = true, bare = false, children }: { mark: ReactNode; name: string; sub?: string; why?: string; code?: string; state: 'out' | 'waiting' | 'in' | 'looking'; act?: string; onAct?: () => void; onCancel?: () => void; canCancel?: boolean; bare?: boolean; children?: ReactNode }) => (
  <li className="setup-row">
    {mark}
    <span className="wl-two">
      <span className="wl-name">{name}</span>
      {sub && <span className={`wl-sub${state === 'looking' ? ' msg-step' : ''}`}>{sub}</span>}
      {why && state === 'out' && <span className="wl-sub is-problem" role="alert">{why}</span>}
      {!bare && children}
    </span>
    {bare && (state === 'looking' || state === 'waiting') ? (onCancel
      // (Claude Code's field for its code: a line of its own under the rest)
      ? <><span className="wl-busy">{busy(name, state, code)}<button className="set-button" disabled={!canCancel} aria-label={`Cancel signing in to ${name}`} onClick={onCancel}>Cancel</button></span>
        {children && <div className="wl-below">{children}</div>}</>
      : <span className="wl-busy wl-slot">{busy(name, state, code)}</span>)
    : state === 'in' ? (bare
      ? <span className="wl-ready is-bare wl-slot" role="img" aria-label="Ready"><Check size={20} strokeWidth={2.25} /></span>
      : <span className="wl-ready"><Check size={14} strokeWidth={2} />Ready</span>)
      : state !== 'looking' && act && <button className="set-button" disabled={state === 'waiting'} onClick={onAct}>{state === 'waiting' ? 'Waiting…' : act}</button>}
  </li>
);

// GitHub on the new onboarding's tools step (FirstRun.tsx), as an agent's row there: Connect (gh's
// own device sign-in, through obid), its code and a spinner while that's under way, then a check
export function GitHubRow() {
  const s = useSetup();
  return <Row mark={<BrandMark brand={BRANDS.github} size={22} />} name="GitHub" state={s.github} act="Connect" onAct={signInGitHub}
    code={s.codes.github?.userCode ?? undefined} bare />;
}

// Tetrix on the new onboarding's knowledge step, the same way: a spinner while it's under way, then
// a check. On obid, Connect opens Settings' own Connect Tetrix window, where the person gives their
// Tetrix's address (Oby has none built in); Continue in Browser signs in from the row
export function TetrixRow({ sub, bare = true }: { sub?: string; bare?: boolean }) {
  const s = useSetup();
  const [asking, setAsking] = useState<Tetrix | null>(null);
  const connect = onObid ? () => void api.tetrix().then(setAsking, say) : () => void connectTetrix();
  return <>
    <Row mark={<BrandMark brand={BRANDS.tetrix} size={22} />} name="Tetrix" sub={sub} state={s.tetrix} act="Connect" onAct={connect} bare={bare} />
    {asking && <TetrixWindow tetrix={asking} waiting={false} onConnect={(to) => { setAsking(null); void connectTetrix(to); }}
      onCancel={() => setAsking(null)} onClose={() => setAsking(null)} />}
  </>;
}

// something under way: a ring going round (still, with reduced motion: app.css)
export const Spinner = ({ label }: { label: string }) => (
  <svg className="spinner" width="18" height="18" viewBox="0 0 18 18" role="img" aria-label={label}>
    <circle cx="9" cy="9" r="7" fill="none" stroke="currentColor" strokeOpacity="0.2" strokeWidth="1.75" />
    <path d="M9 2 a7 7 0 0 1 7 7" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
  </svg>
);
