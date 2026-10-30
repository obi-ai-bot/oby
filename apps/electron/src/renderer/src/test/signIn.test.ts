import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// First run's rows when a sign-in doesn't take (docs/contracts/sign-in.md, SIGN-IN-10 to
// SIGN-IN-12): the sentence under the row comes from obid's `reason`, never its `error`.

type Answer = { status: number; body?: unknown };
let answers: Record<string, Answer> = {};
const flush = () => new Promise((r) => setTimeout(r, 0));

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
  const call = async (method: string, path: string) => {
    const a = answers[`${method} ${path}`] ?? { status: 404, body: { error: 'Not found.' } };
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  vi.stubGlobal('window', { app: { obid: { call }, openExternal: () => {} } });
  return { onboarding: await import('../onboarding'), why: await import('../signInWhy'), obid: await import('../obid') };
}
// obid's own error, as raw as it gets: a folder, a code, what a CLI said
const RAW = 'obid can’t find /Users/me/.local/bin/cursor-agent: ABCD-1234 sk-ant-oat01-xyz (os error 2)';

describe('sign-in errors under first run’s rows', () => {
  beforeEach(() => { answers = {}; });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('SIGN-IN-10: an agent’s refused sign-in shows its reason’s sentence, never obid’s error', async () => {
    const { onboarding } = await load();
    answers['POST /v1/accounts'] = { status: 201, body: { id: 'c1', agent: 'cursor', name: 'Personal' } };
    answers['GET /v1/accounts/c1/sign-in'] = { status: 503, body: { error: RAW, reason: 'notInstalled' } };
    await onboarding.signInAgent('cursor');
    const s = onboarding.dev!.get();
    expect(s.agents.cursor).toBe('out');
    expect(s.problems.cursor).toBe('Cursor isn’t on this Mac.');

    answers['POST /v1/github/sign-in'] = { status: 502, body: { error: RAW, reason: 'didNotStart' } };
    await onboarding.signInGitHub();
    expect(onboarding.dev!.get().problems.github).toBe('GitHub’s sign-in didn’t start. Try again.');

    answers['POST /v1/tetrix/sign-in'] = { status: 422, body: { error: RAW, reason: 'badAddress' } };
    await onboarding.connectTetrix({ url: 'http://tetrix.example/mcp' });
    expect(onboarding.dev!.get().problems.tetrix).toBe('That isn’t a Tetrix address Oby can use.');
    for (const why of Object.values(onboarding.dev!.get().problems)) expect(why).not.toMatch(/\/Users|ABCD|sk-ant|os error/);
  });

  it('SIGN-IN-11: no reason, one the app doesn’t know, or obid not answering: the plain sentence', async () => {
    const { why, obid } = await load();
    expect(why.signInWhy('Codex', new obid.ApiError(500, RAW))).toBe('Couldn’t connect Codex. Try again.');
    expect(why.signInWhy('Codex', new obid.ApiError(409, RAW, 'somethingNew'))).toBe('Couldn’t connect Codex. Try again.');
    expect(why.signInWhy('Codex', new obid.ApiError(0, RAW))).toBe('Couldn’t connect Codex. Try again.');
    expect(why.signInWhy('Codex', new Error(RAW))).toBe('Couldn’t connect Codex. Try again.');
  });

  it('SIGN-IN-12: GitHub’s or Tetrix’s sign-in that ended without signing in puts Connect back with why', async () => {
    const { onboarding } = await load();
    answers['POST /v1/github/sign-in'] = { status: 200, body: { url: 'https://github.com/login/device', userCode: 'AB12-CD34' } };
    answers['GET /v1/github'] = { status: 200, body: { gh: true, signedIn: false, user: null, signIn: null } };
    await onboarding.signInGitHub();
    expect(onboarding.dev!.get().github).toBe('out');
    expect(onboarding.dev!.get().problems.github).toBe('Didn’t finish signing in. Try again.');

    answers['POST /v1/tetrix/sign-in'] = { status: 200, body: { url: 'https://auth.example/authorize' } };
    answers['GET /v1/tetrix'] = { status: 200, body: { connected: false, signIn: null, error: RAW } };
    await onboarding.connectTetrix({ url: 'https://tetrix.example/mcp' });
    await flush();
    expect(onboarding.dev!.get().tetrix).toBe('out');
    expect(onboarding.dev!.get().problems.tetrix).toBe('Didn’t finish signing in. Try again.');

    // the next try clears it while under way
    answers['GET /v1/github'] = { status: 200, body: { gh: true, signedIn: true, user: 'octo-cat', signIn: null } };
    await onboarding.signInGitHub();
    expect(onboarding.dev!.get().github).toBe('in');
    expect(onboarding.dev!.get().problems.github).toBeUndefined();
  });
});
