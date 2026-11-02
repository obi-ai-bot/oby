import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Settings › Agents' Add account (docs/contracts/accounts.md, ACCOUNTS-10 to ACCOUNTS-15): what the
// sheet asks of obid, and what a card shows of an account.

type Answer = { status: number; body?: unknown };
let answers: Record<string, Answer> = {};
let calls: { method: string; path: string; body?: unknown }[] = [];
let opened: string[] = [];
const flush = () => new Promise((r) => setTimeout(r, 0));

async function load() {
  vi.resetModules();
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const a = answers[`${method} ${path}`] ?? { status: 404, body: { error: 'Not found.' } };
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  vi.stubGlobal('window', { app: { obid: { call }, openExternal: (u: string) => opened.push(u) } });
  return { add: await import('../addAccount'), why: await import('../signInWhy'), onboarding: await import('../onboarding'), settings: await import('../SettingsView') };
}

describe('Add account', () => {
  beforeEach(() => { answers = {}; calls = []; opened = []; });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('ACCOUNTS-10: a row per tool with how many it has, and the logins found on this Mac under theirs', async () => {
    const { add } = await load();
    const rows = add.toolRows(
      [{ agent: 'claude' }, { agent: 'claude' }, { agent: 'cursor' }],
      [{ agent: 'codex', id: 'codex', name: 'Codex' }],
    );
    expect(rows.map((r) => [r.agent, r.added, r.found.map((l) => l.id)])).toEqual([
      ['claude', 2, []], ['codex', 0, ['codex']], ['cursor', 1, []],
    ]);
  });

  it('ACCOUNTS-11: picking a tool makes its account with no name and starts its sign-in at once', async () => {
    const { add } = await load();
    answers['POST /v1/accounts'] = { status: 201, body: { id: 'c1', agent: 'codex', name: 'Codex 2' } };
    answers['GET /v1/accounts/c1/sign-in'] = { status: 200, body: { account: 'c1', url: 'https://auth.openai.com/codex/device', userCode: 'ABCD-EFGHI', pasteCode: false } };
    const { account, page } = await add.startAdding('codex');
    expect(account).toBe('c1');
    expect(page.userCode).toBe('ABCD-EFGHI');
    expect(calls[0]).toMatchObject({ method: 'POST', path: '/v1/accounts' });
    expect(calls[0].body).toEqual({ agent: 'codex' });
    expect(opened).toEqual(['https://auth.openai.com/codex/device']);
  });

  it('ACCOUNTS-12: a sign-in refused takes the account it made back off, and says the reason’s sentence', async () => {
    const { add, why } = await load();
    answers['POST /v1/accounts'] = { status: 201, body: { id: 'c1', agent: 'cursor', name: 'Cursor' } };
    answers['GET /v1/accounts/c1/sign-in'] = { status: 503, body: { error: 'raw /Users/me/bin', reason: 'notInstalled' } };
    answers['DELETE /v1/accounts/c1/sign-in'] = { status: 204 };
    answers['DELETE /v1/accounts/c1'] = { status: 204 };
    const failed = await add.startAdding('cursor').catch((e: unknown) => e);
    expect(why.signInWhy('Cursor', failed)).toBe('Cursor isn’t on this Mac.');
    expect(calls.map((c) => `${c.method} ${c.path}`)).toContain('DELETE /v1/accounts/c1');
  });

  it('ACCOUNTS-13: Cancel ends the sign-in and takes the account back off', async () => {
    const { add } = await load();
    answers['DELETE /v1/accounts/c1/sign-in'] = { status: 204 };
    answers['DELETE /v1/accounts/c1'] = { status: 204 };
    await add.stopAdding('c1');
    await flush();
    expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual(['DELETE /v1/accounts/c1/sign-in', 'DELETE /v1/accounts/c1']);
  });

  it('ACCOUNTS-14: an account taken off as another’s login says whose, in the account’s own name', async () => {
    const { add } = await load();
    answers['GET /v1/accounts/c2/sign-in'] = { status: 409, body: { error: 'That\'s Personal again.', reason: 'sameAccount', sameAs: 'c1' } };
    const same = await add.whoseAgain('c2');
    expect(same).toBe('c1');
    expect(add.again([{ id: 'c1', name: 'Personal' }], same!)).toBe('That’s Personal again');
    // Gone for any other reason: nothing to say.
    expect(await add.whoseAgain('gone')).toBeNull();
  });

  it('ACCOUNTS-15: a card shows who it is and a bar for its session, else its week', async () => {
    const { settings } = await load();
    const row = (more: object) => settings.accountRow({ id: 'a', agent: 'claude', name: 'Max', signedIn: true, fiveHour: null, sevenDay: null, sevenDayResets: null, running: 0, ...more }, Date.now());
    const both = row({ fiveHour: 0.2, sevenDay: 0.5, email: 'me@x.dev', org: 'Deskree', plan: 'team' });
    expect(both.who).toBe('me@x.dev · Deskree · Team');
    expect(settings.barOf(both)).toBe(20);
    expect(settings.barOf(row({ sevenDay: 0.5 }))).toBe(50);
    const none = row({});
    expect(none.who).toBeUndefined();
    expect(settings.barOf(none)).toBeNull();
    expect(settings.barOf(row({ agent: 'cursor', email: 'me@x.dev' }))).toBeNull();
  });

  it('ACCOUNTS-16: first run still connects an agent under the name it gives', async () => {
    const { onboarding } = await load();
    answers['POST /v1/accounts'] = { status: 201, body: { id: 'c1', agent: 'cursor', name: 'Personal' } };
    answers['GET /v1/accounts/c1/sign-in'] = { status: 503, body: { error: 'x', reason: 'notInstalled' } };
    await onboarding.signInAgent('cursor');
    await flush();
    expect(calls.find((c) => c.method === 'POST')?.body).toEqual({ agent: 'cursor', name: 'Personal' });
  });
});
