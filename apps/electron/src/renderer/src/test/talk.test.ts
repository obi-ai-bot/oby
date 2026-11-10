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
  it('AGENTS-6: a saved model is called unavailable only once obid has read the account’s own list', async () => {
    const { modelChoice } = await load();
    const offered = [{ id: 'gpt-6-astra', name: 'GPT-6-Astra' }];
    expect(modelChoice('gpt-7', 'gpt-6-astra', offered, false)).toMatchObject({ id: 'gpt-7', unavailable: false });
    expect(modelChoice('gpt-7', 'gpt-6-astra', offered, true)).toMatchObject({ id: 'gpt-6-astra', name: 'GPT-6-Astra', unavailable: true });
    expect(modelChoice('gpt-6-astra', 'auto', offered, true)).toMatchObject({ id: 'gpt-6-astra', unavailable: false });
  });

  it('AGENTS-11: the message box lists Cursor\'s models once, kept to the account\'s own ids, each pick a listed id', async () => {
    const { menuModels } = await load();
    const groups = [
      { base: 'grok-4.7', name: 'Grok 4.7', default: 'grok-4.7-medium', variants: [
        { id: 'grok-4.7-low', effort: 'low', fast: false }, { id: 'grok-4.7-low-fast', effort: 'low', fast: true },
        { id: 'grok-4.7-medium', effort: 'medium', fast: false }, { id: 'grok-4.7-medium-fast', effort: 'medium', fast: true }] },
      { base: 'gpt-5.3-codex', name: 'Codex 5.3', default: 'gpt-5.3-codex', variants: [{ id: 'gpt-5.3-codex', fast: false }, { id: 'gpt-5.3-codex-fast', fast: true }] },
      { base: 'gone', name: 'Gone', default: 'gone', variants: [{ id: 'gone', fast: false }] },
    ];
    const listed = ['grok-4.7-low', 'grok-4.7-low-fast', 'grok-4.7-medium', 'grok-4.7-medium-fast', 'own-model', 'gpt-5.3-codex', 'gpt-5.3-codex-fast'].map((id) => ({ id, name: id }));
    // an account's own id no group has stands alone, in the list's order; a group it lists none of goes
    expect(menuModels(listed, 'grok-4.7-low-fast', groups)).toEqual([
      { id: 'grok-4.7-low-fast', name: 'Grok 4.7', on: true },
      { id: 'own-model', name: 'own-model', on: false },
      { id: 'gpt-5.3-codex-fast', name: 'Codex 5.3', on: false },
    ]);
    // from a model with no effort: each model's default
    expect(menuModels(listed, 'own-model', groups).map((m) => m.id)).toEqual(['grok-4.7-medium', 'own-model', 'gpt-5.3-codex']);
    // no groups (an older obid, Claude Code, Codex): the list as it is
    expect(menuModels(listed.slice(0, 2), 'grok-4.7-low', undefined)).toEqual([{ id: 'grok-4.7-low', name: 'grok-4.7-low', on: true }, { id: 'grok-4.7-low-fast', name: 'grok-4.7-low-fast', on: false }]);
  });

  it('CONVERSATION-AGENT-15: the list shows the 5-hour session and the week, and an Ultra plan only its week', async () => {
    const { accountRow, usageLines } = await load();
    const words = (a: Parameters<typeof usageLines>[0]) => usageLines(a).map((l) => l.words);
    const soon = new Date(now + 3_600_000).toISOString();
    expect(words(accountRow(wire('m', 'claude', 'Max', { fiveHour: 0.12, fiveHourResets: soon, sevenDay: 0.59 }), now))).toEqual(['5h 12%', 'Week 59%']);
    const ultra = accountRow(wire('u', 'claude', 'Ultra', { plan: 'ultra', fiveHour: 0.4, fiveHourResets: soon, sevenDay: 0.59 }), now);
    expect(words(ultra)).toEqual(['Week 59%']);
    expect(ultra.session).toBeNull();
    expect(words(accountRow(wire('o', 'codex', 'Only session', { fiveHour: 0.3, fiveHourResets: soon }), now))).toEqual(['5h 30%']);
    expect(words(accountRow(wire('c', 'cursor', 'Cursor', { sevenDay: 0.5 }), now))).toEqual(['50%']);
    expect(words(accountRow(wire('n', 'cursor', 'None'), now))).toEqual(['No usage']);
    expect(usageLines(accountRow(wire('h', 'claude', 'High', { fiveHour: 0.95, fiveHourResets: soon, sevenDay: 0.2 }), now))[0].cls).toBe('is-over');
    expect(words(accountRow(wire('f', 'claude', 'Full', { fullUntil: soon, sevenDay: 0.2 }), now))).toEqual(['Full']);
  });

  it('CONVERSATION-AGENT-12: the pill and the menu show each account’s percentage, Cursor’s too, and “No usage” with none', async () => {
    const { accountRow, usageWords, percent, dot, groups } = await load();
    const personal = accountRow(wire('p', 'claude', 'Personal', { fiveHour: 0.2, sevenDay: 0.38 }), now);
    expect([percent(personal), usageWords(personal), dot(personal)]).toEqual([38, '38%', '']);
    // the limit that hits first: the session past the week
    const session = accountRow(wire('s', 'claude', 'Session', { fiveHour: 0.93, sevenDay: 0.38 }), now);
    expect([percent(session), usageWords(session), dot(session)]).toEqual([93, '93%', 'is-over']);
    // held full: said, whatever its figures read
    const held = accountRow(wire('h', 'claude', 'Held', { sevenDay: 0.38, fullUntil: '2026-11-02T21:00:00Z' }), now);
    expect([usageWords(held), dot(held)]).toEqual(['Full', 'is-over']);
    const heldCursor = accountRow(wire('hc', 'cursor', 'Held', { fullUntil: '2026-11-02T21:00:00Z' }), now);
    expect([usageWords(heldCursor), dot(heldCursor)]).toEqual(['Full', 'is-over']);
    const cursor = accountRow(wire('u', 'cursor', 'Personal', { sevenDay: 0.5 }), now);
    expect([percent(cursor), usageWords(cursor), dot(cursor)]).toEqual([50, '50%', '']);
    const fullerCursor = accountRow(wire('uf', 'cursor', 'Fuller', { sevenDay: 0.85 }), now);
    expect([usageWords(fullerCursor), dot(fullerCursor)]).toEqual(['85%', 'is-high']);
    const quiet = accountRow(wire('uq', 'cursor', 'Quiet'), now);
    expect([percent(quiet), usageWords(quiet), dot(quiet)]).toEqual([null, 'No usage', '']);
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
    // By its tool; by its name too once the tool has more than one.
    const { rowTitle, list } = await load();
    expect(rowTitle(x, all)).toBe('Codex');
    expect(rowTitle(p, [p, accountRow(wire('q', 'claude', 'Work'), now)])).toBe('Claude · Personal');
    expect(list(all).every((a) => a.signedIn !== false)).toBe(true);
    // Moved off a chosen one that looks fine (it couldn't run): where it moved to, as the answer says.
    expect(shownAccount(all, 'p', 'x', 'p', 'p')?.id).toBe('x');
    // Auto: where it talked, else what obid says a first turn would use.
    expect(shownAccount(all, null, 'x', 'p')?.id).toBe('x');
    expect(shownAccount(all, null, null, 'p')?.id).toBe('p');
    // Each only while it can run: not a full account while the next turn goes elsewhere.
    expect(shownAccount(all, 'f', 'f', 'p')?.id).toBe('p');
    expect(shownAccount(all, null, 'f', 'o')?.id).toBe('p');
    expect(shownAccount([full, out], null, 'f', 'o')?.id).toBe('f');
    // A conversation's own choice over the setting; `auto` is Auto whatever the setting says.
    expect(chosenOf('x', 'p')).toBe('x');
    expect(chosenOf(null, 'p')).toBe('p');
    expect(chosenOf('auto', 'p')).toBe(null);
    expect(chosenOf(undefined, null)).toBe(null);
  });

  it('CONVERSATION-AGENT-14: Settings offers Auto and every connected account for new conversations', async () => {
    const { accountRow, talkChoices } = await load();
    const x = accountRow(wire('x', 'codex', 'Work'), now);
    const out = accountRow(wire('o', 'claude', 'Out', { signedIn: false }), now);
    expect(talkChoices([x, out])).toEqual([['auto', 'Auto · Whichever has room'], ['x', 'Work · Codex']]);
  });
});
