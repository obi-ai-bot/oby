import { useSyncExternalStore } from 'react';
import { openChat, sayOby, sayYours } from './chats';
import { addTask } from './tasks';
import { wait } from './stream';
import { onObid } from './lean';
import { api } from './obid';
import { ask, liveState, say, useLive } from './live';
import type { Agent, SetupStep } from './mock';

// Setting up, in Oby's first conversation: no wizard, Oby talks you through it
// (as Grok Bot and OpenMausBot do). Oby says what it found and what's next, a
// card under its words is where you act, and a pick carries on, going in as
// your words. The tools you already use (each keeps its own login), GitHub,
// then two optional ones (Tetrix, the Obysphere), then the first task. 2 or 3
// minutes. On obid (lean.ts) every sign-in is the tool's own, through obid
// (docs/runner-api.md: accounts, GitHub's sign-in, Tetrix), and the first task
// goes to Oby; Oby's lines stay these, since no brain runs before an agent is
// signed in. The Obysphere stays the sample tour. Elsewhere it's all mocked.

export const WELCOME = 'welcome';
type Sign = 'out' | 'waiting' | 'in';
// what a sign-in under way needs from you: the code to type on its page (Codex's, GitHub's), or
// the one its page shows, to paste back (Claude Code's: account, the account it signs in)
export type Code = { userCode?: string | null; paste?: string };
export type Setup = {
  found: number;        // how many of the tools on this Mac Oby has looked at so far
  agents: Record<Agent, Sign>;
  github: Sign;
  tetrix: Sign;
  sphere: Sign;
  at: SetupStep | null; // the step waiting on your pick
  codes: Partial<Record<Agent | 'github', Code>>;
  githubUser?: string;
};
let setup: Setup = {
  found: 0, agents: onObid ? { claude: 'out', codex: 'out', cursor: 'out' } : { claude: 'in', codex: 'in', cursor: 'out' },
  github: 'out', tetrix: 'out', sphere: 'out', at: null, codes: {},
};
const listeners = new Set<() => void>();
const set = (fn: (s: Setup) => Partial<Setup>) => { setup = { ...setup, ...fn(setup) }; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
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
// the sample tour, everywhere
export const joinSphere = () => signIn((s) => s.sphere, (sphere) => ({ sphere }));

// On obid, what's signed in already shows so at once
async function already() {
  if (!onObid) return;
  const [g, t] = await Promise.all([api.github().catch(() => null), api.tetrix().catch(() => null)]);
  set(() => ({ ...(g?.signedIn ? { github: 'in' as const, githubUser: g.user ?? undefined } : {}), ...(t?.connected ? { tetrix: 'in' as const } : {}) }));
}

// your pick on the step waiting: it goes in as your words, and Oby carries on
let picked: ((label: string) => void) | null = null;
export function pick(label: string) {
  if (!picked) return;
  const go = picked;
  picked = null;
  set(() => ({ at: null }));
  sayYours(WELCOME, label);
  go(label);
}
const choose = (step: SetupStep) => new Promise<string>((r) => { set(() => ({ at: step })); picked = r; });
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
    text: git === 'Not now' ? 'Fine, I’ll ask again before your first pull request. Two more, both optional.' : 'Two more, both optional.',
    setup: 'extras',
  });
  await choose('extras');
  try { localStorage.setItem(KEY, '1'); } catch { /* set up again next time, that's all */ }
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
}
