import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Gitleaks } from '../../../main/help';

// The Help menu (docs/contracts/help.md, HELP-1 to 5): obid's log shown in Finder, and Copy
// Diagnostics through the secret scan. Main's help.ts runs for real, with Electron and obid stood in
// for, on a fixture log (never a real one).
const the = vi.hoisted(() => ({
  copied: [] as string[],
  shown: [] as string[],
  handlers: new Map<string, (e: unknown, ...args: unknown[]) => unknown>(),
  home: '',
  status: { status: 200, text: JSON.stringify({ runner: { version: '0.2.24', host: 'mac', commit: 'def5678def5678def5678def5678def5678def56' } }) },
}));
vi.mock('electron', () => ({
  app: { getVersion: () => '0.2.24', getPath: () => '/nonexistent/Oby.app/Contents/MacOS/Oby' },
  clipboard: { writeText: (t: string) => { the.copied.push(t); } },
  shell: { showItemInFolder: (p: string) => { the.shown.push(p); } },
  dialog: { showMessageBox: async () => ({ response: 0 }) },
  ipcMain: { handle: (name: string, f: (e: unknown, ...args: unknown[]) => unknown) => { the.handlers.set(name, f); }, on() {} },
}));
vi.mock('../../../main/obid', () => ({ obid: async () => the.status }));
// a home of the test's own, so ~/Library/Logs/obid.log is a fixture's
vi.mock('node:os', async (original) => {
  const os = await original<typeof import('node:os')>();
  the.home = os.tmpdir() + `/oby-help-home-${process.pid}`;
  return { ...os, homedir: () => the.home, default: { ...os, homedir: () => the.home } };
});

const help = await import('../../../main/help');
const folder = mkdtempSync(join(tmpdir(), 'oby-help-'));
afterAll(() => { rmSync(folder, { recursive: true, force: true }); rmSync(the.home, { recursive: true, force: true }); });
beforeEach(() => { the.copied.length = 0; the.shown.length = 0; });

// a fixture log of n numbered lines
function log(n: number, more: string[] = []) {
  const path = join(folder, `obid-${n}-${more.length}.log`);
  writeFileSync(path, [...Array.from({ length: n }, (_, i) => `2026-11-19T10:00:00Z INFO obid: fixture line ${i + 1}`), ...more].join('\n') + '\n');
  return path;
}
// a made-up GitHub token, built here so this file holds none
const token = () => `ghp_${[...randomBytes(36)].map((b) => 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'[b % 62]).join('')}`;
// gitleaks as it behaves on GitHub tokens: each line holding one is a finding (exit 99), none exit 0
const fakeGitleaks: Gitleaks = async (input, report) => {
  const { readFileSync } = await import('node:fs');
  const found = readFileSync(input, 'utf8').split('\n').flatMap((line, i) => (/ghp_[A-Za-z0-9]{36}/.test(line) ? [{ StartLine: i + 1, RuleID: 'github-pat' }] : []));
  writeFileSync(report, JSON.stringify(found));
  return found.length ? 99 : 0;
};
const realGitleaks = ['/opt/homebrew/bin/gitleaks', '/usr/local/bin/gitleaks', '/usr/bin/gitleaks'].find((p) => existsSync(p));

describe('HELP: Reveal obid’s Log and Copy Diagnostics', () => {
  it('help_1_reveal_obids_log_shows_the_file_in_finder', async () => {
    const path = log(3);
    expect(help.revealLog(path)).toBeNull();
    expect(the.shown).toEqual([path]);
    // no log yet: nothing shown, and why
    expect(help.revealLog(join(folder, 'none.log'))).toBe('obid hasn’t written a log yet (~/Library/Logs/obid.log).');
    expect(the.shown).toHaveLength(1);
    // the Help menu's items run main's actions
    const { appMenu } = await import('../../../shared/menu');
    const revealLog = vi.fn(), copyDiagnostics = vi.fn();
    const helpMenu = appMenu({ dev: false, help: { revealLog, copyDiagnostics } }).find((m) => m.role === 'help')!;
    const items = helpMenu.submenu as { label: string; click: () => void }[];
    expect(items.map((i) => i.label)).toEqual(['Reveal obid’s Log', 'Copy Diagnostics']);
    items.forEach((i) => i.click());
    expect([revealLog.mock.calls.length, copyDiagnostics.mock.calls.length]).toEqual([1, 1]);
  });

  it('help_2_copy_diagnostics_copies_the_versions_the_mac_and_the_logs_last_200_lines', async () => {
    const facts = await help.facts(log(250));
    expect(facts.obid).toEqual({ version: '0.2.24', commit: 'def5678def5678def5678def5678def5678def56' });
    expect(facts.arch).toBe(process.arch);
    expect(facts.log).toHaveLength(200);
    expect(facts.log![0]).toContain('fixture line 51');
    expect(facts.log!.at(-1)).toContain('fixture line 250');
    expect(await help.copyDiagnostics({ facts: async () => facts, scan: async () => ({ kind: 'clean' }) })).toBeNull();
    const [text] = the.copied;
    const head = text.split('\n').slice(0, 6);
    expect(head[0]).toMatch(/^Oby: 0\.2\.24 \(commit /);
    expect(head[1]).toBe('obid: 0.2.24 (commit def5678def5678def5678def5678def5678def56)');
    expect(head[2]).toMatch(/^macOS: \S/);
    expect(head[3]).toBe(`Architecture: ${process.arch}`);
    expect(head[5]).toBe('obid\'s log, its last 200 lines (~/Library/Logs/obid.log):');
    expect(text.split('\n')).toHaveLength(206);
    // obid not answering, and no log: said so, the rest still copied
    the.status = { status: 0, text: '' };
    const none = await help.facts(join(folder, 'none.log'));
    expect(help.diagnosticsText(none).split('\n').slice(1, 6)).toEqual(['obid: isn’t running', `macOS: ${none.macos}`, `Architecture: ${process.arch}`, '', 'obid\'s log: none at ~/Library/Logs/obid.log']);
    // a log longer than what's read: its last lines, none cut in half
    const long = help.tail(log(4000), 200)!;
    expect(long).toHaveLength(200);
    expect(long[0]).toBe('2026-11-19T10:00:00Z INFO obid: fixture line 3801');
  });

  it('help_3_a_secret_in_the_diagnostics_copies_nothing', async () => {
    const facts = await help.facts(log(10, [`2026-11-19T10:00:01Z WARN obid: token=${token()}`]));
    const scan = (text: string) => help.scanText(text, fakeGitleaks);
    expect(await help.copyDiagnostics({ facts: async () => facts, scan })).toBe('The secret scan found what looks like a secret (github-pat), so nothing was copied.');
    expect(the.copied).toEqual([]);
    // the same text without it is clean, and copied
    expect(await help.copyDiagnostics({ facts: async () => ({ ...facts, log: facts.log!.slice(0, 10) }), scan })).toBeNull();
    expect(the.copied).toHaveLength(1);
  });

  it.skipIf(!realGitleaks)('help_3_with_the_real_gitleaks', async () => {
    const run = help.runGitleaks(realGitleaks!);
    expect(await help.scanText('Oby: 0.2.24\nobid: fine', run)).toEqual({ kind: 'clean' });
    expect(await help.scanText(`Oby: 0.2.24\nobid: token=${token()}`, run)).toEqual({ kind: 'found', rules: ['github-pat'] });
  });

  it('help_4_a_scan_that_cant_run_copies_nothing', async () => {
    const facts = await help.facts(log(5));
    const copy = (gitleaks: Gitleaks | null) => help.copyDiagnostics({ facts: async () => facts, scan: (t) => help.scanText(t, gitleaks) });
    // no gitleaks
    expect(await copy(null)).toBe('The secret scan couldn’t run (gitleaks isn’t installed), so nothing was copied.');
    // one that fails, or can't start
    expect(await copy(async () => 1)).toBe('The secret scan couldn’t run (gitleaks stopped with code 1), so nothing was copied.');
    expect(await copy(async () => { throw new Error('spawn EACCES'); })).toBe('The secret scan couldn’t run (gitleaks didn’t run: spawn EACCES), so nothing was copied.');
    // one that says clean without having read the text (its canary unseen)
    expect(await copy(async (_i, report) => { writeFileSync(report, '[]'); return 0; })).toBe('The secret scan couldn’t run (gitleaks didn’t read the text), so nothing was copied.');
    // one that wrote no report
    expect(await copy(async () => 0)).toBe('The secret scan couldn’t run (gitleaks wrote no report), so nothing was copied.');
    expect(the.copied).toEqual([]);
  });

  it('help_5_the_page_asks_main_for_them_through_its_own_channel', async () => {
    let mine = true;
    help.helping(() => mine);
    const ask = the.handlers.get('help')!;
    // anything else than the two is refused before it runs
    expect(await ask({}, 'rm -rf')).toBe('Not this app’s page.');
    expect(help.OBID_LOG).toBe(join(the.home, 'Library/Logs/obid.log'));
    expect(await ask({}, 'reveal-log')).toBe('obid hasn’t written a log yet (~/Library/Logs/obid.log).');
    mkdirSync(join(the.home, 'Library/Logs'), { recursive: true });
    writeFileSync(help.OBID_LOG, 'fixture\n');
    expect(await ask({}, 'reveal-log')).toBeNull();
    expect(the.shown).toEqual([help.OBID_LOG]);
    mine = false;
    expect(await ask({}, 'copy-diagnostics')).toBe('Not this app’s page.');
    expect(the.copied).toEqual([]);
  });
});
