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
    // a session past its reset reads empty
    expect(accountRow(account({ fiveHour: 0.5, fiveHourResets: '2026-10-30T19:00:00Z', sevenDay: 0.1 }), now).session).toBe(0);
  });

  it('USAGE-6: Cursor says it reports none', async () => {
    const { accountRow } = await load();
    const row = accountRow(account({ agent: 'cursor', fiveHour: 0.5, sevenDay: 0.5 }), now);
    expect([row.used, row.session, row.resets]).toEqual([null, null, 'Cursor doesn’t report usage']);
  });
});
