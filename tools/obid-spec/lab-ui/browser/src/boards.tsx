// The design's 17 boards: each one's number, its canvas file (design/boards), its size, the screen
// that draws it and the fixture that puts it on screen with no server. A board's page is
// /#/<n> (fixtures) or /#/<n>?live (live: lane A's API through this app's server).
import type { ReactNode } from 'react';
import type { BrowserState, ScreenHints, Tab } from './api/types';
import type { Ui } from './store';
import * as S from './scenes';
import { Desktop, DesktopPermission } from './desktop';

export type Board = { n: number; file: string; title: string; width: number; height: number; screen: () => ReactNode; state: BrowserState; ui?: Partial<Ui> };

const ON = { agentsUseBrowser: true, agentsUseMac: true };
const state = (tabs: BrowserState['tabs'], extra: Partial<BrowserState> = {}): BrowserState => ({ tabs, active: tabs[0]?.id ?? null, asks: [], downloads: [], settings: ON, ...extra });
const codex = { kind: 'agent', agent: 'codex' } as const;
const claude = { kind: 'agent', agent: 'claude' } as const;
const cursor = { kind: 'agent', agent: 'cursor' } as const;
const t = (id: string, rest: Partial<Tab & ScreenHints> & Pick<Tab, 'title' | 'url'>): Tab & ScreenHints => ({ id, owner: codex, state: 'ready', holder: 'agent', ...rest });

const signIn = t('signin', { title: 'Sign in · Acme', url: 'localhost:3000/login', action: 'Clicking Sign in', pointer: { x: 322, y: 338, ring: true } });
const signUp = t('signup', { title: 'Sign up · Acme', url: 'localhost:3000/signup', action: 'Clicking Sign in' });

export const BOARDS: Board[] = [
  { n: 1, file: 'Closed', title: 'Closed: the agent works in the browser, a globe in the title bar opens it', width: 1280, height: 800, screen: () => <S.Signup />, state: state([signIn, signUp]), ui: { open: false } },
  { n: 2, file: 'Main', title: 'Open beside the conversation: the agent drives, you watch', width: 1280, height: 800, screen: () => <S.Signup />, state: state([signIn, signUp]) },
  {
    n: 3, file: 'Together', title: 'Side panel and browser together; tabs from several agents', width: 1440, height: 900, screen: () => <S.Together />,
    state: state([
      t('pricing', { owner: claude, title: 'Pricing · Acme', url: 'staging.acme.dev/pricing', secure: true, action: 'Scrolling to Plans', pointer: { x: 330, y: 160 }, dot: 'working', closable: false }),
      t('signin', { title: 'Sign in · Acme', url: 'localhost:3000/login', action: 'Clicking Sign in', dot: 'working' }),
      t('button', { owner: cursor, title: 'Button · Storybook', url: 'localhost:6006/button', action: 'Opening Button', dot: 'working' }),
      t('status', { owner: { kind: 'you' }, title: 'Status · Acme', url: 'status.acme.dev', holder: 'you', basis: 130 }),
    ]),
  },
  { n: 4, file: 'Narrow', title: 'A narrow window: the conversation keeps 420px', width: 994, height: 670, screen: () => <S.Narrow />, state: state([t('signup', { title: 'Sign up · Acme', url: 'localhost:3000/signup', action: 'Typing in Email', pointer: { x: 250, y: 192 } })]) },
  {
    n: 5, file: 'Full', title: 'Full width: the page across the window', width: 1280, height: 800, screen: () => <S.Signup />,
    state: state([t('workspace', { title: 'Workspace · Acme', url: 'localhost:3000/workspace/members', action: 'Clicking Invite', pointer: { x: 1205, y: 26, ring: true } }), signIn]),
    ui: { full: true },
  },
  { n: 6, file: 'Hold', title: 'Take control: you hold the tab, the agent waits, its click is refused', width: 1280, height: 800, screen: () => <S.Signup signedUp={false} chips={false} />, state: state([{ ...signIn, holder: 'you', hold: 1 }]) },
  {
    n: 7, file: 'AsksYou', title: 'The agent asks you to take over: a sign-in', width: 1280, height: 800, screen: () => <S.Asks7 />,
    state: state([t('admin', { title: 'Sign in · Acme Admin', url: 'staging.acme.dev/admin/login', secure: true, holder: 'agent' })], { asks: [{ id: 'ask-1', tab: 'admin', kind: 'signIn', agent: 'codex', host: 'staging.acme.dev' }] }),
  },
  {
    n: 8, file: 'Secret', title: 'A password goes straight into the field, never to the agent', width: 1280, height: 800, screen: () => <S.Asks7 />,
    state: state([t('admin', { title: 'Sign in · Acme Admin', url: 'staging.acme.dev/admin/login', secure: true, holder: 'agent' })], { asks: [{ id: 'ask-1', tab: 'admin', kind: 'signIn', agent: 'codex', host: 'staging.acme.dev' }] }),
    ui: { card: 'password', typed: 10 },
  },
  { n: 9, file: 'Asks', title: 'Every ask: CAPTCHA, a choice, a code, a passkey; who has the tab', width: 1280, height: 800, screen: () => <S.Asks9 />, state: state([]) },
  {
    n: 10, file: 'PageStates', title: 'Page states: loading, failed, timed out, a dialog, certificate, crashed', width: 1280, height: 880, screen: () => <S.PageStates />,
    state: state([
      t('loading', { title: 'Dashboard', url: 'localhost:3000/dashboard', state: 'loading' }),
      t('failed', { title: 'Dashboard', url: 'localhost:3000/dashboard', state: 'failed' }),
      t('timed', { title: 'Health', url: 'api.acme.dev/health', state: 'timedOut', secure: true }),
      t('dialog', { title: 'Settings', url: 'localhost:3000/settings', state: 'dialog' }),
      t('cert', { title: 'Admin', url: 'staging.acme.dev/admin', state: 'certificate' }),
      t('crashed', { owner: cursor, title: 'Reports', url: 'localhost:5173/reports', state: 'crashed' }),
    ]),
  },
  { n: 11, file: 'Script', title: 'A script in the page, and its result as outside text', width: 1280, height: 800, screen: () => <S.Script />, state: state([t('members', { title: 'Members · Acme', url: 'localhost:3000/workspace/members', holder: null })]) },
  {
    n: 12, file: 'Downloads', title: 'Downloads: the agent’s stay in its browser, yours come to your Mac', width: 1280, height: 800, screen: () => <S.Downloads />,
    state: state([t('invoices', { title: 'Invoices · Billing', url: 'billing.acme.dev/invoices', secure: true, holder: null })], {
      downloads: [
        { id: 'csv', name: 'invoices-2026-10.csv', size: '48 KB', by: codex },
        { id: 'pdf', name: 'statement-oct.pdf', by: { kind: 'you' }, saved: true },
      ],
    }),
  },
  { n: 13, file: 'SettingsBrowser', title: 'Settings › General: two switches for the browser, on by default', width: 1200, height: 860, screen: () => <S.SettingsBrowser />, state: state([]) },
  {
    n: 14, file: 'OtherComputer', title: 'Another computer: the browser shows that computer’s localhost', width: 1280, height: 800, screen: () => <S.OtherComputer />,
    state: state([t('dash', { owner: claude, title: 'Dashboard · Acme', url: 'localhost:3000/dashboard', action: 'Reading the dashboard', computer: { name: 'devbox', os: 'Linux' } })]),
  },
  { n: 15, file: 'Desktop', title: 'An agent controls the Mac: what you see, and Stop', width: 1440, height: 900, screen: () => <Desktop />, state: state([]) },
  { n: 16, file: 'DesktopPermission', title: 'The first time: why macOS will ask, once', width: 1280, height: 800, screen: () => <DesktopPermission />, state: state([]) },
  { n: 17, file: 'Empty', title: 'First use: no pages yet', width: 1280, height: 800, screen: () => <S.Empty />, state: state([]) },
];
