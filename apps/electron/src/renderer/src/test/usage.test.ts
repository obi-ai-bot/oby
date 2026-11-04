import { describe, expect, it, vi } from 'vitest';

// The Usage panel's and Settings › Agents' rows from obid's status (docs/contracts/usage.md).

async function load() {
  vi.resetModules();
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', { app: { obid: { call: async () => ({ status: 404, text: '' }) } } });
  return import('../SettingsView');
}

const now = Date.parse('2026-10-30T20:00:00Z');
const account = (more: Record<string, unknown>) =>
  ({ id: 'a', agent: 'claude', name: 'Default', signedIn: true, fiveHour: null, sevenDay: null, sevenDayResets: null, running: 0, ...more }) as never;

describe('usage rows', () => {
  it('USAGE-1: a Claude Code account read before any task shows its session and its week', async () => {
    const { accountRow, weekWords } = await load();
    const row = accountRow(account({ fiveHour: 0.04, fiveHourResets: '2026-10-30T20:29:59Z', sevenDay: 0.09, sevenDayResets: '2026-11-04T21:59:59Z' }), now);
    expect(row.session).toBe(4);
    expect(row.sessionResets).toBeTruthy();
    expect(weekWords(row)).toBe(`This week 9% · resets ${new Date('2026-11-04T21:59:59Z').toLocaleDateString([], { weekday: 'long' })}`);
    // not read yet: Sangam's words
    expect(accountRow(account({}), now).resets).toBe('No use reported yet');
    // a session past its reset reads empty, with no week to show instead
    expect(accountRow(account({ fiveHour: 0.5, fiveHourResets: '2026-10-30T19:00:00Z' }), now).session).toBe(0);
  });

  it('USAGE-10: an ended session beside a week shows the week: a week at 91% never reads 0%', async () => {
    const { accountRow, barOf, weekWords } = await load();
    const row = accountRow(account({ fiveHour: 0.04, fiveHourResets: '2026-10-26T14:00:00Z', sevenDay: 0.91, sevenDayResets: '2026-11-05T22:00:00Z' }), now);
    expect([row.session, row.sessionResets, barOf(row)]).toEqual([null, null, 91]);
    expect(weekWords(row)).toBe(`This week 91% · resets ${new Date('2026-11-05T22:00:00Z').toLocaleDateString([], { weekday: 'long' })}`);
    // a session still going keeps the bar, at 0% too
    const live = accountRow(account({ agent: 'codex', fiveHour: 0, fiveHourResets: '2026-10-30T21:00:00Z', sevenDay: 0.66 }), now);
    expect(barOf(live)).toBe(0);
  });

  it('USAGE-6: Cursor shows the figure obid gives, naming no week, and "No usage" with none', async () => {
    const { accountRow, weekWords, barOf } = await load();
    // a weekly plan: its reset by its day
    const soon = '2026-11-04T21:59:59Z';
    const week = accountRow(account({ agent: 'cursor', sevenDay: 0.31, sevenDayResets: soon }), now);
    expect([week.used, week.session, barOf(week)]).toEqual([31, null, 31]);
    expect(weekWords(week)).toBe(`31% used · resets ${new Date(soon).toLocaleDateString([], { weekday: 'long' })}`);
    // a monthly plan: its reset by its date
    const later = '2026-11-20T12:00:00Z';
    const month = accountRow(account({ agent: 'cursor', sevenDay: 0.5, sevenDayResets: later }), now);
    expect(weekWords(month)).toBe(`50% used · resets ${new Date(later).toLocaleDateString([], { month: 'short', day: 'numeric' })}`);
    // none: a read that failed, a plan with no figure
    const none = accountRow(account({ agent: 'cursor' }), now);
    expect([none.used, none.session, weekWords(none)]).toEqual([null, null, 'No usage']);
    // signed out: what obid says of the login
    expect(weekWords(accountRow(account({ agent: 'cursor', signedIn: false, label: 'Sign in again' }), now))).toBe('Sign in again');
  });
});
