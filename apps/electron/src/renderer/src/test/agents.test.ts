import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Settings › Agents' choices kept on this Mac before obid kept them (docs/contracts/agents.md,
// AGENTS-7): moved to obid once, then forgotten here, so nobody's choice is lost.

type Answer = { status: number; body?: unknown };
const calls: { method: string; path: string; body?: unknown }[] = [];
let answers: Record<string, Answer> = {};

function memory(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); },
  };
}

async function load(kept: Record<string, unknown>) {
  vi.resetModules();
  calls.length = 0;
  vi.stubGlobal('location', { search: '' });
  const storage = memory();
  for (const [k, v] of Object.entries(kept)) storage.setItem(`oby.setting.${k}`, JSON.stringify(v));
  vi.stubGlobal('localStorage', storage);
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const a = answers[`${method} ${path}`] ?? { status: 404, body: { error: 'Not found.' } };
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  vi.stubGlobal('window', { app: { obid: { call } } });
  const settings = await import('../SettingsView');
  const left = (k: string) => JSON.parse(storage.getItem(`oby.setting.${k}`) ?? 'null');
  return { settings, left };
}

const account = (id: string) => ({ id, agent: 'claude', name: id, signedIn: true, fiveHour: null, sevenDay: null, sevenDayResets: null, running: 0 });
const status = (more: Record<string, unknown> = {}) =>
  ({ runner: { version: '0.2.0', host: 'mac' }, accounts: [account('a1'), account('a2')], models: {}, maxRunning: 6, running: 0, ...more }) as never;

describe('Settings › Agents on obid', () => {
  beforeEach(() => { answers = {}; });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('AGENTS-7: the default and each account\'s model kept here move to obid once, then are forgotten here', async () => {
    const { settings, left } = await load({
      'agent.default': 'a2',
      'models.mine': { a2: { model: 'claude-sonnet-5-5', effort: 'high' }, gone: { model: 'claude-haiku-5-5' } },
    });
    answers['PUT /v1/settings/jobs'] = { status: 200, body: { defaultAccount: 'a2' } };
    answers['PATCH /v1/accounts/a2'] = { status: 200, body: account('a2') };
    expect(await settings.moveKeptChoices(status({ defaultAccount: null }))).toBe(true);
    expect(calls).toEqual([
      { method: 'PUT', path: '/v1/settings/jobs', body: { defaultAccount: 'a2' } },
      { method: 'PATCH', path: '/v1/accounts/a2', body: { model: 'claude-sonnet-5-5', effort: 'high' } },
    ]);
    expect(left('agent.default')).toBe(null);
    expect(left('models.mine')).toEqual({});
  });

  it('AGENTS-7: an older obid (no defaultAccount in its status) gets nothing, and the choices stay here', async () => {
    const { settings, left } = await load({ 'agent.default': 'a2', 'models.mine': { a2: { model: 'claude-sonnet-5-5' } } });
    expect(await settings.moveKeptChoices(status())).toBe(false);
    expect(calls).toEqual([]);
    expect(left('agent.default')).toBe('a2');
    expect(left('models.mine')).toEqual({ a2: { model: 'claude-sonnet-5-5' } });
  });

  it('AGENTS-7: a default obid has already wins, and a choice obid refused stays to be tried again', async () => {
    const { settings, left } = await load({ 'agent.default': 'a2', 'models.mine': { a1: { effort: 'ultra' } } });
    answers['PATCH /v1/accounts/a1'] = { status: 422, body: { error: 'effort is one of low, medium, high, xhigh, max, or null.' } };
    expect(await settings.moveKeptChoices(status({ defaultAccount: 'a1' }))).toBe(false);
    expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual(['PATCH /v1/accounts/a1']);
    expect(left('agent.default')).toBe(null);
    expect(left('models.mine')).toEqual({ a1: { effort: 'ultra' } });
  });
});
