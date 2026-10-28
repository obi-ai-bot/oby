import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { ArrowRight } from 'lucide-react';
import Oby from './Oby';
import { AgentRows, GitHubRow, TetrixRow } from './SetupCard';
import { findAgents, useSetup } from './onboarding';
import { useLive } from './live';
import { resetSettingsTip } from './SettingsTip';

// Setting up, Sangam's way (2026-10-27): a window of its own, not the chat. Oby
// hops up out of the bottom edge (as the Obys of a conversation do), half of it
// out of sight, as it greets you; then what it is and a button rise in under the
// greeting, a beat apart. Oby alone: no effects round it but the fly. Each step
// after is the same: Oby's line at the top, what to do in the middle, the
// button on to the next. Next: Oby jumps up to its place on Home as the window
// grows to its usual size. Wellington's setup in Oby's first conversation
// (onboarding.ts) stays as it is for now; this one opens from the control
// center (dev/ControlCenter.tsx), and on every launch until it's done (below).
export type Step = 'hello' | 'tools' | 'knowledge' | 'sphere';
const STEPS: Step[] = ['hello', 'tools', 'knowledge', 'sphere'];

// it opens on every launch until it's been gone through to the end (finishOnboarding);
// ?onboarding opens it anyway (=tools at that step), and the control center's Reset
// (which clears every oby.* key) sets it up again
const DONE = 'oby.onboarded';
// (while it's being designed, Sangam's call of 2026-10-27, until he says otherwise: a dev
// build opens on it every time, done or not; a release build, once)
const ALWAYS = import.meta.env.DEV;
const done = () => { if (ALWAYS) return false; try { return !!localStorage.getItem(DONE); } catch { return false; } };
const fromUrl = new URLSearchParams(location.search).get('onboarding');
let step: Step | null = fromUrl !== null ? (STEPS.includes(fromUrl as Step) ? (fromUrl as Step) : 'hello') : done() ? null : 'hello';
const listeners = new Set<() => void>();
const put = (s: Step | null) => { step = s; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const openOnboarding = (s: Step = 'hello') => put(s);
// (closed from the control center: as good as done for the first conversation's setting up too,
// so it doesn't take over Home behind it)
export const closeOnboarding = () => { try { localStorage.setItem('oby.setUp', '1'); } catch { /* fine */ } handover = null; put(null); };
// The last step's button: set up (the next launch opens on the app, and the first conversation's
// setting up, onboarding.ts, has nothing left to do), then the hand over to Home: the others
// leave and the words go ('leaving'); then the window grows to its own size with Home there
// underneath, and Oby jumps into its place on it ('home'); then this is gone
export type Handover = null | 'leaving' | 'home';
let handover: Handover = null;
export const setHandover = (h: Handover) => { handover = h; listeners.forEach((l) => l()); };
export const useHandover = () => useSyncExternalStore(subscribe, () => handover);
export const finishOnboarding = () => {
  try {
    localStorage.setItem(DONE, '1'); localStorage.setItem('oby.setUp', '1');
    localStorage.setItem('oby.sidebarOpen', '0');   // Home, the first time: just Oby, the side panel closed
    localStorage.removeItem('oby.sidebarWidth');    // and opened, at its default width (362px)
  } catch { /* it opens again next launch, that's all */ }
  resetSettingsTip();   // and the tip over the message box, pointing at Settings
  setHandover('leaving');
};
// The window: setting up's narrow one, or Home's (main.tsx). Where it's growing to, if it is,
// so Home's Oby can come up the moment it's there
let windowGoal: number | null = null;
export const sizeWindow = (setup: boolean) => { windowGoal = window.app?.setupWindow?.(setup) ?? null; };
// Home's Oby, coming in from setting up: it waits under the floor of its frame (arriving) till
// setting up has faded from over it, then hops up into its place (Home.tsx)
let arriving = false;
export const arrivingNow = () => handover === 'home' || arriving;
export const useArriving = () => useSyncExternalStore(subscribe, () => arriving);
const setArriving = (a: boolean) => { arriving = a; listeners.forEach((l) => l()); };
export const useOnboarding = () => useSyncExternalStore(subscribe, () => step);

// the button's arrow, as heavy as its label: 17px at 500 draws 1.77px stems; lucide's
// 24-unit stroke at 16px is 16/24 of it, so 2.65 draws 1.77px
const ARROW_STROKE = 2.65;
// on a step where you act, Oby hops down this far (in bodies) and peeks over the bottom
// edge (its ears and eyes), out of the way of what you're doing; back on
// the greeting, it hops up again
const DUCK = 0.3;
// its hops slower than the small Obys' (52), the same arc: up in 0.52s, down in 0.25s; and Home's
// Oby hopping up into its place after (Home.tsx), so it lands as this one did
const GRAVITY = 30;
export const ARRIVAL_GRAVITY = GRAVITY;
// on the greeting it stands a little lower than its full height: 10% less of it showing
// (about 188px over the edge at full height; 0.07 of a body, about 19px, lower)
const HELLO = 0.07;
// and while it nods or shakes its head there, it rises smoothly to show more of itself (this far
// down), then eases back
const PEEK = 0.12, RISE_MS = 250;
// the moves that come with that rise, and how long each is (oby.html: Agree 2.4s, Nope 1.6s)
const LIFTED: Record<string, number> = { Agree: 2400, Nope: 1600 };
// the Obysphere step: all three smaller (so they fit side by side), the other two starting just
// past the window's edges (bodies from the middle; about 1.5 shows each side). Shoving, they're
// TOUCH apart; settled, SPACE, close (two turned to each other at once can just brush)
const GROUP = 0.64, OFF = 2.3, TOUCH = 0.69, SPACE = 0.75;
// and a little lower than the greeting's Oby, so their bottoms are cut by the window's edge
const LOW = 0.14;
// the way out of a step before the next comes in (app.css, .onb-copy.is-leaving)
const LEAVE_MS = 220;

// what Oby is told to play (Oby.tsx's cue), from anywhere on these screens
let cueN = 0, nodAt = -Infinity;
const cues = new Set<(c: { name: string; n: number }) => void>();
const play = (name: string) => cues.forEach((f) => f({ name, n: ++cueN }));
// the pointer resting on a button and no click: Oby watches it, glances back at you (its own
// rhythm with a still pointer: at 1.2s, for 1 to 1.8s), looks at it again, and only then nods
// you on. Off the button first, or clicked, no nod; not again while it's still nodding
const NOD_AFTER = 3200, NOD_MS = 2400;
let nodTimer = 0;
const nod = () => {
  clearTimeout(nodTimer);
  nodTimer = window.setTimeout(() => { if (performance.now() - nodAt > NOD_MS) { nodAt = performance.now(); play('Agree'); } }, NOD_AFTER);
};
const noNod = () => clearTimeout(nodTimer);

export default function FirstRun() {
  const at = useOnboarding() ?? 'hello';
  const [cue, setCue] = useState<{ name: string; n: number }>();
  // ducked, a nod or a shake of the head comes with a rise, so its eyes are in sight while it
  // says so: up, the move a beat later, back down once it's done (off again when the step changes)
  const [lift, setLift] = useState<'off' | 'up' | 'down'>('off');
  const atRef = useRef(at);
  atRef.current = at;
  useEffect(() => {
    let ts: number[] = [];
    const f = (c: { name: string; n: number }) => {
      const ms = LIFTED[c.name];
      if (!ms || atRef.current === 'hello') { setCue(c); return; }
      ts.forEach(clearTimeout);
      setLift('up');
      ts = [window.setTimeout(() => setCue(c), RISE_MS), window.setTimeout(() => setLift('down'), RISE_MS + ms)];
    };
    cues.add(f);
    return () => { cues.delete(f); ts.forEach(clearTimeout); };
  }, []);
  useEffect(() => setLift('off'), [at]);
  const handover = useHandover();
  // (handing over, the scene stops: what's left is leaving)
  const scene = useScene(at === 'sphere' && !handover);
  // Leaving: all of them hop away, down out of the bottom of the window (a little hop, and down,
  // as they duck), a beat apart, and the words fade; then Home (main.tsx grows the window and puts
  // Home underneath)
  useEffect(() => {
    if (handover !== 'leaving') return;
    const ts: number[] = [];
    (at === 'sphere' ? ['main', 'right', 'left'] as const : ['main'] as const).forEach((w, i) =>
      ts.push(window.setTimeout(() => scene.send(w, { duck: 3, lookAt: null }), i * 90)));
    ts.push(window.setTimeout(() => setHandover('home'), 750));
    return () => ts.forEach(clearTimeout);
  }, [handover]);
  // Home: once the window's done growing and Home's Oby is drawn (waiting under the floor of its
  // frame), this screen's dark fades from over Home; then Home's Oby hops up into its place, and
  // this is gone
  useEffect(() => {
    if (handover !== 'home') return;
    setArriving(true);
    let dead = false;
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    (async () => {
      // the window there (checked every frame; 1.5s at most, should it never quite get there)
      for (const t0 = performance.now(); windowGoal != null && outerWidth !== windowGoal && performance.now() - t0 < 1500;) {
        await new Promise(requestAnimationFrame);
      }
      // and Home's Oby drawn (every frame too; 3s at most)
      for (const t0 = performance.now(); performance.now() - t0 < 3000; await new Promise(requestAnimationFrame)) {
        const h = document.querySelector<HTMLIFrameElement>('.stage .oby-frame');
        if (h && h.style.visibility !== 'hidden') break;
      }
      if (dead) return;
      setArriving(false);
      await sleep(100);
      if (dead) return;
      setHandover(null); put(null);
    })();
    // closed meanwhile (the control center's Close): Home mustn't stay waiting under the floor
    return () => { dead = true; setArriving(false); };
  }, [handover]);
  const duck = at === 'hello' ? HELLO : at === 'sphere' ? LOW : lift === 'up' ? { depth: PEEK, soft: true as const } : lift === 'down' ? { depth: DUCK, soft: true as const } : DUCK;
  // Oby comes up once its first frame is drawn (or after a moment, whatever happens)
  const [on, setOn] = useState(false);
  // reduced motion: Oby's just there, no hop
  const [still] = useState(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
  useEffect(() => { const t = setTimeout(() => setOn(true), 1500); return () => clearTimeout(t); }, []);
  // the step showing, which lags `at` while the last one leaves
  const [shown, setShown] = useState(at);
  const [leaving, setLeaving] = useState(false);
  useEffect(() => {
    if (at === shown) return;
    setLeaving(true);
    const t = setTimeout(() => { setShown(at); setLeaving(false); }, LEAVE_MS);
    return () => clearTimeout(t);
  }, [at, shown]);

  return (
    <div className={`onb${on ? ' is-on' : ''}${handover ? ` is-${handover}` : ''}`}>
      <div className="onb-oby"><Oby size={1.4} simple gravity={GRAVITY} enter={still ? undefined : on} duck={duck} cue={cue} scale={at === 'sphere' ? GROUP : 1}
        move={scene.moves.main} onEvent={scene.heard('main')} onDrawn={() => setOn(true)} /></div>
      {/* a click over the Obys: handed to each frame, and the one it's on is poked (laid on one
          another, only the top frame would get it); and the window isn't dragged from here */}
      <div className="onb-poke" onPointerDown={(e) => {
        for (const f of e.currentTarget.parentElement!.querySelectorAll<HTMLIFrameElement>('.onb-oby .oby-frame')) {
          const r = f.getBoundingClientRect();
          f.contentWindow?.postMessage({ pokeAt: { x: e.clientX - r.left, y: e.clientY - r.top } }, '*');
        }
      }} />
      {/* the Obysphere step: two more Obys, each in a frame of its own over this one, come in from the sides */}
      {at === 'sphere' && (['right', 'left'] as const).map((who) => (
        <div key={who} className="onb-oby" aria-hidden="true">
          <Oby size={1.4} simple gravity={GRAVITY} x={who === 'right' ? OFF : -OFF} scale={GROUP} duck={{ depth: LOW, soft: true }}
            move={scene.moves[who]} onEvent={scene.heard(who)} />
        </div>
      ))}
      {on && (
        <div key={shown} className={`onb-copy${shown === 'tools' ? ' is-tall' : ''}${leaving ? ' is-leaving' : ''}`}>
          {shown === 'hello' ? <Hello /> : shown === 'tools' ? <Tools /> : shown === 'knowledge' ? <Knowledge /> : <Sphere />}
        </div>
      )}
    </div>
  );
}

// each piece rises in a beat after the one before: `d` ms after Oby starts up
// soft: slower, and easing in more gently (app.css, .onb-in.is-soft)
const In = ({ d, soft = false, as: Tag = 'div', className = '', children }: { d: number; soft?: boolean; as?: 'div' | 'h1' | 'p' | 'ul'; className?: string; children: React.ReactNode }) => (
  <Tag className={`onb-in${soft ? ' is-soft' : ''} ${className}`} style={{ '--d': `${d}ms` } as React.CSSProperties}>{children}</Tag>
);

function Hello() {
  // the greeting as Oby hops up; what it is and the button once it's landed and
  // settled (it lands at about 0.4s and its squash is still by about 0.9s)
  return <>
    <In d={0} soft as="h1" className="onb-title">Hi, I’m Oby.</In>
    <In d={840} soft as="p" className="onb-text">I’m your engineering delegate.<br />I dispatch tasks to your coding agents and see them through.</In>
    <In d={950} soft className="onb-hit"><button type="button" className="onb-go" onPointerEnter={nod} onPointerLeave={noNod} onClick={() => { noNod(); openOnboarding('tools'); }}>Let’s connect your tools<ArrowRight size={16} strokeWidth={ARROW_STROKE} /></button></In>
  </>;
}

// What Oby needs to start: a coding agent (one at least, to go on) and access to your code
// (GitHub here; GitLab and Bitbucket from Settings' connectors). GitHub can wait: Home asks for it
function Tools() {
  const s = useSetup();
  const { problem, why } = useLive();
  useEffect(() => { void findAgents(); }, []);
  // obid didn't start: its own reason, where the splash would have said it
  const failed = !!problem && why !== 'waiting' && s.found === 0;
  // on once an agent is signed in (and their states are in). Before that, Continue says no
  // without words: the agents ripple top to bottom with a small shake each, and Oby shakes its head
  const ready = s.found >= 3 && Object.values(s.agents).includes('in');
  const [nope, setNope] = useState(0);
  const go = () => { if (ready) { openOnboarding('knowledge'); return; } setNope((n) => n + 1); play('Nope'); };
  return <>
    <In d={0} as="p" className="onb-text">Connect your coding agents and repositories.</In>
    <In d={120} as="ul" className={`setup-rows onb-rows${nope ? ` is-nope-${nope % 2}` : ''}`}><AgentRows bare /></In>
    {/* GitHub a little apart from the agents, no labels: the marks say what each is. Under it, where
        the other places code lives go (not a link: nobody leaves setting up half way) */}
    <In d={180} as="ul" className="setup-rows onb-rows is-apart"><GitHubRow /></In>
    <In d={200} as="p" className="onb-note">Use GitLab or Bitbucket? Add it in Settings after setup.</In>
    {failed && <p className="onb-why" role="status">{problem}</p>}
    <In d={260} className="onb-hit"><button type="button" className="onb-go" onPointerEnter={() => { if (ready) nod(); }} onPointerLeave={noNod} onClick={() => { noNod(); go(); }}>Continue<ArrowRight size={16} strokeWidth={ARROW_STROKE} /></button></In>
  </>;
}

// Knowledge, optional: Tetrix (or any MCP server, from Settings later). Skip is the way past;
// Continue is for once it's connected (it nods you on then): before that it says no as on the
// tools step, the row's small shake and Oby's head
function Knowledge() {
  const s = useSetup();
  const on = s.tetrix === 'in';
  const [nope, setNope] = useState(0);
  const go = () => { if (on) { openOnboarding('sphere'); return; } setNope((n) => n + 1); play('Nope'); };
  return <>
    <In d={0} as="p" className="onb-text">Connect your team’s knowledge system.</In>
    <In d={120} as="ul" className={`setup-rows onb-rows${nope ? ` is-nope-${nope % 2}` : ''}`}><TetrixRow /></In>
    <In d={180} as="p" className="onb-note">Or add any MCP server in Settings after setup.</In>
    <In d={260} className="onb-hit onb-buttons">
      <button type="button" className="onb-go" onPointerEnter={() => { if (on) nod(); }} onPointerLeave={noNod} onClick={() => { noNod(); go(); }}>Continue<ArrowRight size={16} strokeWidth={ARROW_STROKE} /></button>
      {/* under Continue; connected, it fades out but keeps its room, so nothing moves */}
      <button type="button" className={`onb-skip${on ? ' is-gone' : ''}`} tabIndex={on ? -1 : undefined} aria-hidden={on || undefined} onClick={() => { noNod(); openOnboarding('sphere'); }}>Skip for now</button>
    </In>
  </>;
}

// One more thing: Obyspheres. Nothing to do here; the other Obys come in round yours (useScene)
function Sphere() {
  return <>
    <In d={0} soft as="h1" className="onb-title">One more thing.</In>
    <In d={500} soft as="p" className="onb-text">Your Oby can talk to other Obys.<br />Create an Obysphere for your team and bring them together.</In>
    <In d={620} soft className="onb-hit"><button type="button" className="onb-go" onClick={finishOnboarding}>Let’s go<ArrowRight size={16} strokeWidth={ARROW_STROKE} /></button></In>
  </>;
}

// The Obysphere step's little scene, as the Obys of a conversation would play it: the one on the
// right walks in and yours scoots aside to make room; then the one on the left walks in and
// shoves both along, so all three fit; then they talk it over. Each move waits for the last to land
// (what the Obys say)
type Who = 'main' | 'right' | 'left';
type Move = { msg: object; n: number };
function useScene(on: boolean) {
  const [moves, setMoves] = useState<Partial<Record<Who, Move>>>({});
  const n = useRef(0);
  const waits = useRef<{ who: Who; ev: string; done: () => void }[]>([]);
  const heard = (who: Who) => (ev: string) => {
    waits.current = waits.current.filter((w) => (w.who === who && w.ev === ev ? (w.done(), false) : true));
  };
  const send = (who: Who, msg: object) => setMoves((m) => ({ ...m, [who]: { msg, n: ++n.current } }));
  useEffect(() => {
    if (!on) return;
    let dead = false;
    // a move, while the scene's still on: one sent after it stopped would override the leaving step's
    const go = (who: Who, msg: object) => { if (!dead) send(who, msg); };
    // till that one says so (or a while, whatever happens)
    const when = (who: Who, ev: string, max = 4000) => new Promise<void>((done) => {
      waits.current.push({ who, ev, done });
      setTimeout(done, max);
    });
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    const at = { left: -SPACE, main: 0, right: SPACE } as const;
    const all: Who[] = ['left', 'main', 'right'];
    const pick = <T,>(xs: readonly T[]) => xs[Math.floor(Math.random() * xs.length)];
    const IN = 0.55;   // where the two either side step in to (bodies from the middle), yours being further back
    const NEAR = 0.9;   // and yours, back there, a touch smaller still (it's further off in the circle)
    // Talking it over, built up as the group does it: one of the two either side turns to the
    // others; they look at it; then they gather a beat apart, the one going furthest back first
    // (yours steps back, smaller), the one that turned, then the other, both turned well in. Three
    // turns, the one that started first: the one talking leans toward another, mouth going; the
    // rest look at it, nodding (the one it's talking to tilting its head); yours, in the middle,
    // looks from one to the other. Then a giggle, and they break up, the front ones first
    const huddle = async () => {
      const first: Who = pick(['left', 'right'] as const), far: Who = first === 'left' ? 'right' : 'left';
      const pos = { left: -IN, main: 0, right: IN } as const;
      go(first, { face: 0.7, lookAt: 0 }); await sleep(350); if (dead) return;
      go('main', { lookAt: at[first] }); await sleep(250); if (dead) return;
      go(far, { lookAt: at[first] }); await sleep(500); if (dead) return;
      go('main', { back: 0.5, scale: GROUP * NEAR }); await sleep(150 + Math.random() * 100); if (dead) return;
      go(first, { walk: pos[first], side: true }); await sleep(150 + Math.random() * 100); if (dead) return;
      go(far, { walk: pos[far], side: true, face: 0.7 }); await sleep(700); if (dead) return;
      let prev: Who | null = null;
      for (let k = 0; k < 3; k++) {
        const speaker: Who = k === 0 ? first : pick(all.filter((w) => w !== prev));
        const told: Who = prev && Math.random() < 0.6 ? prev : pick(all.filter((w) => w !== speaker));
        const secs = 1.8 + Math.random() * 1.4;
        go(speaker, { talk: { to: pos[told], secs } });
        for (const o of all) if (o !== speaker) go(o, { listen: { to: pos[speaker], secs, told: o === told } });
        // yours, in the middle, looks from one to the other: talking, it turns to the other one half
        // way through; listening, it glances at the other side a moment and back
        const other: Who = speaker === 'main' ? (told === 'left' ? 'right' : 'left') : speaker === 'left' ? 'right' : 'left';
        if (speaker === 'main') {
          await sleep(secs * 500); if (dead) return;
          go('main', { lookAt: pos[other] }); await sleep(secs * 500 + 250); if (dead) return;
        } else {
          await sleep(secs * 550); if (dead) return;
          go('main', { lookAt: pos[other] }); await sleep(600); if (dead) return;
          go('main', { lookAt: pos[speaker] }); await sleep(Math.max(0, secs * 450 - 600) + 250); if (dead) return;
        }
        prev = speaker;
      }
      // the laugh, still facing in, at each other (it was funny in there); breaking up as they
      // laugh, the front ones first, a beat apart, yours last, back to its size; and only then,
      // one by one, they turn back to you
      for (const w of all) { go(w, { play: 'Laugh', lookAt: w === 'main' ? pos[pick(['left', 'right'] as const)] : 0 }); await sleep(140); }
      await sleep(800); if (dead) return;
      for (const w of pick([['left', 'right'], ['right', 'left']] as const)) {
        go(w, { walk: at[w], side: true }); await sleep(150 + Math.random() * 100); if (dead) return;
      }
      go('main', { back: 0, scale: GROUP });
      await sleep(700); if (dead) return;
      for (const w of pick([['left', 'main', 'right'], ['right', 'main', 'left'], ['main', 'left', 'right']] as const)) {
        go(w, { lookAt: null, face: 0.35 }); await sleep(200); if (dead) return;
      }
      await sleep(600);
    };
    (async () => {
      await Promise.all([when('right', 'ready'), when('left', 'ready')]);
      await sleep(700); if (dead) return;
      // in from the right, up to near yours; yours looks over and scoots aside to make room, and it
      // steps in beside it (no joke: just making room)
      // (yours notices it coming, and watches it all the way in)
      go('right', { walk: TOUCH + 0.25 }); await sleep(300); if (dead) return;
      go('main', { lookAt: TOUCH + 0.25 }); await when('right', 'arrived'); if (dead) return;
      await sleep(300); if (dead) return;
      go('main', { walk: -SPACE / 2, side: true }); await when('main', 'arrived'); if (dead) return;
      go('main', { lookAt: null }); go('right', { walk: SPACE / 2 }); await when('right', 'arrived'); if (dead) return;
      // in from the left, up to yours; it shoves the both of them along, and steps into its place
      // (the two notice it coming, and turn to it)
      await sleep(500);
      go('left', { walk: -SPACE / 2 - TOUCH }); await sleep(300); if (dead) return;
      go('main', { lookAt: -SPACE / 2 - TOUCH }); await sleep(150); go('right', { lookAt: -SPACE / 2 - TOUCH });
      await when('left', 'arrived'); if (dead) return;
      await sleep(250); go('left', { shove: 1 });
      await sleep(350); if (dead) return;
      go('main', { shoved: { dir: 1, dist: SPACE / 2 } });
      await sleep(90); if (dead) return;
      go('right', { shoved: { dir: 1, dist: SPACE / 2 } });
      await when('main', 'landed'); if (dead) return;
      go('left', { walk: -SPACE }); await when('left', 'arrived'); if (dead) return;
      // they giggle it off, a beat apart, and come to rest, looking at you; then they talk it over,
      // as the description says they do (huddle, above)
      await sleep(200);
      for (const w of all) { go(w, { play: 'Laugh' }); await sleep(140); }
      await sleep(1000); if (dead) return;
      for (const w of ['main', 'right'] as const) go(w, { lookAt: null });
      await sleep(1500); if (dead) return;
      await huddle(); if (dead) return;
      // then, like birds on a wire, each in its place, they look at one another now and then: one
      // glances at another; one turns to its neighbour, who notices and turns back, and they
      // smile; yours turns to one and then the other, each smiling back, and yours smiles too;
      // two look at the third, who looks back, then smiles at you. A beat of 3.5 to 6s between
      const smile = (w: Who, look?: Who) => go(w, look ? { lookAt: at[look], play: 'Grin' } : { play: 'Grin' });   // (one message: two at once would be one)
      const back = (...ws: Who[]) => ws.forEach((w) => go(w, { lookAt: null }));
      // and once in a while (never first, at most every 30s, 1 in 4 when it may), they talk it over again
      let beat = 0, gossiped = performance.now();   // (they just did, after the shove)
      for (let last = ''; !dead;) {
        await sleep(3500 + Math.random() * 2500); if (dead) return;
        beat++;
        const kind = beat > 1 && performance.now() - gossiped > 30_000 && Math.random() < 0.25
          ? 'gossip' : pick(['glance', 'smile', 'all', 'stare'].filter((k) => k !== last));
        last = kind;
        if (kind === 'gossip') {
          gossiped = performance.now();
          await huddle();
        } else if (kind === 'glance') {
          const a = pick(all), b = pick(all.filter((w) => w !== a && Math.abs(at[w] - at[a]) < SPACE * 1.5));
          go(a, { lookAt: at[b] }); await sleep(1900); back(a);
        } else if (kind === 'smile') {
          const [a, b] = pick([['left', 'main'], ['main', 'left'], ['main', 'right'], ['right', 'main']] as const);
          go(a, { lookAt: at[b] }); await sleep(700); if (dead) return;
          go(b, { lookAt: at[a] }); await sleep(400); if (dead) return;
          smile(a); await sleep(150); smile(b);
          await sleep(1800); back(a, b);
        } else if (kind === 'all') {
          go('main', { lookAt: at.left }); await sleep(600); if (dead) return;
          smile('left', 'main'); await sleep(900); if (dead) return;
          go('main', { lookAt: at.right }); await sleep(600); if (dead) return;
          smile('right', 'main'); await sleep(700); if (dead) return;
          go('main', { lookAt: null, play: 'Grin' }); await sleep(1200);
          back('left', 'right');
        } else {
          const t = pick(all), others = all.filter((w) => w !== t);
          for (const o of others) go(o, { lookAt: at[t] });
          await sleep(800); if (dead) return;
          go(t, { lookAt: at[pick(others)] }); await sleep(1100); if (dead) return;
          go(t, { lookAt: null, play: 'Grin' });
          await sleep(900); back(...others);
        }
      }
    })();
    return () => { dead = true; waits.current = []; };
  }, [on]);
  return { moves, heard, send };
}
