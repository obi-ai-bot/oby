// The design's boards on the demo data (?everything&browser=<n>), as the lab's fixtures had them
// (tools/obid-spec/lab-ui/browser/src/boards.tsx): each board's browser state, the window's own
// state, and the demo page each tab shows. The app.* workflows open a board this way to measure the
// real app against it (tools/obid-spec/workflows/app/browser-*.json). Never shown on obid.
import type { ReactNode } from 'react';
import { BROWSER_DEFAULT_SETTINGS } from '../../../shared/obid-wire';
import type { BrowserState, ScreenHints, Tab } from './view';
import type { Ui } from './store';
import * as Site from './sites';

export type Demo = {
  state: BrowserState;
  ui?: Partial<Ui>;
  peek?: { agent: Tab['owner']; title: string; line: string };
};

const state = (tabs: BrowserState['tabs'], extra: Partial<BrowserState> = {}): BrowserState => ({ tabs, active: tabs[0]?.id ?? null, asks: [], downloads: [], settings: BROWSER_DEFAULT_SETTINGS, ...extra });
const codex = { kind: 'agent', agent: 'codex' } as const;
const claude = { kind: 'agent', agent: 'claude' } as const;
const cursor = { kind: 'agent', agent: 'cursor' } as const;
const t = (id: string, rest: Partial<Tab & ScreenHints> & Pick<Tab, 'title' | 'url'>): Tab & ScreenHints => ({ id, owner: codex, state: 'ready', holder: 'agent', ...rest });

const signIn = t('signin', { title: 'Sign in · Acme', url: 'localhost:3000/login', action: 'Clicking Sign in', pointer: { x: 322, y: 338, ring: true } });
const signUp = t('signup', { title: 'Sign up · Acme', url: 'localhost:3000/signup', action: 'Clicking Sign in' });
const admin = t('admin', { title: 'Sign in · Acme Admin', url: 'staging.acme.dev/admin/login', secure: true, holder: 'agent' });
// the conversation folded to one line while the page is full width: its task, and what it's at (5)
const signupPeek = { agent: codex, title: 'Sign-up flow works end to end', line: 'Signed up and in; now inviting a teammate' };
const askSignIn = { asks: [{ id: 'ask-1', tab: 'admin', kind: 'signIn' as const, agent: 'codex' as const, host: 'staging.acme.dev', field: 'Password', reason: 'Sign in to the staging admin. The mail log is behind a sign-in.' }] };
// 9: one ask of each other kind, one at a time over the message box
const everyAsk = {
  asks: [
    { id: 'ask-captcha', tab: 'admin', kind: 'captcha' as const, agent: 'codex' as const, host: 'staging.acme.dev' },
    { id: 'ask-choice', tab: 'pricing', kind: 'choice' as const, agent: 'claude' as const, host: 'staging.acme.dev', reason: 'Which plan should I pick at checkout? The test card works with both. Business adds SSO, which the next step checks.', choices: ['Team · $12', 'Business · $29'] },
    { id: 'ask-code', tab: 'admin', kind: 'code' as const, agent: 'codex' as const, host: 'staging.acme.dev', field: '#otp' },
    { id: 'ask-passkey', tab: 'admin', kind: 'passkey' as const, agent: 'codex' as const, host: 'staging.acme.dev' },
  ],
};

const BOARDS: Record<number, Demo> = {
  1: { state: state([signIn, signUp]), ui: { open: false }, peek: signupPeek },
  2: { state: state([signIn, signUp]), peek: signupPeek },
  3: {
    state: state([
      t('pricing', { owner: claude, title: 'Pricing · Acme', url: 'staging.acme.dev/pricing', secure: true, action: 'Scrolling to Plans', pointer: { x: 330, y: 160 }, dot: 'working', closable: false }),
      t('signin', { title: 'Sign in · Acme', url: 'localhost:3000/login', action: 'Clicking Sign in', dot: 'working' }),
      t('button', { owner: cursor, title: 'Button · Storybook', url: 'localhost:6006/button', action: 'Opening Button', dot: 'working' }),
      t('status', { owner: { kind: 'you' }, title: 'Status · Acme', url: 'status.acme.dev', holder: 'you', basis: 130 }),
    ]),
    ui: { width: 600 },
  },
  4: { state: state([t('signup', { title: 'Sign up · Acme', url: 'localhost:3000/signup', action: 'Typing in Email', pointer: { x: 250, y: 192 } })]) },
  5: {
    state: state([t('workspace', { title: 'Workspace · Acme', url: 'localhost:3000/workspace/members', action: 'Clicking Invite', pointer: { x: 1205, y: 26, ring: true } }), signIn]),
    ui: { full: true },
    peek: signupPeek,
  },
  6: { state: state([{ ...signIn, holder: 'you', hold: 1 }]), peek: signupPeek },
  7: { state: state([admin], askSignIn) },
  8: { state: state([admin], askSignIn), ui: { card: 'password', typed: 10 } },
  9: { state: state([admin, t('pricing', { owner: claude, title: 'Pricing · Acme', url: 'staging.acme.dev/pricing', secure: true })], everyAsk) },
  10: {
    state: state([
      t('loading', { title: 'Dashboard', url: 'localhost:3000/dashboard', state: 'loading' }),
      t('failed', { title: 'Dashboard', url: 'localhost:3000/dashboard', state: 'failed' }),
      t('timed', { title: 'Health', url: 'api.acme.dev/health', state: 'timedOut', secure: true }),
      t('dialog', { title: 'Settings', url: 'localhost:3000/settings', state: 'dialog', dialog: 'Leave this page? Changes you made may not be saved.' }),
      t('cert', { title: 'Admin', url: 'staging.acme.dev/admin', state: 'certificate' }),
      t('crashed', { owner: cursor, title: 'Reports', url: 'localhost:5173/reports', state: 'crashed' }),
    ]),
  },
  11: { state: state([t('members', { title: 'Members · Acme', url: 'localhost:3000/workspace/members', holder: null, ran: { ms: 18 } })]), ui: { width: 600 } },
  12: {
    state: state([t('invoices', { title: 'Invoices · Billing', url: 'billing.acme.dev/invoices', secure: true, holder: null })], {
      downloads: [
        { id: 'csv', name: 'invoices-2026-10.csv', size: '48 KB', by: codex },
        { id: 'pdf', name: 'statement-oct.pdf', by: { kind: 'you' }, saved: true },
      ],
    }),
    ui: { width: 600 },
  },
  14: { state: state([t('dash', { owner: claude, title: 'Dashboard · Acme', url: 'localhost:3000/dashboard', action: 'Reading the dashboard', computer: { name: 'devbox', os: 'Linux' } })]), ui: { width: 620 } },
  17: { state: state([]), ui: { width: 600 } },
};

export const demoBoard = (n: number): Demo | null => BOARDS[n] ?? null;

/** The demo page a tab shows, and how a person hears it (the page's role=img label). */
export function demoPage(tab: Tab): { page: ReactNode; label: string } {
  switch (tab.id) {
    case 'signin': return tab.holder === 'you'
      ? { page: <Site.SignIn focus />, label: 'Acme’s sign-in page' }
      : { page: <Site.SignIn />, label: 'The page Codex is on: Acme’s sign-in, filled in' };
    case 'signup': return { page: <Site.SignUp />, label: 'Acme’s sign-up page, Codex typing the email' };
    case 'workspace': return { page: <Site.Members />, label: 'Acme’s members page, Codex clicking Invite' };
    case 'pricing': return { page: <Site.Pricing />, label: 'Acme’s pricing page, which Claude Code is reading' };
    case 'admin': return { page: <Site.AdminSignIn tab={tab.id} />, label: 'The staging admin’s sign-in page' };
    case 'members': return { page: <Site.MembersScript />, label: 'Acme’s members page, three rows marked by the script' };
    case 'invoices': return { page: <Site.Invoices />, label: 'Acme Billing’s invoices page' };
    case 'dash': return { page: <Site.Dashboard />, label: 'The dashboard running on devbox' };
    case 'dialog': return { page: <Site.Settings />, label: 'Acme’s settings page' };
    case 'loading': return { page: <><Site.Bone w="40%" top={22} h={14} /><Site.Bone w="70%" /><Site.Bone w="60%" /><Site.Bone w="66%" /></>, label: 'The dashboard, loading' };
    default: return { page: null, label: tab.title };
  }
}
