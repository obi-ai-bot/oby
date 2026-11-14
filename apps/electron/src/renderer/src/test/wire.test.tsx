import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { InboxItem, Job, TranscriptEntry } from '../obid';
import type { Task } from '../tasks';

// A value obid sends that this build doesn't know shows neutral: gray, no word of a known state's,
// nothing to answer, nothing thrown (docs/contracts/wire.md)

// a menu's portal has no server markup: its items, in place
vi.mock('../Menu', () => ({ Menu: ({ children }: { children: React.ReactNode }) => <div>{children}</div> }));
vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));

const call = vi.fn();
let live: typeof import('../live');
let tellPlaceholder: typeof import('../tasks').tellPlaceholder;
let ui: {
  TaskCard: typeof import('../TaskCard').default; AskBlock: typeof import('../TaskCard').AskBlock;
  ThreadTask: typeof import('../ChatView').ThreadTask; StatusDot: typeof import('../StatusDot').default; TaskMenu: typeof import('../TaskMenu').default;
};
beforeAll(async () => {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', { app: { obid: { call } } });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  // ChatView's words make a scratch textarea as they load
  vi.stubGlobal('document', { createElement: () => ({}) });
  live = await import('../live');
  ({ tellPlaceholder } = await import('../tasks'));
  const card = await import('../TaskCard');
  ui = {
    TaskCard: card.default, AskBlock: card.AskBlock, ThreadTask: (await import('../ChatView')).ThreadTask,
    StatusDot: (await import('../StatusDot')).default, TaskMenu: (await import('../TaskMenu')).default,
  };
});
beforeEach(() => {
  const s = live.liveState();
  s.jobs = {};
  s.inbox = [];
});

const at = '2026-11-14T10:00:00Z';
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Fix the login', agent: 'claude', account: 'a1', repo: 'o/r', branch: 'oby/fix-1', base: 'dev', issue: null, kind: 'task',
  status: 'running', step: 'Working', summary: '', created: at, finished: null, commits: null, published: null, rounds: 1, round: 1,
  check: null, model: null, dismissed: null, permission: null, pipeline: null, title: 'Fix the login', ...more,
});
const item = (kind: string, more: Partial<InboxItem> = {}): InboxItem => ({
  id: `j1-${kind}`, kind, job: 'j1', title: 'Pick one', text: 'obid’s words', options: ['A', 'B'], details: null,
  created: at, state: 'open', later: null, headline: null, recommended: null, ...more,
});
// the task the screens draw for obid's job, with what obid has for it
const taskFor = (j: Job, items: InboxItem[] = []): Task => {
  const s = live.liveState();
  s.jobs = { [j.id]: j };
  s.inbox = items;
  // opened already: no blue dot
  s.seen.add(j.id);
  return live.taskOf(s, j);
};
const read = (html: string) => html.replaceAll('<!-- -->', '');
const rect = { top: 0, bottom: 0, left: 0, right: 0, width: 0, height: 0, x: 0, y: 0 } as DOMRect;
const markup = (task: Task) => ({
  dot: read(renderToStaticMarkup(<ui.StatusDot task={task} />)),
  thread: read(renderToStaticMarkup(<ui.ThreadTask task={task} current onOpen={() => {}} />)),
  menu: read(renderToStaticMarkup(<ui.TaskMenu task={task} at={rect} onClose={() => {}} onOpenChat={() => {}} onRename={() => {}} />)),
  card: read(renderToStaticMarkup(<ui.TaskCard task={task} />)),
});

describe('WIRE-2: an inbox item of a kind this build doesn’t know', () => {
  it('wire_2_unknown_inbox_kind_renders', () => {
    const s = live.liveState();
    s.jobs = { j1: job() };
    s.inbox = [item('somethingNew')];
    const ask = live.askOf(s, s.jobs.j1);
    expect(ask).toMatchObject({ headline: 'Pick one', text: 'obid’s words', options: [] });
    const html = renderToStaticMarkup(<ui.AskBlock task={{ id: 'j1', job: { ask } }} />);
    expect(html).toContain('Pick one');
    expect(html).toContain('tc-ask-kind is-unknown');
    // its label is obid's kind, as before
    expect(html).toContain('>somethingNew<');
    expect(html).not.toContain('<button');
  });

  it('wire_2_unknown_inbox_kind_renders: a known kind keeps its buttons', () => {
    const s = live.liveState();
    s.jobs = { j1: job() };
    s.inbox = [item('permission')];
    const html = renderToStaticMarkup(<ui.AskBlock task={{ id: 'j1', job: { ask: live.askOf(s, s.jobs.j1) } }} />);
    for (const b of ['A', 'B', 'Later']) expect(html).toMatch(new RegExp(`<button[^>]*>${b}</button>`));
    expect(html).not.toContain('is-unknown');
  });
});

describe('WIRE-3: a job status this build doesn’t know', () => {
  it('wire_3_unknown_job_status_shows_a_neutral_mark', () => {
    for (const items of [[], [item('failed', { options: ['Try again', 'Dismiss'] })], [item('permission')]]) {
      const task = taskFor(job({ status: 'paused', finished: at }), items);
      expect(task.status).not.toBe('needs');
      expect(task.job.ask).toBeUndefined();
      expect(task.unknown).toBe(true);
      const { dot, thread, menu, card } = markup(task);
      expect(dot).toContain('is-unknown');
      expect(dot).toContain('aria-label="paused"');
      expect(dot).not.toContain('<svg');
      expect(dot).not.toContain('is-working');
      for (const word of ['>Done', 'Running', 'Queued', 'Needs you', 'Failed', 'Stopped']) expect(thread).not.toContain(word);
      expect(thread).toContain('status-dot is-unknown');
      expect(thread).not.toContain('is-failed');
      expect(menu).not.toContain('Stop task');
      expect(menu).not.toContain('Archive task');
      expect(menu).toContain('Rename');
      expect(card).not.toContain('tc-result');
      // the box under it names no state and offers no follow-up
      expect(tellPlaceholder(task, 'Claude Code')).not.toContain('follow-up');
    }
  });

  it('wire_3_unknown_job_status_shows_a_neutral_mark: a done job beside it is as before', () => {
    const task = taskFor(job({ status: 'done', finished: at }));
    expect(task.unknown).toBeUndefined();
    const { dot, thread, menu } = markup(task);
    expect(dot).toContain('aria-label="Done"');
    expect(dot).toContain('<svg');
    expect(thread).toContain('>Done · Claude Code');
    expect(menu).toContain('Archive task');
    expect(tellPlaceholder(task, 'Claude Code')).toContain('follow-up');
  });
});

describe('WIRE-4: an agent this build doesn’t know', () => {
  it('wire_4_unknown_agent_renders_with_its_own_name', async () => {
    const { agentOf } = await import('../mock');
    const { BRANDS } = await import('../brands');
    expect(agentOf('gemini')).toEqual({ name: 'gemini', brand: BRANDS.circle });
    expect(agentOf('codex').name).toBe('Codex');
    const task = taskFor(job({ agent: 'gemini', status: 'done', finished: at }));
    expect(task.brand).toBe(BRANDS.circle);
    const { thread, card } = markup(task);
    expect(thread).toContain('gemini · ');
    expect(card).toContain('gemini · ');
  });
});

describe('WIRE-5: a stage, a transcript entry or a state this build doesn’t know', () => {
  it('wire_5_unknown_stage_and_session_values_render_neutral', async () => {
    const stages = [{ id: 'prepare', title: 'Prepare', state: 'done' }, { id: 'work', title: 'Work', state: 'later' }];
    const task = taskFor(job({ pipeline: { number: 1, total: 2, stages } }));
    const card = read(renderToStaticMarkup(<ui.TaskCard task={task} alone />));
    expect(card).toContain('aria-label="later"');
    expect(card).not.toContain('is-failed');

    const { sessionOf } = await import('../jobPage');
    const entry = (n: number, more: Partial<TranscriptEntry>): TranscriptEntry => ({ n, at, kind: 'said', summary: 'Said', ...more });
    const lines = sessionOf([entry(1, { kind: 'said', text: 'Hi' }), entry(2, { kind: 'video' }), entry(3, { kind: 'tool', summary: 'Reading a.ts', state: 'paused', exit: 1 })]);
    expect(lines).not.toContain(undefined);
    expect(lines.map((l) => l.kind)).toEqual(['said', 'tool']);
    const tool = lines[1] as Extract<(typeof lines)[number], { kind: 'tool' }>;
    expect(tool.state).not.toBe('failed');
    expect(tool.meta).toBeUndefined();
    // drawn as the gray ring still to come, with no word: a call's, and a to-do's
    const { Line } = await import('../SessionView');
    const { BRANDS } = await import('../brands');
    const draw = (e: (typeof lines)[number]) => renderToStaticMarkup(<Line entry={e} job="j1" brand={BRANDS.claude} live={{ shown: '', tick: 0 }} />);
    const drawn = draw(tool);
    expect(drawn).toContain('status-dot is-queued');
    expect(drawn).not.toContain('is-failed');
    expect(drawn).not.toContain('Declined');
    expect(draw({ kind: 'plan', todos: [{ text: 'Run the check', state: 'later' }] })).toContain('status-dot is-queued');
  });
});
