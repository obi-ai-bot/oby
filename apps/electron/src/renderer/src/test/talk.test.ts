import { describe, expect, it, vi } from 'vitest';

// The chat window's pill and menu (docs/contracts/conversation-agent.md).

async function load() {
  vi.resetModules();
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', { app: { obid: { call: async () => ({ status: 404, text: '' }) } } });
  const settings = await import('../SettingsView');
  return { ...settings, ...(await import('../talk')) };
}

const now = Date.parse('2026-11-02T20:00:00Z');
const wire = (id: string, agent: string, name: string, more: Record<string, unknown> = {}) =>
  ({ id, agent, name, signedIn: true, fiveHour: null, sevenDay: null, sevenDayResets: null, running: 0, ...more }) as never;

describe('talking on any agent', () => {
  it('CONVERSATION-AGENT-12: the pill and the menu show each account’s percentage, and Cursor none', async () => {
    const { accountRow, usageWords, percent, dot, groups } = await load();
    const personal = accountRow(wire('p', 'claude', 'Personal', { fiveHour: 0.5, sevenDay: 0.38 }), now);
    expect([percent(personal), usageWords(personal), dot(personal)]).toEqual([38, '38%', '']);
    const cursor = accountRow(wire('u', 'cursor', 'Personal', { fiveHour: 0.5, sevenDay: 0.5 }), now);
    expect([percent(cursor), usageWords(cursor), dot(cursor)]).toEqual([null, 'No usage reported', '']);
    // yellow past 80%, red past 90%, of the session or the week, as the message box's gauge
    expect(dot(accountRow(wire('a', 'claude', 'A', { fiveHour: 0.85, sevenDay: 0.1 }), now))).toBe('is-high');
    expect(dot(accountRow(wire('b', 'codex', 'B', { fiveHour: 0.1, sevenDay: 0.95 }), now))).toBe('is-over');

    const signedOut = accountRow(wire('o', 'codex', 'Out', { signedIn: false }), now);
    const codex = accountRow(wire('x', 'codex', 'Work'), now);
    const list = groups([cursor, codex, signedOut, personal]);
    expect(list.map((g) => [g.agent, g.accounts.map((a) => a.id)])).toEqual([['claude', ['p']], ['codex', ['x']], ['cursor', ['u']]]);
  });

  it('CONVERSATION-AGENT-13: the pill names the chosen account, else where it talked, else what obid says is next', async () => {
    const { accountRow, shownAccount, chosenOf } = await load();
    const p = accountRow(wire('p', 'claude', 'Personal', { sevenDay: 0.2 }), now);
    const x = accountRow(wire('x', 'codex', 'Work'), now);
    const full = accountRow(wire('f', 'claude', 'Full', { fullUntil: '2026-11-02T21:00:00Z' }), now);
    const out = accountRow(wire('o', 'claude', 'Out', { signedIn: false }), now);
    const all = [p, x, full, out];
    // Chosen, at once, whatever it talked on before.
    expect(shownAccount(all, 'x', 'p', 'p')?.id).toBe('x');
    // The chosen one filled, or can't run: where it moved to.
    expect(shownAccount(all, 'f', 'x', 'p')?.id).toBe('x');
    expect(shownAccount(all, 'o', 'x', 'p')?.id).toBe('x');
    // Auto: where it talked, else what obid says a first turn would use.
    expect(shownAccount(all, null, 'x', 'p')?.id).toBe('x');
    expect(shownAccount(all, null, null, 'p')?.id).toBe('p');
    // A conversation's own choice over the setting; `auto` is Auto whatever the setting says.
    expect(chosenOf('x', 'p')).toBe('x');
    expect(chosenOf(null, 'p')).toBe('p');
    expect(chosenOf('auto', 'p')).toBe(null);
    expect(chosenOf(undefined, null)).toBe(null);
  });
});
