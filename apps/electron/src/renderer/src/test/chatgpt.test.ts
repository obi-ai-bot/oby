import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Continue with ChatGPT in the app (docs/contracts/chatgpt-sign-in.md): what it asks of obid and what
// it shows. obid holds the tokens; nothing here ever sees one.

type Answer = { status: number; body?: unknown };
let answers: Record<string, Answer[]> = {};
let calls: { method: string; path: string; body?: unknown }[] = [];
let opened: string[] = [];

const say = (key: string, ...more: Answer[]) => { answers[key] = more; };

function memory(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); },
  };
}

async function load() {
  vi.resetModules();
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', memory());
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const list = answers[`${method} ${path}`];
    const a = (list && list.length > 1 ? list.shift() : list?.[0]) ?? { status: 404, body: { error: 'Not found.' } };
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  vi.stubGlobal('window', { app: { obid: { call }, openExternal: (u: string) => opened.push(u) } });
  return {
    onboarding: await import('../onboarding'),
    chatgpt: await import('../chatgpt'),
    live: await import('../live'),
    settings: await import('../SettingsView'),
  };
}
const page = { account: 'c2', url: 'https://auth.openai.com/page', userCode: null, pasteCode: false, expiresAt: '2030-01-01T00:00:00Z' };
const row = (more: Record<string, unknown>) => ({ id: 'c2', agent: 'codex', name: 'Personal', signedIn: false, fiveHour: null, sevenDay: null, sevenDayResets: null, running: 0, ...more });

describe('Continue with ChatGPT', () => {
  beforeEach(() => { answers = {}; calls = []; opened = []; });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('CHATGPT-1: no login on this Mac: one click opens OpenAI’s page, no code to type', async () => {
    const { onboarding } = await load();
    say('POST /v1/accounts', { status: 201, body: { id: 'c2', agent: 'codex', name: 'Personal' } });
    say('GET /v1/accounts/c2/sign-in?with=chatgpt', { status: 200, body: page });
    say('GET /v1/status', { status: 200, body: { accounts: [row({ signedIn: true, login: 'chatgpt' })] } });
    await onboarding.signInAgent('codex', true);
    expect(opened).toEqual([page.url]);
    expect(calls.some((c) => c.path === '/v1/accounts/c2/sign-in')).toBe(false);
    const s = onboarding.dev!.get();
    expect(s.problems.codex).toBeUndefined();
    expect(s.codes.codex).toBeUndefined();
    // it stays connected: no DELETE of the account it made
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false);
  });

  it('CHATGPT-1: the Mac’s own login’s account answers ownLogin, so a new one signs in', async () => {
    const { onboarding } = await load();
    say('POST /v1/accounts', { status: 201, body: { id: 'codex', agent: 'codex', name: 'Codex' } }, { status: 201, body: { id: 'c2', agent: 'codex', name: 'Personal' } });
    say('GET /v1/accounts/codex/sign-in?with=chatgpt', { status: 409, body: { error: 'its own login', reason: 'ownLogin' } });
    say('GET /v1/accounts/c2/sign-in?with=chatgpt', { status: 200, body: page });
    say('GET /v1/status', { status: 200, body: { accounts: [row({ signedIn: true, login: 'chatgpt' })] } });
    await onboarding.signInAgent('codex', true);
    expect(opened).toEqual([page.url]);
    expect(calls.filter((c) => c.method === 'POST' && c.path === '/v1/accounts').length).toBe(2);
  });

  it('CHATGPT-2: with a login on this Mac, adopted or found, the button doesn’t show', async () => {
    const { chatgpt } = await load();
    const none = { accounts: [] };
    expect(chatgpt.offersChatGpt(none as never)).toBe(true);
    expect(chatgpt.offersChatGpt({ accounts: [row({ id: 'codex' })] } as never), 'the seeded account, no login').toBe(true);
    expect(chatgpt.offersChatGpt({ accounts: [row({ signedIn: true })] } as never), 'signed in').toBe(false);
    expect(chatgpt.offersChatGpt({ accounts: [], availableLogins: [{ agent: 'codex', id: 'codex', name: 'Codex' }] } as never), 'found, removed here').toBe(false);
    expect(chatgpt.offersChatGpt({ accounts: [row({ id: 'codex', label: 'Run codex once' })] } as never), 'there but ending').toBe(false);
    expect(chatgpt.offersChatGpt({ accounts: [row({ login: 'chatgpt', label: 'Continue with ChatGPT again' })] } as never), 'a ChatGPT login that ended').toBe(true);
  });

  it('CHATGPT-3: without chatgpt it’s the device login, its code to type', async () => {
    const { onboarding } = await load();
    say('POST /v1/accounts', { status: 201, body: { id: 'c2', agent: 'codex', name: 'Personal' } });
    say('GET /v1/accounts/c2/sign-in', { status: 200, body: { ...page, userCode: 'ABCD-1234' } });
    const going = onboarding.signInAgent('codex');
    await vi.waitFor(() => expect(opened.length).toBe(1));
    expect(onboarding.dev!.get().codes.codex?.userCode).toBe('ABCD-1234');
    expect(calls.some((c) => c.path.includes('with=chatgpt'))).toBe(false);
    await onboarding.cancelAgent('codex');
    void going;
  });

  it('CHATGPT-4: a sign-in that didn’t take says why within one look, and the account it made goes', async () => {
    const { onboarding } = await load();
    say('POST /v1/accounts', { status: 201, body: { id: 'c2', agent: 'codex', name: 'Personal' } });
    say('GET /v1/accounts/c2/sign-in?with=chatgpt', { status: 200, body: page });
    say('GET /v1/status', { status: 200, body: { accounts: [row({ label: 'ChatGPT’s identity couldn’t be verified. Nothing was kept.' })] } });
    say('DELETE /v1/accounts/c2/sign-in', { status: 204 });
    say('DELETE /v1/accounts/c2', { status: 204 });
    await onboarding.signInAgent('codex', true);
    const s = onboarding.dev!.get();
    expect(s.problems.codex).toContain('couldn’t be verified');
    expect(s.agents.codex).toBe('out');
    expect(calls.map((c) => `${c.method} ${c.path}`)).toContain('DELETE /v1/accounts/c2');
  });

  it('CHATGPT-6: a sign-in kept with plan use off keeps its account and says so', async () => {
    const { onboarding } = await load();
    say('POST /v1/accounts', { status: 201, body: { id: 'c2', agent: 'codex', name: 'Personal' } });
    say('GET /v1/accounts/c2/sign-in?with=chatgpt', { status: 200, body: page });
    say('GET /v1/status', { status: 200, body: { accounts: [row({ login: 'chatgpt', label: 'ChatGPT plan use is off' })] } });
    await onboarding.signInAgent('codex', true);
    expect(onboarding.dev!.get().problems.codex).toBe('ChatGPT plan use is off');
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false);
  });

  it('CHATGPT-13: the limit’s card has OpenAI’s words, Manage usage first, and no reset time', async () => {
    const { live, chatgpt } = await load();
    const onPlan = { accounts: [row({ signedIn: true, login: 'chatgpt', fullUntil: '2030-01-01T00:00:00Z' })] };
    const decision = { id: 'd1', revision: 1, account: 'c2', agent: 'codex', resetAt: null, jobs: [{ id: 'j1', title: 'Fix it' }],
      options: [{ kind: 'resume' }, { kind: 'stop' }] };
    const state = { ...live.liveState(), status: onPlan, limits: [decision], answering: new Set<string>() } as never;
    const [card] = live.limitAsks(state);
    expect(card.ask!.headline).toBe('Usage limit reached');
    expect(card.ask!.options[0]).toBe('Manage usage');
    expect(card.ask!.options).toContain('Resume when available');
    expect(card.ask!.text).toContain('Review your plan or this app’s limit in ChatGPT settings.');
    expect(card.ask!.text).not.toMatch(/resets|reset time/i);
    // It opens OpenAI's settings, and answers nothing.
    await live.answerJob(card.id, 'Manage usage');
    expect(opened).toEqual([chatgpt.USAGE_URL]);
    expect(calls.filter((c) => c.method !== 'GET')).toEqual([]);
    // Another account's card is as it was.
    const onLogin = { accounts: [row({ signedIn: true })] };
    const [other] = live.limitAsks({ ...live.liveState(), status: onLogin, limits: [decision], answering: new Set<string>() } as never);
    expect(other.ask!.headline).toContain('reached its limit');
    expect(other.ask!.options[0]).toBe('Resume when available');
  });

  it('a ChatGPT row shows no use reported, and at its limit no time', async () => {
    const { settings } = await load();
    const now = Date.parse('2030-01-01T00:00:00Z');
    const account = row({ signedIn: true, login: 'chatgpt' }) as never;
    const shown = settings.accountRow(account, now);
    expect(shown.chatgpt).toBe(true);
    expect(shown.resets).toBe('No use reported yet');
    expect(settings.accountRow(row({ signedIn: true, login: 'chatgpt', fullUntil: '2030-01-01T01:00:00Z' }) as never, now).resets).toBe('Usage limit reached');
    expect(settings.accountRow(row({ login: 'chatgpt', label: 'Continue with ChatGPT again' }) as never, now).resets).toBe('Continue with ChatGPT again');
    expect(settings.accountRow(row({ signedIn: true }) as never, now).chatgpt).toBe(false);
  });

  it('CHATGPT-17: the welcome shows once, obid’s word on it, even with localStorage emptied', async () => {
    const { chatgpt } = await load();
    const signed = { accounts: [row({ signedIn: true, login: 'chatgpt' })] } as never;
    say('GET /v1/settings/chatgpt', { status: 200, body: { welcomed: false } });
    expect(await chatgpt.welcomeDue(signed)).toBe(true);
    expect(await chatgpt.welcomeDue({ accounts: [row({ signedIn: true })] } as never), 'a device login').toBe(false);
    expect(await chatgpt.welcomeDue(null)).toBe(false);
    say('PUT /v1/settings/chatgpt', { status: 200, body: { welcomed: true } });
    await chatgpt.welcomed();
    expect(calls.at(-1)).toEqual({ method: 'PUT', path: '/v1/settings/chatgpt', body: { welcomed: true } });
    say('GET /v1/settings/chatgpt', { status: 200, body: { welcomed: true } });
    localStorage.clear();
    expect(await chatgpt.welcomeDue(signed)).toBe(false);
  });
});
