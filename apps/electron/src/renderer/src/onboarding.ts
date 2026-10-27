import { useSyncExternalStore } from 'react';
import { openChat, sayOby, sayYours } from './chats';
import { addTask } from './tasks';
import { wait } from './stream';
import { onObid } from './lean';
import { CAN_CREATE } from './spheres';
import { api } from './obid';
import { ask, liveState, say, useLive } from './live';
import type { Agent, SetupStep } from './mock';

// Setting up, in Oby's first conversation: no wizard, Oby talks you through it
// (as Grok Bot and OpenMausBot do). Oby says what it found and what's next, a
// card under its words is where you act, and a pick carries on, going in as
// your words. The tools you already use (each keeps its own login), GitHub,
// then Tetrix (optional), then the first task, and while it runs a tour of a
// sample Obysphere. 2 or 3 minutes. On obid (lean.ts) every sign-in is the
// tool's own, through obid (docs/runner-api.md: accounts, GitHub's sign-in,
// Tetrix), and the first task goes to Oby; Oby's lines stay these, since no
// brain runs before an agent is signed in. The Obysphere stays the sample tour
// (before the first task there). Elsewhere it's all mocked.

export const WELCOME = 'welcome';
type Sign = 'out' | 'waiting' | 'in';
// what a sign-in under way needs from you: the code to type on its page (Codex's, GitHub's), or
// the one its page shows, to paste back (Claude Code's: account, the account it signs in)
export type Code = { userCode?: string | null; paste?: string };
// where the Obysphere tour is: explaining, showing how, the sample out (calling you, then you
// inside it), the sample on its way out, then gone (or skipped)
type Tour = 'off' | 'explain' | 'showme' | 'sample' | 'inside' | 'leaving' | 'gone';
export type Setup = {
  found: number;        // how many of the tools on this Mac Oby has looked at so far
  agents: Record<Agent, Sign>;
  github: Sign;
  tetrix: Sign;
  sphere: Tour;
  member?: string;      // in the sample: whose conversation is showing (or Knowledge)
  call?: string;        // in the sample: who's called next (or Knowledge)
  at: SetupStep | null; // the step waiting on your pick
  codes: Partial<Record<Agent | 'github', Code>>;
  githubUser?: string;
};
let setup: Setup = {
  found: 0, agents: onObid ? { claude: 'out', codex: 'out', cursor: 'out' } : { claude: 'in', codex: 'in', cursor: 'out' },
  github: 'out', tetrix: 'out', sphere: 'off', at: null, codes: {},
};
const listeners = new Set<() => void>();
const set = (fn: (s: Setup) => Partial<Setup>) => { setup = { ...setup, ...fn(setup) }; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
// dev only (dev/ControlCenter.tsx): setting up as it stands, changed, and watched
export const dev = import.meta.env.DEV ? { get: () => setup, set, subscribe } : undefined;
// On obid, a tool is in once one of its accounts is signed in, as obid says
const signedIn = (a: Agent) => !!liveState().status?.accounts.some((x) => x.agent === a && x.signedIn);
export function useSetup(): Setup {
  const s = useSyncExternalStore(subscribe, () => setup);
  useLive();
  if (!onObid) return s;
  return { ...s, agents: Object.fromEntries((Object.keys(s.agents) as Agent[]).map((a) => [a, signedIn(a) ? 'in' : s.agents[a]])) as Record<Agent, Sign> };
}
// how a signed-in tool shows: its account
export const accountOf = (a: Agent) => liveState().status?.accounts.find((x) => x.agent === a && x.signedIn)?.name;

// done once: the next launch opens on Home; ?welcome sets up again
const KEY = 'oby.setUp';
export const setUp = () => {
  if (new URLSearchParams(location.search).has('welcome')) return false;
  try { return !!localStorage.getItem(KEY); } catch { return false; }
};

// ponytail: off obid, each stands in for the tool's own sign-in page, finished after a moment
const SIGN_IN_MS = 1600;
async function signIn(get: (s: Setup) => Sign, put: (v: Sign) => Partial<Setup>) {
  if (get(setup) !== 'out') return;
  set(() => put('waiting'));
  await wait(SIGN_IN_MS);
  set(() => put('in'));
}
const agentSign = (a: Agent, v: Sign) => ({ agents: { ...setup.agents, [a]: v } });
const open = (url: string) => window.app?.openExternal(url);
// what went wrong, said, and the row back to its button
const failed = (e: unknown, back: () => void) => { back(); say(e); };

// An agent: its login already on this Mac becomes an account (availableLogins); else an account
// of its own, signed in with the tool's own login on its page. obid's `accounts` event says when
// it's in.
const attempts: Partial<Record<Agent, number>> = {};
export async function signInAgent(a: Agent) {
  if (!onObid) return signIn((s) => s.agents[a], (v) => agentSign(a, v));
  if (setup.agents[a] !== 'out' || signedIn(a)) return;
  const mine = (attempts[a] = (attempts[a] ?? 0) + 1);
  set(() => agentSign(a, 'waiting'));
  try {
    const s = liveState().status;
    if (s?.availableLogins?.some((l) => l.agent === a)) {
      await api.addAccount({ agent: a, default: true });
    } else {
      const account = s?.accounts.find((x) => x.agent === a)?.id ?? (await api.addAccount({ agent: a, name: 'Personal' })).id;
      const page = await api.signIn(account);
      set((x) => ({ codes: { ...x.codes, [a]: { userCode: page.userCode, paste: page.pasteCode ? account : undefined } } }));
      open(page.url);
    }
    // a login that didn't take (its page closed, an expired login adopted): the button again,
    // unless a later try took over
    if (!(await until(async () => signedIn(a) || attempts[a] !== mine))) set((x) => ({ ...agentSign(a, 'out'), codes: { ...x.codes, [a]: undefined } }));
  } catch (e) {
    failed(e, () => set(() => agentSign(a, 'out')));
  }
}
// Claude Code's code, pasted from its page
export async function sendAgentCode(a: Agent, code: string) {
  const account = setup.codes[a]?.paste;
  if (!account || !code.trim()) return;
  try {
    await api.signInCode(account, code.trim());
    set((x) => ({ codes: { ...x.codes, [a]: undefined } }));
  } catch (e) {
    // the sign-in ended: start again for a new link
    failed(e, () => set((x) => ({ ...agentSign(a, 'out'), codes: { ...x.codes, [a]: undefined } })));
  }
}

// asked again every few seconds while its page is open (obid sends no event for these)
async function until(done: () => Promise<boolean>, minutes = 15) {
  for (const end = Date.now() + minutes * 60_000; Date.now() < end; await wait(3000)) if (await done().catch(() => false)) return true;
  return false;
}
// GitHub, with gh's own device sign-in: its code to type on GitHub's page
export async function signInGitHub() {
  if (!onObid) return signIn((s) => s.github, (github) => ({ github }));
  if (setup.github !== 'out') return;
  set(() => ({ github: 'waiting' }));
  try {
    const page = await api.githubSignIn();
    set((x) => ({ codes: { ...x.codes, github: { userCode: page.userCode } } }));
    open(page.url);
    const ok = await until(async () => { const g = await api.github(); if (g.signedIn) set(() => ({ github: 'in', githubUser: g.user ?? undefined })); return g.signedIn; });
    if (!ok) set(() => ({ github: 'out' }));
  } catch (e) {
    failed(e, () => set(() => ({ github: 'out' })));
  }
}
// Tetrix: Oby signs in once, for every agent; its page in the browser
export async function connectTetrix() {
  if (!onObid) return signIn((s) => s.tetrix, (tetrix) => ({ tetrix }));
  if (setup.tetrix !== 'out') return;
  set(() => ({ tetrix: 'waiting' }));
  try {
    const preset = (await api.tetrix()).presets[0];
    if (!preset) throw new Error('Oby has no Tetrix to sign in to.');
    open((await api.tetrixSignIn(preset.id)).url);
    if (!(await until(async () => (await api.tetrix()).connected, 10))) set(() => ({ tetrix: 'out' }));
    else set(() => ({ tetrix: 'in' }));
  } catch (e) {
    failed(e, () => set(() => ({ tetrix: 'out' })));
  }
}

// On obid, what's signed in already shows so at once
async function already() {
  if (!onObid) return;
  const [g, t] = await Promise.all([api.github().catch(() => null), api.tetrix().catch(() => null)]);
  set(() => ({ ...(g?.signedIn ? { github: 'in' as const, githubUser: g.user ?? undefined } : {}), ...(t?.connected ? { tetrix: 'in' as const } : {}) }));
}

// your pick on the step waiting: it goes in as your words, and Oby carries on.
// quiet: the step went on by itself (left alone, or its scene ended), no words of yours
let picked: ((label: string) => void) | null = null;
export function pick(label: string, quiet = false) {
  if (!picked || (aside && !quiet)) return;
  const go = picked;
  picked = null;
  set(() => ({ at: null }));
  if (!quiet) sayYours(WELCOME, label);
  go(label);
}
// idle: the tour's steps don't wait forever; left alone, they go on as this pick.
// A Replay (sceneReplayed) makes the timer stale and starts another
const IDLE_MS = 22_000;
let idleGen = 0;
const idleAfter = (r: (label: string) => void, idle: string) => {
  const g = ++idleGen;
  void wait(IDLE_MS).then(() => { if (g === idleGen && picked === r) pick(idle, true); });
};
const choose = (step: SetupStep, idle?: string) => new Promise<string>((r) => {
  set(() => ({ at: step }));
  picked = r;
  if (idle) idleAfter(r, idle);
});
// typed in the message box: at the last step, your own words are the first task
export const typedFirst = (text: string) => setup.at === 'first' && !!text.trim() && (pick(text.trim()), true);

export const FIRST_TASK = 'Find a small bug in obi and fix it';

let started = false;
// open: show a conversation (Home's), for the first task's own on obid
export async function startOnboarding(show?: (id: string) => void) {
  if (started) return;
  started = true;
  void already();
  openChat(WELCOME, 'Getting started');
  await sayOby(WELCOME, { text: 'Hi, I’m Oby. I hand your coding work to the agents you already use, and I stay on it until it’s done.' });
  await sayOby(WELCOME, { text: 'Here’s what’s on this Mac. Each one keeps its own login; I never see your passwords.', setup: 'agents' });
  for (let i = 1; i <= 3; i++) { await wait(450); set(() => ({ found: i })); }
  await choose('agents');
  await sayOby(WELCOME, { text: 'Now GitHub, so your agents can open pull requests. I sign in with GitHub’s own tool.', setup: 'github' });
  const git = await choose('github');
  await sayOby(WELCOME, {
    text: git === 'Not now' ? 'Fine, I’ll ask again before your first pull request. One more, and it’s optional.' : 'One more, and it’s optional.',
    setup: 'extras',
  });
  await choose('extras');
  // (saved first: leaving mid-tour doesn't set everything up again)
  try { localStorage.setItem(KEY, '1'); } catch { /* set up again next time, that's all */ }
  // on obid the first task opens a conversation of its own, so the tour comes first
  if (onObid) await sphereTour();
  await sayOby(WELCOME, { text: 'You’re set. What should I work on first? Pick one, or tell me in your own words.', setup: 'first' });
  const first = await choose('first');
  // on obid, Oby itself takes it: a conversation of its own, opened
  if (onObid) {
    const id = ask(first);
    if (id) show?.(id);
    return;
  }
  await sayOby(WELCOME, { text: 'On it. Claude Code is looking through obi on Personal. I’ll tell you when there’s something to see.' }, () => {
    const id = addTask(first);
    return id ? [id] : undefined;
  });
  await wait(1500);
  await sphereTour();
}

// The Obysphere tour: Oby says what one is, shows how one is made and joined,
// then lets you look around a sample (SAMPLE, spheres.ts) in the side panel, one
// teammate's example conversation at a time, then Knowledge. The sample is
// constants only: no network, no invite, no store, no other Oby. Each click on it
// goes in as your words, as a pick does, so each of Oby's steps has its own face;
// what to click next is in the cards, never in Oby's words, so nothing it said
// points at the sample once it's put away (the cards become one line then).
const SAMPLE_MEMBERS = ['Ernesto', 'Dima', 'Wellington'] as const;
type Member = (typeof SAMPLE_MEMBERS)[number];
// the teammates whose conversation has played (each plays once: its card stays in the chat)
const seen = new Set<string>();
// how one is made, as the + is shown or not (spheres.ts)
const orMake = CAN_CREATE ? ', or the + on the left makes your own.' : '.';
const SAMPLE_SAYS: Record<Member, string> = {
  Ernesto: 'Everyone here has an Oby, and yours can ask theirs. When GitHub needs another person’s OK on your pull request, your Oby asks a teammate’s, like this:',
  Dima: 'Dima’s Oby asked about your branch. Yours knew, so it answered by itself and told you after.',
  Wellington: 'Wellington’s Oby asked for work on your side, so yours asked you first and did what you said.',
};
// the side panel follows the tour: opened for a while, not remembered (Home.tsx)
const panel = (open: boolean) => dispatchEvent(new CustomEvent('oby:sidebar', { detail: open }));
export const sampleOut = (s: Setup) => s.sphere === 'sample' || s.sphere === 'inside' || s.sphere === 'leaving';

async function sphereTour() {
  set(() => ({ sphere: 'explain' }));
  await sayOby(WELCOME, { text: `${onObid ? 'Your' : 'While Claude Code works: your'} team can have an Obysphere. It’s where your Obys meet and ask each other for reviews and answers, so nobody has to chase anyone.`, setup: 'sphere' });
  if ((await choose('sphere', 'Skip')) === 'Skip') return putAway(`Okay. When your team has one, its invite link brings you in${orMake}`);
  set(() => ({ sphere: 'showme' }));
  await sayOby(WELCOME, { text: CAN_CREATE ? 'You make one with the + on the left, or join your team’s from an invite link. Like this:' : 'Someone on your team makes one, and the rest join from its invite link. Like this:', setup: 'showme' });
  await choose('showme', 'Continue');
  await sayOby(WELCOME, { text: 'Here’s a sample to look around in: Deskree, with four teammates. It’s only an example, and it goes away when we’re done.', setup: 'sample' });
  await wait(300);
  panel(true);
  await wait(520);
  set(() => ({ sphere: 'sample' }));
  if ((await choose('sample', 'Not now')) === 'Not now') return putAway('Okay, I’ve put the sample away.');
  set(() => ({ sphere: 'inside' }));
  // each teammate in turn; a click on another plays theirs, and the rest keep their order
  for (let who: Member | undefined = 'Ernesto'; who;) {
    seen.add(who);
    set(() => ({ member: who, call: undefined }));
    await sayOby(WELCOME, { text: SAMPLE_SAYS[who], setup: `convo:${who}` });
    set(() => ({ call: SAMPLE_MEMBERS.find((m) => !seen.has(m)) ?? 'Knowledge' }));
    const next = await choose(`convo:${who}`, 'Done looking');
    if (next === 'Done looking') return wrapUp();
    who = SAMPLE_MEMBERS.find((m) => next === `${m}’s` && !seen.has(m));
  }
  set(() => ({ member: 'Knowledge', call: undefined }));
  await sayOby(WELCOME, { text: 'Wellington connected Tetrix here once, for everyone. Your Oby asks through his connection and gets only the answer; his sign-in never leaves his Oby.', setup: 'knowledge' });
  await choose('knowledge', 'Continue');
  await wrapUp();
}
async function wrapUp() {
  set(() => ({ member: undefined, call: undefined }));
  await sayOby(WELCOME, { text: `That’s an Obysphere. When your team makes a real one, its invite link brings you in${orMake}` });
  await putAway('That’s it. I’ve put the Deskree sample away. Ask me anything, here or from Home.');
}
// The end of the tour: the sample's tile shrinks away (220ms, app.css), the side
// panel closes if the tour opened it, and each sample card becomes one line.
// ponytail: Skip setup isn't in the app yet; when it is, it ends the tour here too
async function putAway(line: string) {
  const out = sampleOut(setup);
  if (out) {
    set(() => ({ sphere: 'leaving', member: undefined, call: undefined }));
    await wait(220);
  }
  set(() => ({ sphere: 'gone' }));
  if (out) setTimeout(() => panel(false), 300);
  await sayOby(WELCOME, { text: line });
}

// Show me's scene played to its end (or was skipped): the tour goes on by itself a moment later
// (a Replay meanwhile, sceneReplayed, makes its timer stale: the tour waits for the replay's end)
let sceneGen = 0;
export const sceneDone = () => { const g = sceneGen; void wait(1200).then(() => { if (g === sceneGen && setup.at === 'showme') pick('Continue', true); }); };
export const sceneReplayed = () => { sceneGen++; if (picked && setup.at === 'showme') idleAfter(picked, 'Continue'); };

// A click on the sample in the side panel (Sidebar.tsx): the same as its pick in
// the chat. Sangam (no conversation yet) and Invite member (off in a sample) get
// a line, and the tour waits on. aside: that line landing, when clicks wait
let aside = false;
export async function sampleDid(what: string) {
  if (!picked || aside) return;
  const at = setup.at ?? '';
  if (what === 'tile') return void (at === 'sample' && pick('Take a look'));
  if (!at.startsWith('convo:')) return;
  if (what === 'Knowledge') return void pick('Knowledge');
  // one already played (its card is up there): nothing new to show
  if ((SAMPLE_MEMBERS as readonly string[]).includes(what)) return void (!seen.has(what) && pick(`${what}’s`));
  aside = true;
  await sayOby(WELCOME, { text: what === 'invite' ? 'That’s how you’d ask a teammate in. It’s off here, since this one’s a sample.' : `Nothing with ${what}’s Oby yet. Conversations show up under each Oby.` });
  aside = false;
}
