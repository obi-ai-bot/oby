// @vitest-environment happy-dom
import { act } from 'react';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Tetrix } from '../obid';

// Oby offers no Tetrix by name: the first run's Knowledge step and Settings' Connect Tetrix take the
// address of the person's own (docs/contracts/tetrix-url.md, TETRIX-URL-1 to 4).

vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), useLive: () => ({ status: null, repos: [] }), liveState: () => ({ status: null, repos: [] }) }));

// what obid answers for a Tetrix with a preset in its (old, owner-made) config: the app reads none of it
const OFFERED: Tetrix = { connected: false, url: null, preset: null, presets: [{ id: 'team', name: 'Tetrix Preset', url: 'https://offered.example/mcp' }], connectedAt: null, signIn: null, error: null };
type Answer = { status: number; body?: unknown };
let answers: Record<string, Answer> = {};
let sent: { method: string; path: string; body?: unknown }[] = [];
const call = vi.fn(async (method: string, path: string, body?: unknown) => {
  sent.push({ method, path, body });
  const a = answers[`${method} ${path}`] ?? { status: 404, body: { error: 'Not found.' } };
  return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
});

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  (window as unknown as { app: unknown }).app = { obid: { call }, openExternal: vi.fn() };
  localStorage.clear();
  answers = { 'GET /v1/tetrix': { status: 200, body: OFFERED } };
  sent = [];
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

const field = () => document.querySelector<HTMLInputElement>('input[aria-label="Your Tetrix URL"]')!;
const button = (name: string) => Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent === name)!;
async function type(text: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
  await act(async () => { set.call(field(), text); field().dispatchEvent(new Event('input', { bubbles: true })); });
}
const leave = () => act(async () => { field().dispatchEvent(new FocusEvent('focusout', { bubbles: true })); });
const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });

async function firstRun() {
  const { TetrixRow } = await import('../SetupCard');
  await act(async () => root.render(<ul><TetrixRow /></ul>));
}
async function settings(t: Tetrix = OFFERED, said?: string) {
  const { TetrixWindow } = await import('../Connectors');
  const onConnect = vi.fn();
  await act(async () => root.render(<TetrixWindow tetrix={t} said={said} waiting={false} onConnect={onConnect} onCancel={() => {}} onClose={() => {}} />));
  return onConnect;
}

describe('TETRIX-URL-1: Oby offers no Tetrix by name', () => {
  it('tetrix_url_1_first_run_asks_for_your_tetrix_url_and_offers_no_server', async () => {
    await firstRun();
    expect(field()).not.toBeNull();
    expect(field().placeholder).toBe('https://tetrix.example.com/mcp');
    expect(document.querySelector('select, [role="combobox"], [role="listbox"]')).toBeNull();
    expect(host.textContent).not.toContain('Tetrix Preset');
    expect(button('Connect').disabled).toBe(true);
    expect(sent).toEqual([]);
  });

  it('tetrix_url_1_settings_connect_tetrix_asks_for_the_url_too_and_never_lists_obids_presets', async () => {
    await settings();
    expect(document.body.textContent).toContain('Connect Tetrix');
    expect(field().value).toBe('');
    expect(document.querySelector('select, [role="combobox"]')).toBeNull();
    expect(document.body.textContent).not.toContain('Tetrix Preset');
    expect(document.body.textContent).not.toContain('Another server');
    expect(button('Continue in Browser').disabled).toBe(true);
  });

  it('tetrix_url_1_edit_starts_from_the_address_oby_uses_even_when_obid_says_a_preset_named_it', async () => {
    await settings({ ...OFFERED, connected: true, url: 'https://mine.example/mcp', preset: 'team' });
    expect(document.body.textContent).toContain('Edit Tetrix');
    expect(field().value).toBe('https://mine.example/mcp');
    expect(button('Continue in Browser').disabled).toBe(true);
    await type('https://other.example/mcp');
    expect(button('Continue in Browser').disabled).toBe(false);
  });
});

describe('TETRIX-URL-1: an Obysphere’s Knowledge page adds the person’s own Tetrix too', () => {
  it('tetrix_url_1_add_knowledge_source_asks_for_the_url_and_offers_no_org', async () => {
    const { default: AddSourceModal } = await import('../AddSourceModal');
    await act(async () => root.render(<AddSourceModal sphere="s" hasTetrix={false} onClose={() => {}} />));
    await act(async () => button('TetrixYour org’s code, infra and work as one graph').click());
    const input = document.querySelector<HTMLInputElement>('#source-tetrix')!;
    expect(input.placeholder).toBe('https://tetrix.example.com/mcp');
    expect(document.querySelector('select, [role="combobox"]')).toBeNull();
    expect(document.body.textContent).not.toMatch(/deskree/i);
    expect(button('Sign in to Tetrix').disabled).toBe(true);
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
    await act(async () => { set.call(input, 'https://tetrix.example.com/mcp'); input.dispatchEvent(new Event('input', { bubbles: true })); });
    expect(button('Sign in to Tetrix').disabled).toBe(false);
  });
});

describe('TETRIX-URL-2: Connect waits for an address obid would take', () => {
  it('tetrix_url_2_https_with_a_host_is_the_way_in_and_the_rest_says_why', async () => {
    const { tetrixUrl } = await import('../tetrixUrl');
    expect(tetrixUrl('')).toBeNull();
    expect(tetrixUrl('   ')).toBeNull();
    expect(tetrixUrl('https://tetrix.example.com/mcp')).toEqual({ url: 'https://tetrix.example.com/mcp' });
    // trimmed, and written the way obid reads it (a bare host is `https://host/`)
    expect(tetrixUrl('  https://tetrix.example.com ')).toEqual({ url: 'https://tetrix.example.com/' });
    expect(tetrixUrl('http://127.0.0.1:8080/mcp')).toEqual({ url: 'http://127.0.0.1:8080/mcp' });
    expect(tetrixUrl('https://t.example/mcp?org=3F2504E0-4F89-41D3-9A0C-0305E82C3301')).toMatchObject({ url: expect.stringContaining('?org=') });
    for (const bad of ['tetrix.example.com', 'http://tetrix.example.com/mcp', 'ftp://tetrix.example.com', 'https://', 'https://me:pw@tetrix.example.com/mcp', 'https://tetrix.example.com/mcp#x', 'https://tetrix.example.com/mcp?a=1', 'https://tetrix.example.com/mcp?org=abc']) {
      expect(tetrixUrl(bad), bad).toHaveProperty('problem');
    }
  });

  it('tetrix_url_2_the_problem_shows_once_the_field_is_left_and_connect_stays_off', async () => {
    await firstRun();
    await type('http://tetrix.example.com/mcp');
    expect(host.querySelector('[role="alert"]')).toBeNull();
    await leave();
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('Use a secure address, one that starts with https://.');
    expect(button('Connect').disabled).toBe(true);
    await type('https://tetrix.example.com/mcp');
    expect(host.querySelector('[role="alert"]')).toBeNull();
    expect(button('Connect').disabled).toBe(false);
  });
});

describe('TETRIX-URL-3: Connect goes to the address typed, and a refusal says why under the field', () => {
  it('tetrix_url_3_connect_sends_the_typed_address_and_nothing_else', async () => {
    answers['POST /v1/tetrix/sign-in'] = { status: 200, body: { url: 'https://auth.example/authorize', server: 'https://auth.example', expiresAt: '2026-11-17T12:00:00Z' } };
    answers['GET /v1/tetrix'] = { status: 200, body: { ...OFFERED, connected: true, url: 'https://tetrix.example.com/mcp' } };
    await firstRun();
    await type(' https://tetrix.example.com/mcp ');
    await act(async () => button('Connect').click());
    await flush();
    const signIn = sent.find((c) => c.method === 'POST' && c.path === '/v1/tetrix/sign-in');
    expect(signIn?.body).toEqual({ url: 'https://tetrix.example.com/mcp' });
  });

  it('tetrix_url_3_enter_connects_too', async () => {
    answers['POST /v1/tetrix/sign-in'] = { status: 502, body: { error: 'raw /Users/me', reason: 'noSignIn' } };
    await firstRun();
    await type('https://tetrix.example.com/mcp');
    await act(async () => { field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); });
    await flush();
    expect(sent.some((c) => c.method === 'POST' && c.path === '/v1/tetrix/sign-in')).toBe(true);
  });

  it('tetrix_url_3_obids_refusal_is_its_reasons_sentence_under_the_field_never_its_error', async () => {
    answers['POST /v1/tetrix/sign-in'] = { status: 502, body: { error: 'obid can’t reach /Users/me/x: os error 2', reason: 'noSignIn' } };
    await firstRun();
    await type('https://tetrix.example.com/mcp');
    await act(async () => button('Connect').click());
    await flush();
    const said = host.querySelector('[role="alert"]')?.textContent;
    expect(said).toBe('That Tetrix doesn’t say where to sign in.');
    expect(host.textContent).not.toMatch(/\/Users|os error/);
    expect(button('Connect').disabled).toBe(false);
  });

  it('tetrix_url_3_settings_shows_the_same_sentence_under_its_field', async () => {
    const onConnect = await settings(OFFERED, 'That isn’t a Tetrix address Oby can use.');
    expect(document.querySelector('[role="alert"]')?.textContent).toBe('That isn’t a Tetrix address Oby can use.');
    await type('https://tetrix.example.com/mcp');
    await act(async () => button('Continue in Browser').click());
    expect(onConnect).toHaveBeenCalledWith({ url: 'https://tetrix.example.com/mcp' });
  });
});

describe('TETRIX-URL-4: no Tetrix is built in', () => {
  const SRC = join(__dirname, '..', '..', '..');
  const files = (dir: string): string[] => readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === 'node_modules' || f === 'test' ? [] : files(p);
    return /\.(ts|tsx|css|html)$/.test(f) ? [p] : [];
  });

  it('tetrix_url_4_no_renderer_main_or_shared_file_names_a_preset_instance', () => {
    const offending: string[] = [];
    for (const f of files(SRC)) {
      const text = readFileSync(f, 'utf8');
      // a preset's name or a real instance's host: Oby has none to offer (a person's own is typed)
      if (/dogfood/i.test(text) || /mcp\.tetrix\.dev/.test(text) || /tetrix[\w.-]*\.(deskree|run\.app|nip\.io)/i.test(text)) offending.push(f.replace(SRC, ''));
    }
    expect(offending).toEqual([]);
  });

  it('tetrix_url_4_the_app_reads_no_preset_from_obids_answer', () => {
    const readers = files(join(SRC, 'renderer')).filter((f) => /\.(presets|preset)\b/.test(readFileSync(f, 'utf8')) && /tetrix/i.test(readFileSync(f, 'utf8')));
    expect(readers.map((f) => f.replace(SRC, ''))).toEqual([]);
  });
});
