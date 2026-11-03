import { spawnSync } from 'node:child_process';
import { chmodSync, cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Continue with ChatGPT's gate (docs/contracts/chatgpt-sign-in.md, "The gate"): every way in as the
// app renders it, in a release and in development, and what scripts/package.sh refuses.
vi.mock('react', async (original) => ({
  ...await original<typeof import('react')>(),
  useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
}));
vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));

const BUTTON = 'Continue with ChatGPT';
const REPORT = 'https://github.com/obi-ai-bot/oby/issues/1879#issuecomment-123';
const row = (more: Record<string, unknown>) => ({ id: 'codex', agent: 'codex', name: 'Personal', signedIn: false, fiveHour: null, sevenDay: null, sevenDayResets: null, running: 0, ...more });

// The app as a build of this kind renders it, on an obid with these accounts
async function built({ packaged, report = '', search = '', accounts = [row({})] }: { packaged: boolean; report?: string; search?: string; accounts?: unknown[] }) {
  vi.resetModules();
  vi.stubEnv('DEV', !packaged);
  vi.doMock('../chatgptGate', () => ({ REAL_CHATGPT_SIGN_IN_REPORT: report }));
  vi.stubGlobal('location', { search });
  // (Deskree's own defaults, all of them on: none is the gate)
  vi.stubGlobal('localStorage', { getItem: () => '1', setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('window', { app: { obid: { call: async () => ({ status: 404, text: '{}' }) } } });
  vi.stubGlobal('document', { body: {}, createElement: () => ({}) });
  const status = { accounts, availableLogins: [] };
  vi.doMock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status, repos: [] }) }));
  vi.doMock('../onboarding', async (original) => ({ ...await original<typeof import('../onboarding')>(), useSetup: () => ({ found: 3, agents: { claude: 'out', codex: 'out', cursor: 'out' }, github: 'out', tetrix: 'out', codes: {}, problems: {} }) }));
  const { AgentRows } = await import('../SetupCard');
  const settings = await import('../SettingsView');
  settings.replaceAccounts(accounts.map((a) => settings.accountRow(a as never, Date.now())));
  const none = () => {};
  return {
    firstRun: renderToStaticMarkup(<AgentRows bare />),
    setupCard: renderToStaticMarkup(<AgentRows />),
    agents: renderToStaticMarkup(<settings.Agents />),
    addAccount: renderToStaticMarkup(<settings.AddAccount accounts={[]} setAccounts={none} found={[]} onFound={none} onClose={none} />),
  };
}

// scripts/package.sh on its own in a folder of its own, its gate saying this. It stops by itself past
// the gate (there's no package.json here), so nothing gets built or signed.
function release(gateLine: string, confirmed = '') {
  const folder = mkdtempSync(join(tmpdir(), 'oby-gate-'));
  try {
    mkdirSync(join(folder, 'scripts'));
    mkdirSync(join(folder, 'src/renderer/src'), { recursive: true });
    mkdirSync(join(folder, 'bin'));
    cpSync(resolve(__dirname, '../../../../scripts/package.sh'), join(folder, 'scripts/package.sh'));
    writeFileSync(join(folder, 'src/renderer/src/chatgptGate.ts'), `// the gate\n${gateLine}\n`);
    // GitHub, as it answers for that comment (and nothing else on PATH that could build)
    writeFileSync(join(folder, 'bin/gh'), `#!/bin/sh\n[ -n "${confirmed}" ] && echo "${confirmed}" && exit 0\nexit 1\n`);
    chmodSync(join(folder, 'bin/gh'), 0o755);
    const { status, stdout, stderr } = spawnSync('/bin/zsh', [join(folder, 'scripts/package.sh'), 'arm64'], { encoding: 'utf8', env: { PATH: `${join(folder, 'bin')}:/usr/bin:/bin`, HOME: folder } });
    return { status, words: stdout + stderr };
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}
const gate = (value: string) => `export const REAL_CHATGPT_SIGN_IN_REPORT = '${value}';`;
const REFUSED = /error: .*(chatgptGate|real sign-in)/;

describe('Continue with ChatGPT’s gate', () => {
  afterEach(() => { vi.unstubAllGlobals(); vi.unstubAllEnvs(); vi.doUnmock('../chatgptGate'); vi.doUnmock('../live'); vi.doUnmock('../onboarding'); });

  it('chatgpt_gate_1_packaged_build_hides_the_button: first run, Settings › Agents and Add account, with their own sign-in left', async () => {
    const app = await built({ packaged: true });
    for (const [door, html] of Object.entries(app)) expect(html, door).not.toContain(BUTTON);
    expect(app.firstRun).toContain('Connect');
    expect(app.addAccount).toContain('Sign in');
    expect(app.addAccount).not.toContain('Use a code instead');
  });

  it('chatgpt_gate_2_release_script_refuses_without_the_real_run', () => {
    for (const value of ['yes', 'true', 'https://github.com/obi-ai-bot/oby/issues/1879', 'https://github.com/obi-ai-bot/oby/issues/1#issuecomment-123']) {
      const r = release(gate(value), value);
      expect(r.status, value).not.toBe(0);
      expect(r.words, value).toMatch(REFUSED);
    }
    // the right shape, but GitHub has no such comment, or has it somewhere else, or can't be asked
    for (const answer of ['', 'https://github.com/obi-ai-bot/oby/issues/1879#issuecomment-999']) expect(release(gate(REPORT), answer).words, answer).toContain('GitHub doesn\'t confirm');
    // a gate it can't read isn't an open one
    expect(release('export const REAL_CHATGPT_SIGN_IN_REPORT = `x`;').words).toMatch(REFUSED);
    // past the gate: left empty it says the button stays hidden; a confirmed report goes on
    const hidden = release(gate(''));
    expect(hidden.words).not.toMatch(REFUSED);
    expect(hidden.words).toContain('stays hidden');
    const proven = release(gate(REPORT), REPORT);
    expect(proven.words).not.toMatch(REFUSED);
    expect(proven.words).toContain(`shows in this build: ${REPORT}`);
  });

  it('chatgpt_gate_3_development_build_keeps_the_button, so the real sign-in can be run', async () => {
    const app = await built({ packaged: false });
    expect(app.firstRun).toContain(BUTTON);
    expect(app.agents).toContain(BUTTON);
    expect(app.addAccount).toContain(BUTTON);
    expect(app.addAccount).toContain('Use a code instead');
  });

  it('chatgpt_gate_4_gate_set_shows_the_button_in_a_packaged_build', async () => {
    const app = await built({ packaged: true, report: REPORT });
    expect(app.firstRun).toContain(BUTTON);
    expect(app.addAccount).toContain(BUTTON);
  });

  it('chatgpt_gate_5_everything_and_stored_defaults_dont_open_it', async () => {
    const app = await built({ packaged: true, search: '?everything&home&chat&welcome&dev' });
    for (const [door, html] of Object.entries(app)) expect(html, door).not.toContain(BUTTON);
  });

  it('chatgpt_gate_6_an_account_signed_in_this_way_keeps_working', async () => {
    // signed in: nothing asks it to sign in, and OpenAI's words stay
    const working = await built({ packaged: true, accounts: [row({ login: 'chatgpt', signedIn: true })] });
    expect(working.agents).toContain('Using ChatGPT plan');
    expect(working.agents).toContain('Manage usage');
    // its login ended: its own row keeps the way back, and that's the only door open
    const ended = await built({ packaged: true, accounts: [row({ login: 'chatgpt', label: 'Continue with ChatGPT again' })] });
    expect(ended.agents).toContain(`>${BUTTON}</button>`);
    expect(ended.firstRun).not.toContain(BUTTON);
    expect(ended.addAccount).not.toContain(BUTTON);
  });
});
