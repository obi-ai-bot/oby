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

  it('USAGE-18: the bar’s window against an even pace, and what’s left of it, worked out from what obid says', async () => {
    const { accountRow } = await load();
    const { paceLine } = await import('../pace');
    const at = (hours: number) => new Date(now + hours * 3_600_000).toISOString();
    // a session 3 of its 5 hours gone (60%): 80% used is ahead, 58% on pace, 30% under
    const session = (used: number) => accountRow(account({ fiveHour: used, fiveHourResets: at(2), sevenDay: 0.2, sevenDayResets: at(100) }), now);
    expect(paceLine(session(0.8), now)).toBe('Ahead of pace · 2h 0m of the session left');
    expect(paceLine(session(0.58), now)).toBe('On pace · 2h 0m of the session left');
    expect(paceLine(session(0.3), now)).toBe('Under pace · 2h 0m of the session left');
    // with no session showing, the week's: 3 of its 7 days left is 4/7 gone (57%)
    const week = accountRow(account({ sevenDay: 0.9, sevenDayResets: at(72) }), now);
    expect(week.session).toBeNull();
    expect(paceLine(week, now)).toBe('Ahead of pace · 3d 0h of the week left');
    // no window obid knows the length of: Cursor's plan, no figure, a window that ended
    expect(paceLine(accountRow(account({ agent: 'cursor', sevenDay: 0.5, sevenDayResets: at(48) }), now), now)).toBeNull();
    expect(paceLine(accountRow(account({}), now), now)).toBeNull();
    expect(paceLine(accountRow(account({ sevenDay: 0.5, sevenDayResets: at(-1) }), now), now)).toBeNull();
  });
});
