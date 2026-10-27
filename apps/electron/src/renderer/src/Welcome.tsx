import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type Ref } from 'react';
import { Check, MousePointer2 } from 'lucide-react';
import { ChatsCircleIcon, PlusIcon } from '@phosphor-icons/react';
import Oby from './Oby';
import { BRANDS, BrandMark } from './brands';
import { ObyFace, OwnerAvatar } from './ObyFace';
import SphereTile from './SphereTile';
import { CAN_CREATE, SUFFIX, slugOf } from './spheres';
import { clock } from './stream';
import { AGENTS, PEER_CONVOS, type Agent } from './mock';

// First launch: what Oby found signed in on this Mac, then four short scenes of
// what it does with them. Each scene plays as beats; the ones that tell a
// story start over after a pause, the ones you act in wait for you.

// a beat every `ms`, up to `last`; looping, it holds a few beats and starts over.
// It stops at its end when it doesn't loop, and while off: a scene left in a
// conversation costs nothing
function useBeat(last: number, ms: number, loop = true, on = true) {
  const [beat, setBeat] = useState(0);
  const run = on && (loop || beat < last);
  useEffect(() => {
    if (!run) return;
    const t = setInterval(() => setBeat((b) => (b < last ? b + 1 : loop && b < last + 4 ? b + 1 : loop ? 0 : b)), ms);
    return () => clearInterval(t);
  }, [last, ms, loop, run]);
  return beat;
}
// reduced motion: scenes show their last frame, nothing timed
export const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

const Dot = ({ state }: { state: 'working' | 'needs' }) => <span className="status"><span className={`status-dot is-${state}`} /></span>;

// 1. What's signed in. Each tool keeps its own login; Oby only learns it worked.
const FOUND: { agent: Agent; detail: string; ready: boolean }[] = [
  { agent: 'claude', detail: 'Signed in · Personal and Deskree', ready: true },
  { agent: 'codex', detail: 'Signed in · Deskree', ready: true },
  { agent: 'cursor', detail: 'Signed in · Personal', ready: false },
];
function Found() {
  const beat = useBeat(FOUND.length * 2, 550, false);
  const [cursor, setCursor] = useState<'out' | 'waiting' | 'in'>('out');
  useEffect(() => {
    if (cursor !== 'waiting') return;
    const t = setTimeout(() => setCursor('in'), 1800);  // ponytail: the mock's stand-in for Cursor's own sign-in page
    return () => clearTimeout(t);
  }, [cursor]);
  return (
    <>
      <ul className="wl-found">
        {FOUND.map((f, i) => {
          const known = beat > i * 2 + 1, ready = f.ready || cursor === 'in';
          return (
            <li key={f.agent} className={`wl-card wl-agent${beat > i * 2 ? ' is-in' : ''}`}>
              <BrandMark brand={AGENTS[f.agent].brand} size={24} />
              <span className="wl-two">
                <span className="wl-name">{AGENTS[f.agent].name}</span>
                {!known ? <span className="wl-sub msg-step">Looking</span>
                  : <span className="wl-sub">{ready ? f.detail : cursor === 'waiting' ? 'Finish on Cursor’s sign-in page' : 'On this Mac, not signed in'}</span>}
              </span>
              {known && (ready
                ? <span className="wl-ready"><Check size={14} strokeWidth={2} />Ready</span>
                : <button className="set-button" disabled={cursor === 'waiting'} onClick={() => setCursor('waiting')}>{cursor === 'waiting' ? 'Waiting…' : 'Sign in'}</button>)}
            </li>
          );
        })}
      </ul>
      <p className={`wl-note${beat >= FOUND.length * 2 ? ' is-in' : ''}`}>Each one keeps its own login. I never see your passwords.</p>
    </>
  );
}

// 2. Oby hands each request to an agent and an account
const SENT: { text: string; agent: Agent; account: string }[] = [
  { text: 'Fix the flaky invite test', agent: 'claude', account: 'Personal' },
  { text: 'Review PR #412', agent: 'codex', account: 'Deskree' },
  { text: 'Polish the search page', agent: 'cursor', account: 'Personal' },
];
function Dispatch() {
  const beat = useBeat(SENT.length * 2, 700);
  return (
    <ul className="wl-lanes">
      {SENT.map((s, i) => (
        <li key={s.text} className={`wl-lane${beat > i * 2 ? ' is-asked' : ''}${beat > i * 2 + 1 ? ' is-sent' : ''}`}>
          <span className="wl-bubble">{s.text}</span>
          <span className="wl-wire"><span className="wl-spark" /></span>
          <span className="wl-to">
            <BrandMark brand={AGENTS[s.agent].brand} size={20} />
            <span className="wl-two"><span className="wl-name">{AGENTS[s.agent].name}</span><span className="wl-sub">{s.account}</span></span>
            <Dot state="working" />
          </span>
        </li>
      ))}
    </ul>
  );
}

// 3. An account runs out; the task moves to the next one, same branch
const USED = [64, 76, 88, 97, 100];  // Personal, beat by beat, until it's out
const ROW = 60;  // an account's row and the gap under it
function Accounts() {
  const beat = useBeat(USED.length + 1, 800);
  const moved = beat > USED.length;
  const rows = [
    { agent: 'claude' as Agent, account: 'Personal', used: USED[Math.min(beat, USED.length - 1)] },
    { agent: 'claude' as Agent, account: 'Deskree', used: moved ? 21 : 12 },
    { agent: 'codex' as Agent, account: 'Deskree', used: 34 },
  ];
  return (
    <>
      <div className="wl-accounts">
        {rows.map((r) => (
          <div key={r.agent + r.account} className="wl-account">
            <BrandMark brand={AGENTS[r.agent].brand} size={20} />
            <span className="wl-two"><span className="wl-name">{AGENTS[r.agent].name}</span><span className="wl-sub">{r.account}</span></span>
            <span className={`wl-gauge${r.used >= 90 ? ' is-over' : r.used >= 80 ? ' is-full' : ''}`}><span style={{ width: `${r.used}%` }} /></span>
            <span className="wl-pct">{r.used}%</span>
          </div>
        ))}
        <span className="wl-card wl-job" style={{ top: moved ? ROW + 10 : 10 }}><Dot state="working" />Fix the flaky invite test</span>
      </div>
      <p className={`wl-note${moved ? ' is-in' : ''}`}>Personal ran out, so Deskree carried on from the same branch.</p>
    </>
  );
}

// 4. Oby does the rest by itself and asks once, at the step that's yours.
// Until it has learned how a repository merges, merging is that step.
const DID = ['Wrote the fix in 3 files', 'Ran pnpm test invites · 42 passed', 'Checked for secrets · clean', 'Opened PR #418', 'Reviewed it · nothing to fix'];
function Autonomy() {
  const beat = useBeat(DID.length + 1, 600, false);
  const [step, setStep] = useState<'ask' | 'merged' | 'learned'>('ask');
  return (
    <div className="wl-did">
      <ul>
        {DID.map((d, i) => <li key={d} className={beat > i ? 'is-in' : ''}><Check size={14} strokeWidth={2} />{d}</li>)}
      </ul>
      <div className={`wl-card wl-decide${beat > DID.length ? ' is-in' : ''}`}>
        {step === 'ask' && <><Dot state="needs" /><span className="wl-decide-text">Merge #418 into dev?</span><button className="modal-primary" onClick={() => setStep('merged')}>Merge</button></>}
        {step === 'merged' && <><Check size={14} strokeWidth={2} /><span className="wl-decide-text">Merged. Can I merge reviewed PRs here by myself next time?</span><button className="modal-primary" onClick={() => setStep('learned')}>Yes</button></>}
        {step === 'learned' && <><Check size={14} strokeWidth={2} /><span className="wl-decide-text">Got it. Next time I’ll merge and tell you.</span></>}
      </div>
    </div>
  );
}

// 5. Obys ask each other for reviews; a person steps in only where GitHub needs one.
// A line is your Oby's, the other Oby's (peer), or yours, the person's (you)
type Line = { peer?: boolean; you?: boolean; text: string };
const TALK: Line[] = [
  { text: 'Can you review #418 in tetrix-ee?' },
  { peer: true, text: 'On it. Reviewing with Codex.' },
  { peer: true, text: 'Nothing to fix. Ernesto approved it.' },
  { text: 'Thanks. Merged into dev.' },
];
// The sample Obysphere's conversations (onboarding.ts's tour), one per teammate:
// this scene's, and two of the demo's own (mock.ts), word for word
const peerTalk = (id: string): Line[] => PEER_CONVOS.find((c) => c.id === id)!.messages.map((m) => ({ peer: m.who !== 'oby' && m.who !== 'you', you: m.who === 'you', text: m.text }));
export const SAMPLE_TALK: Record<string, { title: string; talk: Line[] }> = {
  Ernesto: { title: 'Review of #418', talk: TALK },
  Dima: { title: 'Shared graph fixture', talk: peerTalk('p12') },
  Wellington: { title: 'Status bar states for the island', talk: peerTalk('p1') },
};
// onEnd: once its last line is in (not looping)
// skip: straight to its end (played already)
export function Sphere({ talk = TALK, peer = 'Ernesto', loop = true, skip = false, onEnd }: { talk?: Line[]; peer?: string; loop?: boolean; skip?: boolean; onEnd?: () => void } = {}) {
  const calm = still() || skip;
  const beat = useBeat(talk.length, 1100, loop, !calm);
  const b = calm ? talk.length : beat, ended = b >= talk.length;
  useEffect(() => { if (ended) onEnd?.(); }, [ended]);
  return (
    <div className="wl-talk">
      <div className="wl-talk-head">
        <span><ObyFace size={22} />Your Oby</span>
        <span>{peer}’s Oby<OwnerAvatar name={peer} size={22} /></span>
      </div>
      <div className="wl-msgs">
        {talk.map((m, i) => <span key={m.text} className={`wl-bubble${m.peer ? ' is-peer' : ''}${m.you ? ' is-you' : ''}${b > i ? ' is-in' : ''}`}>{m.you && <b>You</b>}{m.text}</span>)}
      </div>
    </div>
  );
}

// Show me (the Obysphere tour): making one with the +, then joining one from a
// teammate's invite link, drawn small with Sangam's own pieces (never the real
// side panel or dialog, which stay as they are). It plays once, a beat every
// 100ms, as MausBot's reel does (a pointer, typing, click ripples), then holds
// its last frame; with reduced motion, its two frames side by side, nothing
// timed. skip: straight to its end; onEnd: once it's there
const HOW_LAST = 98;
const CLICKS = [15, 37, 69, 81];  // on the +, Create, the link, Join
const TEAM = 'My team';
// who makes one, as the + is shown or not (spheres.ts), as Oby's words over it say
const MAKE = CAN_CREATE ? ['Make one', 'Made. You’re its admin, and you invite the rest.'] : ['Someone on your team makes one', 'Made. They’re its admin, and they invite the rest.'];
const HOW_LABEL = 'Example: making an Obysphere with the + and a name, then joining one from a teammate’s invite link';
export function SphereHowTo({ skip = false, onEnd }: { skip?: boolean; onEnd?: () => void }) {
  const calm = still();
  const beat = useBeat(HOW_LAST, 100 / clock.speed, false, !calm && !skip);
  const b = calm || skip ? HOW_LAST : beat, join = b >= 56;
  useEffect(() => { if (b >= HOW_LAST) onEnd?.(); }, [b >= HOW_LAST]);
  // where the pointer is, by beat, measured, so it lands on its target however wide the column
  // is; it glides only while it shows, so it appears at the stage's centre
  const target = b < 4 || (b >= 56 && b < 62) ? 'centre' : b < 30 ? 'plus' : b < 56 ? 'create' : b < 75 ? 'link' : 'join';
  const shown = (b >= 3 && b < 53) || (b >= 61 && b < HOW_LAST);
  const stage = useRef<HTMLDivElement>(null), refs = { plus: useRef<HTMLSpanElement>(null), create: useRef<HTMLSpanElement>(null), link: useRef<HTMLSpanElement>(null), join: useRef<HTMLSpanElement>(null) };
  const [at, setAt] = useState({ x: 0, y: 0 });
  useLayoutEffect(() => {
    const box = stage.current?.getBoundingClientRect(), el = target !== 'centre' ? refs[target].current : null;
    if (!box) return;
    const r = el?.getBoundingClientRect() ?? box;
    setAt({ x: r.left + r.width / 2 - box.left, y: r.top + r.height / 2 - box.top });
  }, [target]);
  if (calm) return <HowStill />;
  const typed = TEAM.slice(0, Math.max(0, b - 19));
  const caption = b < 43 ? MAKE[0] : b < 56 ? MAKE[1] : b < 92 ? 'Or join one' : 'In. Your Oby can now ask theirs.';
  return (
    <div ref={stage} className="how-scene" role="img" aria-label={HOW_LABEL}>
      <MiniRail plusRef={refs.plus} hover={b >= 11 && b < 17}
        tile={!join && b >= 40 ? <SphereTile key="mine" name={TEAM} /> : join && b >= 88 ? <SphereTile key="deskree" name="Deskree" brand={BRANDS.deskree} /> : undefined} />
      <div key={join ? 'join' : 'make'} className="how-area">
        <p key={caption} className="how-caption">{caption}</p>
        {!join ? b >= 17 && b < 40 && <CreateCard name={typed} typing={b < 27} ok={b >= 27} pressed={b === 37} going={b >= 39} buttonRef={refs.create} />
          : <>
            <Invite linkRef={refs.link} />
            {b >= 71 && <JoinRow state={b < 81 ? 'join' : b < 88 ? 'joining' : 'joined'} buttonRef={refs.join} />}
          </>}
      </div>
      {CLICKS.filter((c) => b >= c && b < c + 9).map((c) => <span key={c} className="how-ripple" style={{ left: at.x, top: at.y }} />)}
      <span className={`how-pointer${shown ? ' is-on' : ''}${CLICKS.includes(b) ? ' is-pressing' : ''}`} style={{ transform: `translate(${at.x - 3}px, ${at.y - 3}px)` }}>
        <MousePointer2 size={16} strokeWidth={1.6} fill="currentColor" />
      </span>
    </div>
  );
}
// its two frames, as they end
const HowStill = () => (
  <div className="how" role="img" aria-label={HOW_LABEL}>
    <figure className="how-frame">
      <div className="how-stage">
        <MiniRail tile={<SphereTile name={TEAM} />} />
        <CreateCard name={TEAM} ok />
      </div>
      <figcaption>{MAKE[0]}: + → name it → Create Obysphere</figcaption>
    </figure>
    <figure className="how-frame">
      <div className="how-stage is-join">
        <Invite />
        <JoinRow state="joined" />
      </div>
      <figcaption>Or join one: open the invite link → Join</figcaption>
    </figure>
  </div>
);
// the side panel's icon strip, small: Conversations, the line, an Obysphere, the +
const MiniRail = ({ tile, plusRef, hover = false }: { tile?: ReactNode; plusRef?: Ref<HTMLSpanElement>; hover?: boolean }) => (
  <span className="how-rail">
    <span className="rail-button"><ChatsCircleIcon size={20} /></span>
    <span className="rail-sep" />
    {tile && <span className="rail-button is-on how-pop">{tile}</span>}
    <span ref={plusRef} className={`rail-button${hover ? ' is-hover is-tip' : ''}`} data-tip="Create an Obysphere"><PlusIcon size={20} /></span>
  </span>
);
// CreateSphereModal's card, without its logo and invites
const CreateCard = ({ name, typing = false, ok = false, pressed = false, going = false, buttonRef }: { name: string; typing?: boolean; ok?: boolean; pressed?: boolean; going?: boolean; buttonRef?: Ref<HTMLSpanElement> }) => (
  <div className={`modal how-card${going ? ' is-going' : ''}`}>
    <h2 className="how-title">Create an Obysphere</h2>
    <span className="modal-label">Name</span>
    <span className="modal-field how-field">{name}{typing && <span className="how-caret" />}</span>
    <span className="modal-label sphere-label">Obysphere URL</span>
    <span className="modal-field how-field">{slugOf(name)}<span className="sphere-suffix">{SUFFIX}</span></span>
    <p className={`sphere-note${ok ? ' is-ok' : ''}`}>{ok ? <><Check size={13} strokeWidth={2} />Available</> : 'Letters, numbers and hyphens'}</p>
    <span ref={buttonRef} className={`modal-primary how-create${pressed ? ' is-pressed' : ''}`}>Create Obysphere</span>
  </div>
);
// a teammate's invite, as InviteModal's link reads
const Invite = ({ linkRef }: { linkRef?: Ref<HTMLSpanElement> }) => (
  <p className="how-invite"><OwnerAvatar name="Wellington" size={22} /><span>Join us on Oby: <span ref={linkRef} className="tok-url">deskree.obysphere.com/join/k3f9q2</span></span></p>
);
// the Mac's Join screen, in a row
const JoinRow = ({ state, buttonRef }: { state: 'join' | 'joining' | 'joined'; buttonRef?: Ref<HTMLSpanElement> }) => (
  <div className="setup-row how-pop">
    <SphereTile name="Deskree" brand={BRANDS.deskree} />
    <span className="wl-two"><span className="wl-name">Join Deskree</span><span className="wl-sub">deskree.obysphere.com · 5 members</span></span>
    {state === 'join' ? <span ref={buttonRef} className="modal-primary">Join</span>
      : state === 'joining' ? <span className="wl-sub msg-step">Joining…</span>
      : <span className="wl-ready"><Check size={14} strokeWidth={2} />Joined</span>}
  </div>
);

const SCENES: { title: string; line: string; body: () => ReactNode }[] = [
  { title: 'Hi, I’m Oby.', line: 'I hand your coding work to the agents you already use. Here’s what’s signed in on this Mac.', body: Found },
  { title: 'I don’t write the code. They do.', line: 'Tell me what you want. I pick the agent and the account, hand it over and keep track of it.', body: Dispatch },
  { title: 'When one runs out, another takes over.', line: 'Your work keeps going across all your logins. Nothing to babysit.', body: Accounts },
  { title: 'I only stop you when it matters.', line: 'Everything else I just do, then tell you what I did.', body: Autonomy },
  { title: 'Obys talk to each other.', line: 'In your Obysphere, your teammates’ Obys review your pull requests and yours reviews theirs. Soon they’ll share the work too.', body: Sphere },
];

export default function Welcome({ onDone }: { onDone: () => void }) {
  const [at, setAt] = useState(0);
  const last = SCENES.length - 1;
  const next = () => (at === last ? onDone() : setAt(at + 1));
  const back = () => setAt(Math.max(0, at - 1));
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onDone();
      else if (e.key === 'ArrowRight') next();
      else if (e.key === 'ArrowLeft') back();
    };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  });
  const Scene = SCENES[at].body;
  return (
    <div className={`welcome${at > 0 ? ' is-small' : ''}`}>
      <div className="welcome-bar"><button className="welcome-skip" onClick={onDone}>Skip</button></div>
      <div className="welcome-oby"><Oby /></div>
      <div key={at} className="welcome-text">
        <h1 className="welcome-title">{SCENES[at].title}</h1>
        <p className="welcome-line">{SCENES[at].line}</p>
        <div className="welcome-scene"><Scene /></div>
      </div>
      <div className="welcome-foot">
        <span className="welcome-dots">{SCENES.map((s, i) => <span key={s.title} className={i === at ? 'is-on' : ''} />)}</span>
        <span className="welcome-actions">
          {at > 0 && <button className="set-button" onClick={back}>Back</button>}
          <button className="modal-primary" onClick={next}>{at === last ? 'Start' : 'Continue'}</button>
        </span>
      </div>
    </div>
  );
}
