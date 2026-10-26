import { useSyncExternalStore } from 'react';
import { openChat, sayOby, sayYours } from './chats';
import { addTask } from './tasks';
import { wait } from './stream';
import type { Agent, SetupStep } from './mock';

// Setting up, in Oby's first conversation: no wizard, Oby talks you through it
// (as Grok Bot and OpenMausBot do). Oby says what it found and what's next, a
// card under its words is where you act, and a pick carries on, going in as
// your words. The tools you already use (each keeps its own login), GitHub,
// then two optional ones (Tetrix, the Obysphere), then the first task. 2 or 3
// minutes. Mocked: every sign-in here stands in for the tool's own.

export const WELCOME = 'welcome';
type Sign = 'out' | 'waiting' | 'in';
export type Setup = {
  found: number;        // how many of the tools on this Mac Oby has looked at so far
  agents: Record<Agent, Sign>;
  github: Sign;
  tetrix: Sign;
  sphere: Sign;
  at: SetupStep | null; // the step waiting on your pick
};
let setup: Setup = { found: 0, agents: { claude: 'in', codex: 'in', cursor: 'out' }, github: 'out', tetrix: 'out', sphere: 'out', at: null };
const listeners = new Set<() => void>();
const set = (fn: (s: Setup) => Partial<Setup>) => { setup = { ...setup, ...fn(setup) }; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useSetup = () => useSyncExternalStore(subscribe, () => setup);

// done once: the next launch opens on Home; ?welcome sets up again
const KEY = 'oby.setUp';
export const setUp = () => {
  if (new URLSearchParams(location.search).has('welcome')) return false;
  try { return !!localStorage.getItem(KEY); } catch { return false; }
};

// ponytail: each stands in for the tool's own sign-in page, finished after a moment
const SIGN_IN_MS = 1600;
async function signIn(get: (s: Setup) => Sign, put: (v: Sign) => Partial<Setup>) {
  if (get(setup) !== 'out') return;
  set(() => put('waiting'));
  await wait(SIGN_IN_MS);
  set(() => put('in'));
}
export const signInAgent = (a: Agent) => signIn((s) => s.agents[a], (v) => ({ agents: { ...setup.agents, [a]: v } }));
export const signInGitHub = () => signIn((s) => s.github, (github) => ({ github }));
export const connectTetrix = () => signIn((s) => s.tetrix, (tetrix) => ({ tetrix }));
export const joinSphere = () => signIn((s) => s.sphere, (sphere) => ({ sphere }));

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
export async function startOnboarding() {
  if (started) return;
  started = true;
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
  await sayOby(WELCOME, { text: 'On it. Claude Code is looking through obi on Personal. I’ll tell you when there’s something to see.' }, () => {
    const id = addTask(first);
    return id ? [id] : undefined;
  });
}
