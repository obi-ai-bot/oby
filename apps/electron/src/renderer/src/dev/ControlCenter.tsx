import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import { ObyFace } from '../ObyFace';
import { AGENTS, type Agent, type Ask, type AskKind, type SetupStep, type Stage, type StageId } from '../mock';
import { FIRST_TASK, WELCOME, pick, signInGitHub, dev as setupDev, type Setup } from '../onboarding';
import { addTask, stop, useTasks, dev as tasksDev, type Task } from '../tasks';
import { sayOby, useChats, dev as chatsDev } from '../chats';
import { onObid } from '../lean';
import { clock } from '../stream';

// The control center (dev builds only: main.tsx imports it behind import.meta.env.DEV), as
// Next.js's dev tools: a small Oby in the bottom-right corner, or ⌘⇧., opens it. It drives the
// app's own stores, so it works in the browser preview too. A scenario is an address
// (?welcome&dev=<id>): the page loads on the mock and plays the real script at Instant up to that
// state, so jumping also tests the script. States are written as obid's events would set them.
// On obid it only offers the scenarios: the rest would act on real jobs.

const ob = setupDev!, tk = tasksDev!, ch = chatsDev!;
const store = (k: string, v?: string) => {
  try { if (v === undefined) return sessionStorage.getItem(k); if (v) sessionStorage.setItem(k, v); else sessionStorage.removeItem(k); } catch { /* not kept, that's all */ }
  return null;
};

// --- time: one speed for every wait (stream.ts), and Step and Pause holding each one
const INSTANT = Infinity;
const SPEEDS: [string, number][] = [['Real', 1], ['4×', 4], ['Instant', INSTANT]];
clock.speed = Number(store('oby.dev.speed')) || 1;
type Mode = 'run' | 'step' | 'pause';
let mode: Mode = 'run', version = 0;
const held: { ms: number; go: () => void }[] = [];
const watchers = new Set<() => void>();
const changed = () => { version++; watchers.forEach((w) => w()); };
const useClock = () => useSyncExternalStore((w) => { watchers.add(w); return () => { watchers.delete(w); }; }, () => version);
const setSpeed = (v: number) => { clock.speed = v; store('oby.dev.speed', String(v)); changed(); };
function setMode(m: Mode) {
  mode = m;
  clock.gate = m === 'run' ? undefined : (ms) => new Promise((go) => { held.push({ ms, go }); changed(); });
  if (m === 'run') held.splice(0).forEach((h) => h.go());
  changed();
}
const stepOn = () => { held.shift()?.go(); changed(); };

// --- scenarios
const DEFAULT: Record<SetupStep, string> = { agents: 'Continue', github: 'Continue', extras: 'Skip for now', first: FIRST_TASK };
type Scenario = { id: string; label: string; to?: SetupStep | 'task'; go?: Partial<Record<SetupStep, string>>; then?: () => void; href?: string };
const first = () => tk.get().at(-1);
const SCENARIOS: Scenario[] = [
  { id: 'fresh-install', label: 'Fresh install' },
  { id: 'agents', label: 'Agents', to: 'agents' },
  { id: 'agents-none', label: 'No agent signed in', to: 'agents', then: () => (['claude', 'codex', 'cursor'] as Agent[]).forEach((a, i) => setAgent(a, i, 'out')) },
  { id: 'agents-signing-in', label: 'Agents signing in', to: 'agents', then: () => { setAgent('claude', 0, 'code'); setAgent('cursor', 2, 'waiting'); } },
  { id: 'github', label: 'GitHub', to: 'github' },
  { id: 'github-code', label: 'GitHub’s code', to: 'github', then: () => setGitHub('code') },
  { id: 'github-skipped', label: 'GitHub skipped', to: 'extras', go: { github: 'Not now' } },
  { id: 'extras', label: 'Tetrix and Obysphere', to: 'extras' },
  { id: 'extras-connected', label: 'Both connected', to: 'extras', then: () => ob.set(() => ({ tetrix: 'in', sphere: 'in' })) },
  { id: 'first-task', label: 'First task', to: 'first' },
  { id: 'task-running', label: 'Task running', to: 'task', then: () => setStage(first()!, 1) },
  { id: 'every-decision', label: 'Every decision', to: 'task', then: () => KINDS.forEach((k) => addDecision(k, 1)) },
  { id: 'task-failed-scan', label: 'Secret scan failed', to: 'task', then: () => decide(first()!, 'failed', 3) },
  { id: 'out-of-usage', label: 'Out of usage', to: 'task', then: () => decide(first()!, 'limit', 1) },
  { id: 'merge-waiting', label: 'Merge waiting', to: 'task', then: () => decide(first()!, 'merge', 4) },
  { id: 'remember', label: 'Remember?', to: 'task', then: () => decide(first()!, 'remember', 5) },
  { id: 'task-done', label: 'Task done', to: 'task', then: () => finish(first()!) },
  { id: 'everything', label: 'Everything', href: '?everything' },
];
const hrefOf = (s: Scenario) => s.href ?? `?welcome&dev=${s.id}`;

// the next step waiting on a pick (onboarding's choose sets it, then takes the pick: so a beat later)
const nextStep = () => new Promise<SetupStep>((r) => {
  const look = () => { const at = ob.get().at; if (at) { off(); queueMicrotask(() => r(at)); } };
  const off = ob.subscribe(look);
  look();
});
// Instant while it plays, not kept: left mid-way, the next page plays at the speed picked. A
// script that no longer gets there gives up after 10s, left where it stopped.
const GIVE_UP_MS = 10_000;
async function play(s: Scenario) {
  clock.speed = INSTANT;
  const end = Date.now() + GIVE_UP_MS, late = () => Date.now() > end;
  const timeout = new Promise<null>((r) => setTimeout(() => r(null), GIVE_UP_MS));
  for (;;) {
    const at = await Promise.race([nextStep(), timeout]);
    if (!at || at === s.to) break;
    const label = s.go?.[at] ?? DEFAULT[at];
    if (at === 'github' && label === 'Continue') await signInGitHub();
    pick(label);
    if (at === 'first') break;
  }
  // ponytail: polled, as tasks.ts has no subscribe outside React
  if (s.to === 'task') while (!first() && !late()) await new Promise((r) => setTimeout(r, 20));
  if (late()) console.warn(`Control center: ${s.id} didn't get there in ${GIVE_UP_MS / 1000}s`);
  else s.then?.();
  clock.speed = Number(store('oby.dev.speed')) || 1;
  changed();
}
const playing = SCENARIOS.find((s) => s.id === new URLSearchParams(location.search).get('dev'));
if (playing?.to) void play(playing);

// --- connections, as obid's events would set them
type AgentState = 'looking' | 'out' | 'waiting' | 'code' | 'in';
const agentState = (s: Setup, a: Agent, i: number): AgentState => (s.found <= i ? 'looking' : s.agents[a] === 'waiting' && s.codes[a] ? 'code' : s.agents[a]);
const setAgent = (a: Agent, i: number, v: AgentState) => ob.set((s) => ({
  found: v === 'looking' ? i : Math.max(s.found, i + 1),
  agents: { ...s.agents, [a]: v === 'looking' ? s.agents[a] : v === 'code' ? 'waiting' : v },
  // Claude Code's page shows a code to paste back; Codex's and Cursor's want one typed
  codes: { ...s.codes, [a]: v === 'code' ? (a === 'claude' ? { paste: 'personal' } : { userCode: 'WDJB-MJHT' }) : undefined },
}));
const githubState = (s: Setup) => (s.github === 'waiting' && s.codes.github ? 'code' : s.github);
const setGitHub = (v: string) => ob.set((s) => ({
  github: v === 'code' ? 'waiting' : (v as Setup['github']),
  codes: { ...s.codes, github: v === 'code' ? { userCode: '8F3A-21C9' } : undefined },
  githubUser: v === 'in' ? 'well-prado' : undefined,
}));

// --- the task: its stages, finished, failed, stopped
const STAGES: [StageId, string, string][] = [
  ['prepare', 'Prepare', 'Getting ready'], ['work', 'Work', 'Reading the jobs list'], ['check', 'Check', 'Running scripts/oby-check.sh'],
  ['publish', 'Publish', 'Scanning for secrets'], ['review', 'Review', 'Reviewing its own pull request'], ['merged', 'Merged', 'Merged into dev'],
];
const stagesAt = (at: number, state: Stage['state'] = 'current'): Stage[] =>
  STAGES.map(([id, title], i) => ({ id, title, state: i < at ? 'done' : i === at ? state : 'upcoming' }));
const stageOf = (t: Task) => Math.max(0, STAGES.findIndex(([id]) => t.job.stages.find((s) => s.state === 'current' || s.state === 'failed')?.id === id));
function setStage(t: Task, at: number) {
  const merged = at === STAGES.length - 1;
  tk.patch(t.id, (k) => ({
    status: merged ? 'done' : k.job.ask && !k.answer ? 'needs' : 'running',
    job: { ...k.job, status: merged ? 'done' : 'running', step: STAGES[at][2], stages: stagesAt(at, merged ? 'done' : 'current'), ...(merged ? { finished: 0 } : {}) },
  }));
}
// the honest end (AGENTS.md): the pull request is open, waiting for a review
const finish = (t: Task) => tk.patch(t.id, (k) => ({
  status: 'done', answer: undefined,
  job: { ...k.job, status: 'done', ask: undefined, step: 'Pull request open · waiting for your review on GitHub', stages: stagesAt(4), branch: 'oby/fix-jobs-list', commits: 1, published: 'obi-ai-bot/obi#1502', summary: 'The jobs list skipped its last row; it now counts from zero. One test added.', finished: 0 },
}));
const fail = (t: Task) => tk.patch(t.id, (k) => ({
  status: 'done',
  job: { ...k.job, status: 'failed', ask: undefined, step: 'Failed: the check still fails after 3 rounds', stages: stagesAt(stageOf(k), 'failed'), finished: 0 },
}));

// --- decisions: each kind as the first task would ask it
const KINDS: AskKind[] = ['question', 'permission', 'review', 'failed', 'limit', 'start', 'merge', 'back', 'check', 'remember'];
const ASKS: Record<AskKind, Ask> = {
  question: { kind: 'question', headline: 'Which bug first?', text: 'Claude Code found two small ones in obi: a typo in a Settings label and an off-by-one in the jobs list.', options: ['The off-by-one', 'The typo'] },
  permission: { kind: 'permission', headline: 'Let Claude Code fetch one package from npm?', text: 'It wants vitest for a test. The job has no network, so this one install needs your OK; nothing else goes out.', options: ['Allow once', 'Deny'], detail: 'npm install -D vitest' },
  review: { kind: 'review', headline: 'My review still finds one thing after 2 rounds', text: 'The fix works, but its new test sleeps for a second, and Claude Code couldn’t take the sleep out.', options: ['One more round', 'Open it as it is'] },
  failed: { kind: 'failed', headline: 'The secret scan stopped the push', text: 'It found an API key in .env.example, so nothing was pushed.', options: ['Take it out and retry', 'Stop the task'] },
  limit: { kind: 'limit', headline: 'Your Claude is full until 4 PM', text: 'Personal’s 5-hour window is used up, and no other Claude account has room.', options: ['Wait until 4 PM', 'Move it to Codex'] },
  start: { kind: 'start', headline: 'Start it now?', text: 'A standing rule wants to update obi’s dependencies. It runs in Sandbox mode and publishes by itself.', options: ['Start it', 'Not this time'] },
  merge: { kind: 'merge', headline: 'Merge obi #1502 into dev?', text: 'Checks passed on 4c1e9a0 and my review found nothing to change. I haven’t learned how obi merges yet, so this first one is yours.', options: ['Squash and merge', 'Leave it open'] },
  back: { kind: 'back', headline: 'That Mac couldn’t keep running it', text: 'It ran there for 18 minutes and came back with its notes and 2 commits.', options: ['Run it here', 'Wait for that Mac'] },
  check: { kind: 'check', headline: 'Which check proves it done?', text: 'obi has no check set yet. Pick one, or tell me yours.', options: ['scripts/oby-check.sh', 'npm run typecheck'] },
  remember: { kind: 'remember', headline: 'Can I remember that?', text: 'You squash-merged into dev. Next time in obi I’ll do the same, and say so.', options: ['Yes, remember', 'No'] },
};
const decide = (t: Task, k: AskKind, at = stageOf(t)) => tk.patch(t.id, (x) => ({
  status: 'needs', answer: undefined, job: { ...x.job, status: 'running', ask: ASKS[k], stages: stagesAt(at), step: STAGES[at][2] },
}));
// n new tasks waiting on that kind, on Home and as cards in this conversation
const talkId = () => (ch.get().find((c) => c.id === WELCOME) ?? ch.get()[0])?.id;
function addDecision(k: AskKind, n: number) {
  const ids = Array.from({ length: n }, (_, i) => addTask(`${ASKS[k].headline}${n > 1 ? ` (${i + 1})` : ''}`)!);
  ids.forEach((id) => decide(tk.get().find((t) => t.id === id)!, k, 1));
  const c = talkId();
  if (c) ch.patch(c, (x) => ({ said: [...x.said, { who: 'oby', text: n > 1 ? `${n} tasks need you.` : 'This one needs you.', ago: 0, jobs: ids }] }));
}

// --- Oby's face. ponytail: looking and the cheer come when the face has them
const SAMPLE = 'You’re set. What should I work on first? Pick one, or tell me in your own words.';

// --- the panel
function Seg<T extends string | number>({ options, on, set, label }: { options: [string, T][]; on?: T; set: (v: T) => void; label: string }) {
  return (
    <span className="cc-seg" role="group" aria-label={label}>
      {options.map(([text, v]) => <button key={text} type="button" className={v === on ? 'is-on' : ''} aria-pressed={v === on} onClick={() => set(v)}>{text}</button>)}
    </span>
  );
}
const pills = <T extends string>(...v: T[]): [string, T][] => v.map((x) => [x, x]);

function ControlCenter() {
  const [open, setOpen] = useState(false);
  const [hidden, setHidden] = useState(() => store('oby.dev.hidden') === '1');
  const [count, setCount] = useState(1);
  const s = useSyncExternalStore(ob.subscribe, ob.get);
  const tasks = useTasks(), chats = useChats();
  useClock();
  const badge = useRef<HTMLButtonElement>(null);
  const task = tasks.at(-1);
  const talk = chats.find((c) => c.id === WELCOME) ?? chats[0];

  // ⌘⇧. toggles (and brings it back if hidden); N or → goes on in Step
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.shiftKey && (e.code === 'Period' || e.key === '.' || e.key === '>')) {
        e.preventDefault(); e.stopPropagation();
        store('oby.dev.hidden', ''); setHidden(false); setOpen((o) => !o);
      } else if (mode === 'step' && (e.key === 'n' || e.key === 'ArrowRight') && !(e.target as HTMLElement).closest?.('input, textarea, [contenteditable="true"]')) {
        e.preventDefault(); stepOn();
      }
    };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);

  if (hidden) return null;
  const close = () => { setOpen(false); badge.current?.focus(); };
  const reset = () => {
    try {
      Object.keys(localStorage).filter((k) => k.startsWith('oby.')).forEach((k) => localStorage.removeItem(k));
      Object.keys(sessionStorage).filter((k) => k.startsWith('oby.dev.')).forEach((k) => sessionStorage.removeItem(k));
    } catch { /* nothing kept */ }
    // on the mock, as every scenario: a reset never sets up again on the real obid
    location.href = '?welcome&dev=fresh-install';
  };
  const agents = Object.keys(AGENTS) as Agent[];
  return <>
    <button ref={badge} type="button" className="cc-badge" aria-label="Open the control center" aria-expanded={open} onClick={() => setOpen(!open)}>
      <ObyFace size={14} />
    </button>
    {open && (
      <div className="cc" role="dialog" aria-label="Control center"
        onKeyDown={(e) => { if (e.key === 'Escape') { e.nativeEvent.stopImmediatePropagation(); e.stopPropagation(); close(); } }}>
        <header className="cc-head">
          <b>Control center</b><span className="cc-dev">dev</span>
          <button type="button" className="cc-icon" onClick={reset} title="Clear every oby.* key and set up again">Reset</button>
          <button type="button" className="cc-icon" onClick={close} aria-label="Close the control center">×</button>
        </header>
        <p className="cc-read">
          {onObid ? 'live obid · a scenario opens on the mock' : <>
            at: {s.at ?? (task ? `task · ${task.job.step}` : '—')}{playing ? ` · scenario: ${playing.id}` : ''}
            {mode !== 'run' && ` · ${held.length ? `waiting ${held[0].ms}ms${held.length > 1 ? ` (+${held.length - 1})` : ''}` : 'nothing waiting'}`}
          </>}
        </p>

        <section className="cc-grp" aria-label="Scenarios">
          <h6>Scenarios</h6>
          <span className="cc-seg">
            {SCENARIOS.map((x) => <a key={x.id} href={hrefOf(x)} className={x === playing ? 'is-on' : ''} title={hrefOf(x)}>{x.label}</a>)}
          </span>
        </section>

        {/* on obid these would act on real jobs: there, only the scenarios (on the mock) */}
        {!onObid && <>
          <section className="cc-grp" aria-label="Time">
            <h6>Time</h6>
            <span className="cc-row">
              <Seg label="Speed" options={SPEEDS} on={clock.speed} set={setSpeed} />
              <Seg label="Hold" options={[['Step', 'step'], ['Pause', 'pause']]} on={mode === 'run' ? undefined : mode} set={(m) => setMode(mode === m ? 'run' : m)} />
              {mode !== 'run' && <button type="button" className="cc-go" disabled={!held.length} onClick={mode === 'step' ? stepOn : () => setMode('run')}>{mode === 'step' ? 'Next' : 'Go on'}</button>}
            </span>
          </section>

          <section className="cc-grp" aria-label="Connections">
            <h6>Connections</h6>
            <div className="cc-kv">
              {agents.map((a, i) => [
                <span key={`${a}l`}>{AGENTS[a].name}</span>,
                <Seg key={a} label={AGENTS[a].name} options={pills('looking', 'out', 'waiting', 'code', 'in')} on={agentState(s, a, i)} set={(v) => setAgent(a, i, v)} />,
              ])}
              <span>GitHub</span><Seg label="GitHub" options={pills('out', 'waiting', 'code', 'in')} on={githubState(s)} set={setGitHub} />
              <span>Tetrix</span><Seg label="Tetrix" options={pills('out', 'waiting', 'in')} on={s.tetrix} set={(tetrix) => ob.set(() => ({ tetrix }))} />
              <span>Obysphere</span><Seg label="Obysphere" options={pills('out', 'waiting', 'in')} on={s.sphere} set={(sphere) => ob.set(() => ({ sphere }))} />
            </div>
          </section>

          <section className="cc-grp" aria-label="Task">
            <h6>Task{task && <span className="cc-sub"> · {task.title}</span>}</h6>
            {task ? <>
              <Seg label="Stage" options={STAGES.map(([, title], i) => [title, i])} on={stageOf(task)} set={(i) => setStage(task, i)} />
              <span className="cc-row">
                <button type="button" className="cc-go" onClick={() => finish(task)}>Finish</button>
                <button type="button" className="cc-go" onClick={() => fail(task)}>Fail</button>
                <button type="button" className="cc-go" onClick={() => stop(task.id)}>Stop</button>
              </span>
            </> : <button type="button" className="cc-go" onClick={() => addTask(FIRST_TASK)}>Start one</button>}
          </section>

          <section className="cc-grp" aria-label="Add decision">
            <h6>Add decision</h6>
            <Seg label="How many" options={[['1', 1], ['2', 2], ['3', 3]]} on={count} set={setCount} />
            <span className="cc-seg">{KINDS.map((k) => <button key={k} type="button" onClick={() => addDecision(k, count)}>{k}</button>)}</span>
          </section>

          <section className="cc-grp" aria-label="Oby">
            <h6>Oby</h6>
            <span className="cc-seg">
              <button type="button" className={talk?.typing ? 'is-on' : ''} disabled={!talk} onClick={() => ch.patch(talk!.id, (c) => ({ typing: !c.typing, step: c.typing ? undefined : 'Thinking' }))}>thinking</button>
              <button type="button" disabled={!talk} onClick={() => void sayOby(talk!.id, { text: SAMPLE })}>talking</button>
            </span>
          </section>
        </>}

        <footer className="cc-foot">
          ⌘⇧. toggles · Esc closes · <button type="button" onClick={() => { store('oby.dev.hidden', '1'); setHidden(true); }}>Hide for this session</button>
        </footer>
      </div>
    )}
  </>;
}

const CSS = `
.cc-badge { position: fixed; right: 12px; bottom: 12px; z-index: 1000; width: 28px; height: 28px; padding: 0; border-radius: 50%; display: grid; place-items: center; background: var(--surface); border: 1px solid var(--surface-edge); box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4); cursor: pointer; -webkit-app-region: no-drag; }
.cc-badge:focus-visible, .cc button:focus-visible, .cc a:focus-visible { outline: 2px solid var(--unread); outline-offset: 1px; }
.cc { position: fixed; right: 12px; bottom: 48px; z-index: 1000; width: min(340px, calc(100vw - 24px)); max-height: min(600px, calc(100vh - 96px)); overflow: auto; background: var(--surface); border: 1px solid var(--surface-edge); border-radius: 12px; box-shadow: 0 18px 40px rgba(0, 0, 0, 0.55); color: var(--ink); font: var(--text-sm)/1.4 var(--font); animation: cc-in 160ms var(--warm) both; -webkit-app-region: no-drag; }
@keyframes cc-in { from { transform: translateY(6px) scale(0.98); opacity: 0; } }
.cc-head { position: sticky; top: 0; z-index: 1; display: flex; align-items: center; gap: 8px; padding: 10px 12px; background: var(--surface); border-bottom: 1px solid var(--surface-edge); }
.cc-head b { font-size: var(--text-md); font-weight: 600; }
.cc-dev { font-family: ui-monospace, monospace; font-size: 10px; color: var(--muted); }
.cc-icon { background: none; border: 0; color: var(--muted); font: inherit; cursor: pointer; padding: 0 2px; }
.cc-icon:first-of-type { margin-left: auto; }
.cc-icon:hover { color: var(--ink); }
.cc-read { margin: 0; padding: 8px 12px; color: var(--muted); font: 10.5px/1.4 ui-monospace, monospace; border-bottom: 1px solid var(--surface-edge); overflow-wrap: anywhere; }
.cc-grp { display: flex; flex-direction: column; gap: 6px; padding: 8px 12px; border-bottom: 1px solid var(--surface-edge); }
.cc-grp h6 { margin: 0; font-size: 10.5px; font-weight: 500; color: var(--label); text-transform: uppercase; letter-spacing: 0.05em; }
.cc-sub { text-transform: none; letter-spacing: 0; }
.cc-row, .cc-seg { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; }
.cc-row { gap: 8px; }
.cc-seg button, .cc-seg a, .cc-go { font: inherit; font-size: 11px; line-height: 22px; height: 24px; padding: 0 7px; border-radius: 12px; border: 1px solid var(--line); background: none; color: var(--text); text-decoration: none; cursor: pointer; }
.cc-seg button:hover, .cc-seg a:hover, .cc-go:hover { border-color: var(--line-strong); color: var(--ink); }
.cc-seg .is-on { background: var(--ink); border-color: var(--ink); color: #111; }
.cc button[disabled] { opacity: 0.45; cursor: default; }
.cc-kv { display: grid; grid-template-columns: 68px 1fr; gap: 4px 8px; align-items: center; }
.cc-kv > span:nth-child(odd) { color: var(--muted); }
.cc-foot { padding: 8px 12px; color: var(--muted); font-size: 10.5px; }
.cc-foot button { background: none; border: 0; padding: 0; font: inherit; color: var(--text); text-decoration: underline; cursor: pointer; }
`;

const style = document.createElement('style');
style.textContent = CSS;
document.head.append(style);
const root = document.createElement('div');
document.body.append(root);
createRoot(root).render(<ControlCenter />);
