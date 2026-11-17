import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { liveBridge } from './liveBridge';
import type { Task } from '../tasks';
import type { Job as JobView } from '../view';

// A conversation's tasks nested under its row in the side panel, each a session of its own (Sangam's
// screens 03 and 04, docs/contracts/live-task.md, LIVETASK-19 to 22). From obid's own answers, through
// live.ts, to the real Sidebar rendered: what obid says on one side, the rows on the other.
vi.mock('react', async (original) => ({
  ...await original<typeof import('react')>(),
  useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
}));
// clicks are the handlers React got, by a button's label or else its class; a server render keeps no state
const clicks = vi.hoisted(() => ({}) as Record<string, () => void>);
vi.mock('react/jsx-dev-runtime', async (original) => {
  const real = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...real, jsxDEV: (type: never, props: { onClick?: () => void; 'aria-label'?: string; title?: string; className?: string }, ...rest: never[]) => {
    if (props?.onClick) clicks[props['aria-label'] ?? props.title ?? props.className ?? ''] = props.onClick;
    return (real.jsxDEV as (...a: unknown[]) => unknown)(type, props, ...rest);
  } };
});
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));

let answers: Record<string, unknown> = {};
let kept: Record<string, string> = {};
const flush = async () => { for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0)); };
const at = (m: number) => new Date(Date.UTC(2026, 10, 16, 10, m)).toISOString();
const job = (id: string, conversation: string | null, created: number, more: Record<string, unknown> = {}) => ({
  id, request: 'Fix it', agent: 'claude', account: 'a1', repo: 'well-prado/oby-scratch', branch: 'oby/fix', base: 'main', issue: null, kind: 'work',
  status: 'running', step: 'Working', summary: '', created: at(created), finished: null, commits: 0, published: null, rounds: 1, round: 1, check: null,
  model: null, dismissed: null, permission: null, pipeline: null, title: `Task ${id}`, archived: null, conversation, turn: null, ...more,
});
const talk = (id: string, title: string) => ({ id, title, pinned: false, created: at(0), lastActivity: at(0), cursor: '' });
const permission = (id: string, jobId: string) => ({
  id, kind: 'permission', job: jobId, title: 'Run a command', text: 'It wants to list the signing identities.', options: ['Allow once', 'Deny'],
  details: null, created: at(9), state: 'open', later: null, headline: null, recommended: null,
});

// The app connected to an obid holding these, and the side panel as it stands
async function on({ jobs = [], inbox = [], conversations = [] }: { jobs?: unknown[]; inbox?: unknown[]; conversations?: unknown[] }) {
  vi.resetModules();
  for (const k of Object.keys(clicks)) delete clicks[k];
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: (k: string) => kept[k] ?? null, setItem: (k: string, v: string) => { kept[k] = v; }, removeItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('innerWidth', 1000);
  vi.stubGlobal('addEventListener', () => {});
  vi.stubGlobal('removeEventListener', () => {});
  const bridge = liveBridge();
  vi.stubGlobal('window', { app: { obid: {
    call: async (method: string, path: string) => {
      const a = answers[`${method} ${path}`];
      return a === undefined ? { status: 404, text: '{"error":"Not found."}' } : { status: 200, text: JSON.stringify(a) };
    },
    confirm: async () => ({ status: 200, text: '{}' }),
    ...bridge.obid,
  } } });
  answers['GET /v1/status'] = { runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'a1', agent: 'claude', name: 'Deskree Team' }], models: {}, maxRunning: 6, running: 0 };
  const live = await import('../live');
  live.connect();
  bridge.sync({ jobs, inbox, conversations, limits: [] });
  await flush();
  expect(live.liveState().connection).toBe('live');
  const { default: Sidebar } = await import('../Sidebar');
  const none = () => {};
  const opened = vi.fn();
  return {
    opened,
    side: (more: { activeTalk?: string; activeAgent?: string } = {}) => renderToStaticMarkup(
      <Sidebar open atHome={false} activeTalk={more.activeTalk} activeAgent={more.activeAgent} width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none}
        onOpenTask={none} onOpenAgent={opened} onOpenChat={none} onOpenOby={none} onOpenMember={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
    ).replaceAll('<!-- -->', ''),
  };
}
// one conversation's part of the panel: its row and its nested tasks, up to the next conversation
const nestOf = (html: string, title: string) => html.slice(html.lastIndexOf('<div class="convo-nest', html.indexOf(`>${title}<`))).split(/<div class="convo-nest/)[1] ?? '';
const kidsOf = (nest: string) => [...nest.matchAll(/<button class="convo-kid[^"]*"[^>]*title="([^"]*)"/g)].map((m) => m[1]);

// Two conversations: c1 started a task at work and one asking to run a command, and one archived;
// c2 one finished and one failed
const TWO = {
  conversations: [talk('c1', 'Two fixes for today'), talk('c2', 'Where is 0.9 at?')],
  jobs: [
    job('j1', 'c1', 1, { title: 'Fix the queue order', step: 'Running cargo test' }),
    job('j2', 'c1', 2, { title: 'Fix iOS signing', agent: 'codex', permission: { id: 'p1', tool: 'Bash', title: 'Run a command', detail: 'security find-identity -v -p codesigning', rule: null, always: null } }),
    job('j0', 'c1', 0, { title: 'An archived one', status: 'done', archived: at(5) }),
    job('j3', 'c2', 3, { title: 'Write the notes', status: 'done', commits: 1, finished: at(6) }),
    job('j4', 'c2', 4, { title: 'Bump the version', status: 'failed', reason: 'The check failed.', finished: at(7) }),
  ],
  inbox: [permission('i1', 'j2')],
};

describe('a conversation\'s tasks, nested under it', () => {
  afterEach(() => { vi.unstubAllGlobals(); answers = {}; kept = {}; });

  it('LIVETASK-19: each task a row under the conversation that started it, oldest first: its agent, its title, what it is on, its state', async () => {
    const { side } = await on(TWO);
    const html = side({ activeTalk: 'c1' });
    const c1 = nestOf(html, 'Two fixes for today');
    // the archived one isn't listed; the two others in the order they started
    expect(kidsOf(c1)).toEqual(['Fix the queue order on Deskree Team', 'Fix iOS signing on Deskree Team']);
    expect(c1).toContain('<b>Fix the queue order</b> · Running cargo test');
    expect(c1).toContain('<b>Fix iOS signing</b> · asks to run a command');
    expect(c1).toMatch(/Fix the queue order<\/b>.*aria-label="Working"><span class="status-dot is-working">/);
    expect(c1).toMatch(/Fix iOS signing<\/b>.*aria-label="Needs you"><span class="status-dot is-needs">/);
    expect(c1).not.toContain('An archived one');
    // done and failed, each its own mark (c2 shown)
    const c2 = nestOf(side({ activeTalk: 'c2' }), 'Where is 0.9 at?');
    expect(c2).toMatch(/<b>Write the notes<\/b> · Done<\/span><span class="status" role="img" aria-label="Done"><svg/);
    expect(c2).toMatch(/<b>Bump the version<\/b> · Failed<\/span><span class="status" role="img" aria-label="Failed"><span class="status-dot is-failed">/);
    // a conversation with no task has no twist
    const { side: alone } = await on({ conversations: [talk('c9', 'Just talk')] });
    expect(alone()).not.toContain('convo-twist');
  });

  it('LIVETASK-20: shown for the open conversation and one whose task needs you; the twist folds and shows it, and that is kept', async () => {
    const { side } = await on(TWO);
    // c1 needs you: shown, open or not; c2 neither open nor needing: folded
    const html = side();
    expect(nestOf(html, 'Two fixes for today')).toContain('aria-expanded="true" aria-label="Hide Two fixes for today’s agents"');
    expect(nestOf(html, 'Where is 0.9 at?')).toContain('aria-expanded="false" aria-label="Show Where is 0.9 at?’s agents"');
    expect(nestOf(html, 'Where is 0.9 at?')).not.toContain('convo-kids');
    // the open one is shown
    expect(nestOf(side({ activeTalk: 'c2' }), 'Where is 0.9 at?')).toContain('convo-kids');
    // folded by the owner, it stays folded, needing or not, and that is kept
    clicks['Hide Two fixes for today’s agents']();
    expect(JSON.parse(kept['oby.kidsShown'])).toEqual({ c1: false });
    const again = await on(TWO);
    expect(nestOf(again.side(), 'Two fixes for today')).not.toContain('convo-kids');
  });

  it('LIVETASK-23: a conversation with tasks has the same row as one without, its twist in the row\'s own gutter with no box of its own', async () => {
    const { side } = await on({ ...TWO, conversations: [...TWO.conversations, talk('c9', 'Just talk')] });
    const html = side({ activeTalk: 'c1' });
    // the row's opening tag, with the state classes that are meant to differ taken off
    const rowOf = (title: string) => nestOf(html, title).match(/<div role="button"[^>]*class="([^"]*)"/)![1].replace(/ is-active| is-menu-open/g, '');
    expect(rowOf('Two fixes for today')).toBe('connector task convo-row');
    expect(rowOf('Just talk')).toBe(rowOf('Two fixes for today'));
    // the twist is a sibling before the row, not in it, and the only thing that differs
    const nest = nestOf(html, 'Two fixes for today');
    expect(nest.indexOf('convo-twist')).toBeGreaterThan(-1);
    expect(nest.indexOf('convo-twist')).toBeLessThan(nest.indexOf('convo-row'));
    expect(nestOf(html, 'Just talk')).not.toContain('convo-twist');
    // the css: every row keeps the gutter, the twist is inside the row's box (no negative offset, no fill of its own)
    const css = readFileSync(fileURLToPath(new URL('../app.css', import.meta.url)), 'utf8');
    const rule = (sel: string) => css.split('\n').filter((l) => l.startsWith(`${sel} {`)).join('');
    expect(rule('.convo-row')).toContain('padding-left: var(--convo-gutter');
    expect(rule('.convo-twist')).toMatch(/left: 0[;\s]/);
    expect(rule('.convo-twist')).not.toMatch(/background|border-radius|left: -/);
    expect(rule('.convo-twist:hover, .convo-twist:focus-visible')).not.toContain('background');
  });

  it('LIVETASK-21: the conversation\'s second line says what waits on you, else how many agents work; its mark says so too', async () => {
    const { side } = await on(TWO);
    const c1 = nestOf(side(), 'Two fixes for today');
    expect(c1).toContain('<span class="connector-about">Codex asks to run a command</span>');
    expect(c1).toContain('status-dot is-needs" role="img" aria-label="Needs you"');
    const working = await on({ conversations: [talk('c1', 'Two fixes for today')], jobs: [job('j1', 'c1', 1), job('j2', 'c1', 2, { agent: 'codex' })] });
    expect(nestOf(working.side(), 'Two fixes for today')).toContain('<span class="connector-about">2 agents working</span>');
    // none at work: the conversation keeps its own line
    const ended = await on({ conversations: [talk('c1', 'Two fixes for today')], jobs: [job('j1', 'c1', 1, { status: 'done', finished: at(3) })] });
    expect(nestOf(ended.side({ activeTalk: 'c1' }), 'Two fixes for today')).not.toContain('agents working');
  });

  it('LIVETASK-22: a nested row opens that task\'s own thread over its conversation, and is lit while it shows', async () => {
    const { side, opened } = await on(TWO);
    const html = side({ activeAgent: 'j2' });
    expect(nestOf(html, 'Two fixes for today')).toMatch(/<button class="convo-kid is-active" aria-current="true" title="Fix iOS signing on Deskree Team"/);
    clicks['Fix iOS signing on Deskree Team']();
    expect(opened).toHaveBeenCalledWith('j2', 'c1');
  });
});

// The rules each row's words come from (nested.ts)
describe('nested.ts', () => {
  const task = (status: Task['status'], more: Partial<JobView> = {}) => ({
    id: 'x', type: 'tasks', title: 'T', brand: 'claude', status,
    job: { id: 'x', status: status === 'done' ? 'done' : 'running', step: 'Reading a.ts', agent: 'codex', account: 'Personal', stages: [], created: 3, ...more },
  }) as unknown as Task;

  it('LIVETASK-19: a row\'s line and state from where the task stands', async () => {
    const { kidLine, kidState, kidsLine, kidsOf } = await import('../nested');
    expect(kidLine(task('running'))).toBe('Reading a.ts');
    expect(kidLine(task('running', { status: 'queued' }))).toBe('Queued');
    expect(kidLine(task('needs', { ask: { kind: 'question', headline: 'Which?', text: '', options: [] } }))).toBe('has a question');
    expect(kidLine(task('needs', { ask: { kind: 'permission', headline: 'Run it?', text: '', options: [], detail: 'npm test' } }))).toBe('asks to run a command');
    expect(kidLine(task('done', { nothingDone: true }))).toBe('Nothing done');
    expect([kidState(task('done', { status: 'failed' })), kidState(task('done', { status: 'stopped' })), kidState(task('done'))]).toEqual(['failed', 'failed', 'done']);
    expect(kidsLine([task('running'), task('running')])).toBe('2 agents working');
    expect(kidsLine([task('done')])).toBeUndefined();
    // oldest first, archived left out, only this conversation's
    const a = { ...task('running', { created: 1 }), id: 'a' }, b = { ...task('running', { created: 9 }), id: 'b' }, gone = { ...task('done', { archived: true }), id: 'g' };
    expect(kidsOf([a, b, gone], 'c', () => 'c').map((t) => t.id)).toEqual(['b', 'a']);
    expect(kidsOf([a, b], 'c', (id) => (id === 'a' ? 'c' : 'd')).map((t) => t.id)).toEqual(['a']);
  });
});
