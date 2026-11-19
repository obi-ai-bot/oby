// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Chat } from '../chats';
import { liveBridge } from './liveBridge';

// The owner's skills in the message box (docs/contracts/composer.md, COMPOSER-25 to 27;
// docs/api/skills.md): `/` lists the skills obid gives for the box, one picked is a chip before the
// words, and it goes beside them, never in them. The real Composer on live.ts, main's bridge
// (window.app.obid.call) stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
// the box's words: a plain field holding them, telling the box what the real editor tells it: each
// change, the list's keys (onMentionKey), Enter, and Backspace with the cursor at the very start
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, onEnter, onMentionKey, onBackAtStart, handle }: {
    label: string; onText: (t: string) => void; onEnter: () => void; onMentionKey?: (k: string) => boolean; onBackAtStart?: () => boolean; handle: React.Ref<unknown>;
  }) {
    const field = useRef<HTMLTextAreaElement>(null);
    const put = (t: string) => { if (field.current) field.current.value = t; onText(t); };
    useImperativeHandle(handle, () => ({
      text: () => field.current?.value ?? '', clear: () => put(''), setText: put, focus: () => {}, blur: () => {}, mention: () => {}, quote: () => {},
    }));
    const keys: Record<string, string> = { ArrowUp: 'up', ArrowDown: 'down', Tab: 'pick', Enter: 'pick', Escape: 'close' };
    return <textarea ref={field} aria-label={label} data-box="" onChange={(e) => onText(e.currentTarget.value)} onKeyDown={(e) => {
      const f = e.currentTarget;
      if (e.key === 'Backspace') { if (f.selectionStart === 0 && f.selectionEnd === 0 && onBackAtStart?.()) e.preventDefault(); return; }
      if (keys[e.key] && onMentionKey?.(keys[e.key])) { e.preventDefault(); return; }
      if (e.key === 'Enter') { e.preventDefault(); onEnter(); }
    }} />;
  },
}));

let root: Root | null = null;
vi.setConfig({ testTimeout: 120_000 });
beforeAll(async () => { await import('../Composer'); await import('../live'); }, 300_000);
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
});

type Call = { method: string; path: string; body?: unknown };
const at = '2026-11-17T10:00:00Z';
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities });
const convo = { id: 'c1', title: 'Skills', pinned: false, created: at, lastActivity: at, cursor: '', folders: [] };
const SKILLS = [
  { name: 'write-tests', description: 'Writes the tests for what you name', userInvocable: true, source: 'user' },
  { name: 'review:check-diff', description: 'Reads the diff, tests included, and says what’s risky', userInvocable: true, source: 'plugin', plugin: 'review' },
  { name: 'testing:run', description: 'Runs the suite', userInvocable: true, source: 'project' },
  // only the agent starts it: never offered
  { name: 'test-internals', description: 'For the agent alone', userInvocable: false, source: 'user' },
];
const WHY = 'This task isn’t at work, so its agent can’t take a skill now.';
const ALL = ['composer-v1', 'folders-v1', 'skills-v1'];

function obid(answers: Record<string, unknown> = {}) {
  const calls: Call[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'GET' && path in answers) return { status: 200, text: JSON.stringify(answers[path]) };
    if (method === 'GET' && path.startsWith('/v1/skills')) return { status: 200, text: JSON.stringify({ tool: 'claude', account: 'p', skills: SKILLS }) };
    if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  return { app: { obid: { call, ...bridge.obid }, onFullScreen: () => () => {} }, calls, bridge };
}

type Props = Partial<Parameters<typeof import('../Composer').default>[0]>;
// a conversation's box, unless props say another's
async function box(props: Props = {}, capabilities = ALL, answers: Record<string, unknown> = {}) {
  const o = obid(answers);
  // (the modules beforeAll warmed up were imported with no obid: the app's are imported again on this one)
  vi.resetModules();
  (window as { app?: unknown }).app = o.app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const { default: Composer } = await import('../Composer');
  const live = await import('../live');
  live.connect();
  o.bridge.sync({ status: STATUS(capabilities), conversations: [convo] });
  const talk: Chat = { id: 'c1', title: 'Skills', said: [], at: 0 };
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Composer label="Reply to Oby" talk={talk} conversation="c1" draft="conversation:c1" {...props} />));
  return o;
}
const field = () => document.querySelector<HTMLTextAreaElement>('[data-box]')!;
const type = (text: string, caret = text.length) => act(async () => {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field(), text);
  field().setSelectionRange(caret, caret);
  field().dispatchEvent(new Event('input', { bubbles: true }));
});
const press = (key: string) => act(async () => { field().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })); });
const send = () => act(async () => { document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
// the rows `/` lists, once obid answered
const rows = () => [...document.querySelectorAll('[aria-label="Skills"] [role="option"]')];
const names = () => rows().map((r) => r.firstElementChild!.firstChild!.textContent);
const listed = () => vi.waitFor(() => expect(rows().length).toBeGreaterThan(0));
const chip = () => document.querySelector('.composer-skill .composer-tag-text')?.textContent ?? null;
const skillAsks = (o: ReturnType<typeof obid>) => o.calls.filter((c) => c.path.startsWith('/v1/skills')).map((c) => c.path);

describe('COMPOSER-25: / lists the skills obid gives for the box', () => {
  it('composer_25_slash_lists_the_skills_obid_gives_for_the_box', async () => {
    const o = await box();
    await type('/');
    await listed();
    expect(skillAsks(o)).toEqual(['/v1/skills?conversation=c1']);
    // only those the owner may start, each with its description
    expect(names()).toEqual(['/write-tests', '/review:check-diff', '/testing:run']);
    expect(rows()[0].querySelector('.add-item-about')?.textContent).toBe('Writes the tests for what you name');
    // what's typed after the / filters them: names starting with it, then names holding it, then descriptions
    await type('/TEST');
    expect(names()).toEqual(['/testing:run', '/write-tests', '/review:check-diff']);
    await type('/diff');
    expect(names()).toEqual(['/review:check-diff']);
    // a fresh ask each time it opens
    await type('/diff it');
    expect(rows()).toEqual([]);
    await type('/');
    await listed();
    expect(skillAsks(o)).toEqual(['/v1/skills?conversation=c1', '/v1/skills?conversation=c1']);
  });

  it('composer_25_each_box_asks_for_its_own_skills', async () => {
    // Home's box, with no conversation yet: a new conversation's
    let o = await box({ talk: true, conversation: undefined, draft: 'home' });
    await type('/');
    await listed();
    expect(skillAsks(o)).toEqual(['/v1/skills']);
    await act(async () => root!.unmount());
    root = null;
    vi.resetModules();
    // a task's box: the task's
    o = await box({ talk: undefined, conversation: undefined, skills: { job: 'j1' }, draft: 'task:j1' });
    await type('/');
    await listed();
    expect(skillAsks(o)).toEqual(['/v1/skills?job=j1']);
  });

  it('composer_25_with_none_to_list_the_panel_says_why', async () => {
    await box({ talk: undefined, conversation: undefined, skills: { job: 'j1' }, draft: 'task:j1' }, ALL, { '/v1/skills?job=j1': { tool: 'claude', account: 'p', skills: [], why: WHY } });
    await type('/');
    await vi.waitFor(() => expect(document.querySelector('.slash-panel')?.textContent).toBe(WHY));
    expect(rows()).toEqual([]);
  });

  it('composer_25_with_none_and_no_why_it_says_what_is_missing', async () => {
    await box({}, ALL, { '/v1/skills?conversation=c1': { tool: 'codex', account: 'p', skills: [SKILLS[3]] } });
    await type('/');
    await vi.waitFor(() => expect(document.querySelector('.slash-panel')?.textContent).toContain('Codex’s own commands can’t be listed here yet'));
  });

  it('composer_25_keys_move_pick_and_close_the_list', async () => {
    await box();
    await type('/');
    await listed();
    const lit = () => rows().findIndex((r) => r.getAttribute('aria-selected') === 'true');
    expect(lit()).toBe(0);
    await press('ArrowDown');
    expect(lit()).toBe(1);
    await press('ArrowUp');
    await press('ArrowUp');
    expect(lit()).toBe(2);
    // Esc puts it away until the / goes
    await press('Escape');
    expect(rows()).toEqual([]);
    await type('');
    await type('/w');
    await listed();
    await press('Tab');
    expect(chip()).toBe('/write-tests');
  });

  it('composer_25_an_obid_without_skills_v1_lists_none', async () => {
    const o = await box({}, ['composer-v1', 'folders-v1']);
    await type('/rev');
    expect(document.querySelector('.slash-panel')?.textContent).toContain('Claude Code’s own commands can’t be listed here yet');
    expect(skillAsks(o)).toEqual([]);
    // and a task's box shows nothing, as before
    await act(async () => root!.unmount());
    root = null;
    vi.resetModules();
    const t = await box({ talk: undefined, conversation: undefined, skills: { job: 'j1' }, draft: 'task:j1' }, ['composer-v1']);
    await type('/');
    expect(document.querySelector('.slash-panel, [aria-label="Skills"]')).toBeNull();
    expect(skillAsks(t)).toEqual([]);
  });
});

describe('COMPOSER-26: a skill picked is a chip at the start of the box', () => {
  it('composer_26_a_picked_skill_is_a_chip_before_the_words', async () => {
    await box();
    await type('/rev');
    await listed();
    await press('Enter');
    // the /words went; the chip is the box's first tag
    expect(field().value).toBe('');
    expect(chip()).toBe('/review:check-diff');
    expect(document.querySelector('.composer-tags')!.firstElementChild!.classList.contains('composer-skill')).toBe(true);
    // one at a time: another picked replaces it
    await type('/write');
    await listed();
    await act(async () => { (rows()[0] as HTMLElement).click(); });
    expect(chip()).toBe('/write-tests');
    expect(document.querySelectorAll('.composer-skill')).toHaveLength(1);
  });

  it('composer_26_backspace_at_the_start_takes_the_chip_out', async () => {
    await box();
    await type('/w');
    await listed();
    await press('Enter');
    // the cursor after a word: Backspace is the word's
    await type('fix it');
    await press('Backspace');
    expect(chip()).toBe('/write-tests');
    // at the very start: the chip's
    await type('fix it', 0);
    await press('Backspace');
    expect(chip()).toBeNull();
    expect(field().value).toBe('fix it');
  });

  it('composer_26_its_x_takes_it_out', async () => {
    await box();
    await type('/w');
    await listed();
    await press('Enter');
    await act(async () => { document.querySelector<HTMLButtonElement>('[aria-label="Remove /write-tests"]')!.click(); });
    // (it leaves as a tag leaves, then it's gone)
    await vi.waitFor(() => expect(chip()).toBeNull());
  });
});

describe('COMPOSER-27: the skill goes beside the words', () => {
  it('composer_27_the_picked_skill_goes_beside_the_words', async () => {
    const onSend = vi.fn();
    await box({ onSend });
    await type('/w');
    await listed();
    await press('Enter');
    await type('the login page');
    await send();
    expect(onSend.mock.calls[0][0]).toBe('the login page');
    expect(onSend.mock.calls[0][1].skill).toBe('write-tests');
    // for those words only
    expect(chip()).toBeNull();
    await type('and the footer');
    await send();
    expect(onSend.mock.calls[1][1]).not.toHaveProperty('skill');
  });

  it('composer_27_a_skill_alone_is_sent', async () => {
    const onSend = vi.fn();
    await box({ onSend });
    // an empty box: nothing to send
    expect(document.querySelector<HTMLButtonElement>('[aria-label="Send"]')!.disabled).toBe(true);
    await type('/w');
    await listed();
    await press('Enter');
    // the chip alone: send is lit, and Enter sends it
    expect(field().value).toBe('');
    expect(document.querySelector<HTMLButtonElement>('[aria-label="Send"]')!.disabled).toBe(false);
    await press('Enter');
    expect(onSend).toHaveBeenCalledTimes(1);
    expect(onSend.mock.calls[0][0]).toBe('');
    expect(onSend.mock.calls[0][1].skill).toBe('write-tests');
    expect(chip()).toBeNull();
  });

  it('composer_27_words_typed_as_slash_name_carry_no_skill', async () => {
    const onSend = vi.fn();
    await box({ onSend });
    await type('/write-tests the login page');
    await send();
    expect(onSend.mock.calls[0][0]).toBe('/write-tests the login page');
    expect(onSend.mock.calls[0][1]).not.toHaveProperty('skill');
  });
});

// obid's side of the wire: what live.ts sends, and what main signs
describe('COMPOSER-27: obid gets `skill` on POST /v1/plans and POST /v1/jobs/{id}/messages', () => {
  async function connected(capabilities: string[]) {
    const calls: Call[] = [];
    const call = async (method: string, path: string, body?: unknown) => {
      calls.push({ method, path, body });
      if (path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'reply', text: 'On it.', jobs: [], conversation: 'c1' }) };
      if (path === '/v1/jobs/j1/messages') return { status: 200, text: JSON.stringify({ id: 'j1', status: 'running', title: 'Login', agent: 'claude', repo: null, createdAt: at, updatedAt: at }) };
      if (path.startsWith('/v1/conversation?')) return { status: 200, text: '[]' };
      return { status: 404, text: '{"error":"Not found."}' };
    };
    const bridge = liveBridge(call);
    vi.resetModules();
    (window as { app?: unknown }).app = { obid: { call, ...bridge.obid } };
    vi.stubGlobal('location', { search: '' });
    const live = await import('../live');
    live.connect();
    bridge.sync({ status: STATUS(capabilities), conversations: [convo] });
    return { live, calls };
  }
  const bodyOf = (calls: Call[], path: string) => vi.waitFor(() => { const c = calls.find((x) => x.path === path); expect(c).toBeTruthy(); return c!.body as Record<string, unknown>; });

  it('composer_27_obid_gets_the_skill_beside_the_words', async () => {
    const { live, calls } = await connected(ALL);
    live.ask('the login page', 'c1', undefined, undefined, { skill: 'write-tests' });
    const plan = await bodyOf(calls, '/v1/plans');
    expect(plan).toMatchObject({ said: 'the login page', conversation: 'c1', skill: 'write-tests' });
    await live.tellJob('j1', 'and the footer', [], true, 'review:check-diff');
    expect(await bodyOf(calls, '/v1/jobs/j1/messages')).toMatchObject({ text: 'and the footer', skill: 'review:check-diff' });
    // main signs them: the skill with the body, never one of the words
    const { wordsOf } = await import('../../../shared/words');
    expect(wordsOf('POST', '/v1/plans', plan)).toEqual(['the login page']);
    expect(wordsOf('POST', '/v1/jobs/j1/messages', { text: 'and the footer', conversation: true, skill: 'write-tests' })).toEqual(['and the footer']);
  });

  it('composer_27_a_skill_alone_is_sent_to_obid', async () => {
    const { live, calls } = await connected(ALL);
    expect(live.ask('', 'c1', undefined, undefined, { skill: 'write-tests' })).toBe('c1');
    expect(await bodyOf(calls, '/v1/plans')).toMatchObject({ said: '', conversation: 'c1', skill: 'write-tests' });
    await live.tellJob('j1', '', [], true, 'review:check-diff');
    expect(await bodyOf(calls, '/v1/jobs/j1/messages')).toMatchObject({ text: '', skill: 'review:check-diff' });
    // to an obid that doesn't take skills, an empty box is nothing to send
    const off = await connected(['composer-v1']);
    expect(off.live.ask('', 'c1', undefined, undefined, { skill: 'write-tests' })).toBeUndefined();
  });

  it('composer_27_an_obid_without_skills_v1_never_gets_skill', async () => {
    const { live, calls } = await connected(['composer-v1']);
    live.ask('the login page', 'c1', undefined, undefined, { skill: 'write-tests' });
    expect(await bodyOf(calls, '/v1/plans')).not.toHaveProperty('skill');
    await live.tellJob('j1', 'and the footer', [], true, 'write-tests');
    expect(await bodyOf(calls, '/v1/jobs/j1/messages')).not.toHaveProperty('skill');
  });
});
