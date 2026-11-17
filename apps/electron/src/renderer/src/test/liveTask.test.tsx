import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Job as JobView } from '../view';
import type { Entry } from '../session';
import { around, knownUpTo, lastSteps, thoughtWords, withEntries, withLine } from '../liveTask';
import { parseEvent } from '../../../shared/obid-wire';
import { detailOf, sessionOf } from '../jobPage';
import type { TranscriptEntry } from '../obid';
import type { Chat } from '../chats';
import type { Task } from '../tasks';

// A task's live card in its conversation, and its own thread, a conversation of its own (docs/contracts/live-task.md).
// Each box (Composer) is a marker that keeps what it's given, by its label; what the boxes send to is spied on,
// and onObid is read when a box sends, so the same render sends on obid or off it.
type Props = { placeholder: string; label: string; onSend: (text: string, att: object) => void; onStop?: () => void };
const page = vi.hoisted(() => ({ boxes: {} as Record<string, Props>, chats: [] as unknown[], obid: false }));
// (a store without a server snapshot, peers' conversations, reads as it is)
vi.mock('react', async (original) => {
  const real = await original<typeof import('react')>();
  return { ...real, useSyncExternalStore: (_: unknown, get: () => unknown) => get() };
});
vi.mock('../Composer', () => ({
  default: (props: Props) => { page.boxes[props.label] = props; return <input placeholder={props.placeholder} data-stop={props.onStop ? 'yes' : undefined} />; },
  typingIn: () => false,
}));
vi.mock('../Menu', async (original) => ({ ...await original<typeof import('../Menu')>(), Menu: ({ children }: { children: React.ReactNode }) => <div role="menu">{children}</div> }));
vi.mock('../chats', async (original) => ({ ...await original<typeof import('../chats')>(), useChats: () => page.chats, tellChat: vi.fn() }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), tellAgent: vi.fn(async () => undefined) }));
vi.mock('../tasks', async (original) => ({ ...await original<typeof import('../tasks')>(), tell: vi.fn() }));
vi.mock('../lean', async (original) => ({ ...await original<typeof import('../lean')>(), get onObid() { return page.obid; } }));
const read = (html: string) => html.replaceAll('<!-- -->', '');
function stub() {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', {});
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('performance', { now: () => 0 });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('addEventListener', () => {});
  vi.stubGlobal('removeEventListener', () => {});
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: () => {}, removeEventListener: () => {} }));
  vi.stubGlobal('requestAnimationFrame', () => 0);
  vi.stubGlobal('innerWidth', 1000);
  vi.stubGlobal('DOMRect', class { constructor(public x: number, public y: number, public width: number, public height: number) {} });
}
async function taskOf(id: string, more: Partial<JobView> = {}, status: 'running' | 'needs' | 'done' = 'running') {
  const { AGENTS } = await import('../view');
  const job = {
    id, type: 'issues', title: 'Fix the login test', request: 'Fix it', agent: 'claude', account: 'Personal', model: 'Opus',
    repo: 'deskree/obi', branch: 'oby/fix-1', base: 'dev', status: status === 'done' ? 'done' : 'running', step: 'Working', stages: [], created: 12, ...more,
  } as JobView;
  return { id, type: 'issues', title: job.title, brand: AGENTS.claude.brand, status, job } as Task;
}
// the card, and the task's own thread (TaskThread)
const WORKING = [{ id: 'prepare', title: 'Prepare', state: 'done' }, { id: 'work', title: 'Work', state: 'current' }, { id: 'check', title: 'Check', state: 'upcoming' }] as JobView['stages'];
async function views(entries: Entry[], more: Partial<JobView> = {}, status: 'running' | 'needs' | 'done' = 'running', onOpenAgent: (id: string) => void = () => {}) {
  stub();
  const { ThreadTask } = await import('../ChatView');
  const { default: TaskThread } = await import('../TaskThread');
  const { SESSIONS } = await import('../session');
  const { replaceTasks } = await import('../tasks');
  SESSIONS.j1 = entries;
  const task = await taskOf('j1', more, status);
  replaceTasks([task]);
  return {
    card: read(renderToStaticMarkup(<ThreadTask task={task} current={false} onOpen={() => {}} />)),
    opened: read(renderToStaticMarkup(<TaskThread id="j1" leaving={false} onBack={() => {}} onGone={() => {}} onOpenAgent={onOpenAgent} />)),
  };
}
// a conversation with Oby (c1) whose answer started two tasks: j1 at work, j2 needing the owner
const ask = { kind: 'permission' as const, headline: 'Run npm test?', text: 'It wants to run the tests.', options: ['Allow', 'Deny'] };
async function conversation(tasks?: string[]) {
  stub();
  const { default: ChatView } = await import('../ChatView');
  const { replaceTasks } = await import('../tasks');
  const { SESSIONS } = await import('../session');
  SESSIONS.j1 = [tool('Read', 'a.ts')];
  SESSIONS.j2 = [{ kind: 'said', text: 'It needs the tests run.', at: '1:50 PM' }];
  const all = [await taskOf('j1'), { ...await taskOf('j2', { ask }, 'needs'), title: 'Run the tests' }];
  replaceTasks(all.filter((t) => !tasks || tasks.includes(t.id)));
  page.chats = [{ id: 'c1', title: 'Two fixes', said: [{ who: 'you', text: 'fix both', ago: 5 }, { who: 'oby', text: 'On it.', ago: 4, jobs: ['j1', 'j2'] }] } satisfies Chat];
  const none = () => {};
  return read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={none} onGone={none} onOpenTask={none} />));
}
// one card's own part of a conversation's markup, up to the next card or the message's time
const cardOf = (html: string, title: string) => html.slice(html.indexOf(`aria-label="Open ${title}"`)).split(/class="msg-task-work|class="msg-time"/)[0];
const tool = (verb: string, target: string, state: 'running' | 'done' | 'failed' = 'done'): Entry => ({ kind: 'tool', tool: 'Read', verb, target, state, seconds: 1 });
const plan: Entry = { kind: 'plan', todos: [{ text: 'Read the test', state: 'done' }, { text: 'Replace the timer', state: 'running' }, { text: 'Run the check', state: 'pending' }] };
const wire = (n: number, more: Partial<TranscriptEntry> = {}): TranscriptEntry => ({ n, at: '2026-11-02T10:00:00Z', kind: 'tool', summary: 'Reading a.ts', state: 'running', ...more });

describe('a task at work', () => {
  beforeEach(() => { page.boxes = {}; page.obid = false; vi.clearAllMocks(); });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('LIVETASK-1: the card says what it is on and its stages; its thread shows its to-dos with the one it is on lit, and what it did', async () => {
    const { card, opened } = await views([{ kind: 'said', text: 'Looking.', at: '1:50 PM' }, tool('Read', 'a.ts'), tool('Read', 'b.ts'), plan, { kind: 'thought', text: '', seconds: 6 }, tool('Editing', 'session.ts', 'running')], { step: 'Editing session.ts', stages: WORKING });
    expect(card).toContain('>Running · Claude Code · obi<');
    expect(card).toMatch(/msg-task-now"><span class="sheen">Work · Editing session\.ts</);
    expect(card).toMatch(/tc-progress msg-task-stages" role="img" aria-label="Stage 2 of 3: Work"><span class="is-done"><\/span><span class="is-current"><\/span><span><\/span>/);
    // the card holds no list: the thread does, on top
    expect(card).not.toContain('What it’s doing');
    expect(opened).toContain('What it’s doing');
    expect(opened).toMatch(/is-done.*Read the test/);
    expect(opened).toMatch(/is-on is-working.*sheen">Replace the timer/);
    expect(opened).toMatch(/is-up.*Run the check/);
    expect(opened).toContain('b.ts');
    expect(opened).toContain('Thought for 6s');
    expect(opened).toContain('session.ts');
  });

  it('LIVETASK-1: a long list on a card shows the part around the one it is on', () => {
    const todos = Array.from({ length: 20 }, (_, i) => ({ text: `T${i}`, state: i < 10 ? 'done' as const : i === 10 ? 'running' as const : 'pending' as const }));
    const { todos: shown, before, after } = around(todos);
    expect(shown.map((t) => t.text)).toEqual(['T8', 'T9', 'T10', 'T11', 'T12', 'T13', 'T14', 'T15']);
    expect([before, after]).toEqual([8, 4]);
    expect(around(todos.slice(0, 5)).before).toBe(0);
  });

  it('LIVETASK-2: with no list from the agent it shows only its steps, and invents no to-do', async () => {
    const { card, opened } = await views([tool('Read', 'a.ts'), tool('Running', 'npm test', 'running')], { step: 'Running npm test' });
    expect(card).not.toContain('What it’s doing');
    expect(opened).not.toContain('lt-thread-todos');
    expect(card).toContain('npm test');
    // and before it has done anything, what obid says it's on; nothing said, no line for it
    expect((await views([], { step: 'Waiting for a free slot' })).card).toContain('Waiting for a free slot');
    expect((await views([], { step: '', waiting: undefined })).card).not.toContain('msg-task-now');
  });

  it('RECOVERY-5: a job being tried again says so, under the steps it already has', async () => {
    const trying = 'Trying again in 5 minutes (2 of 3)';
    const reading = { ...tool('Read', 'a.ts'), summary: 'Reading a.ts' } as Entry;
    expect((await views([reading], { step: trying })).card).toContain(trying);
    expect((await views([reading, plan], { step: trying })).card).toContain(trying);
    // on the card, what it's on is said once
    const times = (card: string, text: string) => card.split(text).length - 1;
    expect(times((await views([reading], { step: 'Reading a.ts' })).card, 'Reading a.ts')).toBe(1);
  });

  it('LIVETASK-3: obid\'s transcript events are read, entries update by n, and words never go back', () => {
    expect(parseEvent('transcript', JSON.stringify({ job: 'j1', entry: wire(3) }))).toEqual({ kind: 'transcript', job: 'j1', entry: wire(3) });
    expect(parseEvent('transcript', JSON.stringify({ job: 'j1', n: 9, append: 'A line\n' }))).toEqual({ kind: 'transcriptLine', job: 'j1', n: 9, append: 'A line\n' });
    expect(parseEvent('transcript', JSON.stringify({ job: 'j1' }))).toBeNull();
    // a call comes again when it ends, in its place
    const ended = withEntries([wire(1), wire(2)], [wire(1, { state: 'done', seconds: 2 })]);
    expect(ended.map((e) => [e.n, e.state])).toEqual([[1, 'done'], [2, 'running']]);
    // a fetch answered late doesn't undo what an event already said
    expect(withEntries(ended, [wire(1)])[0].state).toBe('done');
    const words = wire(4, { kind: 'said', state: undefined, text: 'One\nTwo\n' });
    expect(withEntries([words], [{ ...words, text: 'One\n' }])[0].text).toBe('One\nTwo\n');
    // the same line from a fetch and its event shows once
    expect(withLine([{ ...words, fetched: true }], 4, 'Two\n')[0].text).toBe('One\nTwo\n');
    // and only then: an agent that writes the same line twice wrote it twice
    expect(withLine([words], 4, 'Two\n')[0].text).toBe('One\nTwo\nTwo\n');
    expect(withLine(withLine([{ ...words, fetched: true }], 4, 'Two\n'), 4, 'Two\n')[0].text).toBe('One\nTwo\nTwo\n');
    expect(withLine([words], 4, 'Three\n')[0].text).toBe('One\nTwo\nThree\n');
    // what to fetch after: the last with none missing before it
    expect(knownUpTo([wire(1), wire(2), wire(5)])).toBe(2);
    expect(knownUpTo([])).toBe(0);
  });

  it('LIVETASK-4: ended, the card folds to one line that says so, with its branch and pull request', async () => {
    const { card } = await views([plan, tool('Read', 'a.ts')], { commits: 2, published: 'https://github.com/deskree/obi/pull/9', finished: 1 }, 'done');
    expect(card).toContain('>Done · Claude Code · obi<');
    expect(card).toContain('2 commits on oby/fix-1');
    expect(card).toContain('obi#9');
    expect(card).not.toContain('lt-now');
    expect((await views([plan], { status: 'stopped' }, 'done')).card).toContain('>Stopped · Claude Code');
    expect((await views([plan], { status: 'failed' }, 'done')).card).toContain('>Failed · Claude Code');
  });

  it('LIVETASK-5: a click on the card opens its own thread, which holds its words, steps, thinking and to-dos, and a box with Stop in it', async () => {
    const { card, opened } = await views([
      { kind: 'start', text: 'Started on Claude Code', at: '1:50 PM' }, { kind: 'you', text: 'Fix the login test', at: '1:50 PM' }, tool('Read', 'a.ts'),
      { kind: 'said', text: 'It races a timer.', at: '1:53 PM' }, plan, { kind: 'thought', text: '', seconds: 8 }, tool('Editing', 'session.ts', 'running'),
    ]);
    // a page of its own, its head as the card's, not a card opened in place
    expect(opened).toMatch(/^<div class="chat-layer agent-layer"><div class="lt-thread" role="group" aria-label="Conversation with Claude Code">.*<div class="msg-task agent-head">.*Fix the login test/);
    expect(opened.replace(/<[^>]+>/g, '')).toContain('It races a timer.');
    expect(opened).toContain('1:53 PM');
    expect(opened).toContain('Thought for 8s');
    expect(opened).toContain('session.ts');
    expect(opened).toMatch(/lt-thread-todos.*Replace the timer/);
    // the list is on top once: not a line of the conversation as well
    expect(opened.split('Replace the timer')).toHaveLength(2);
    expect(opened).not.toContain('ss-plan');
    expect(opened).toContain('placeholder="Tell Claude Code…"');
    expect(page.boxes['Tell the agent about Fix the login test'].onStop).toBeTypeOf('function');
    // no close or Stop button of its own: Stop is the box's
    expect(opened).not.toContain('>Stop<');
    expect(opened).not.toContain('Close this conversation');
    // the card: none of it, and nothing that opens in place
    expect(card).not.toContain('aria-expanded');
    expect(card).not.toContain('lt-thread');
    expect(card).not.toContain('ss-entries');
    expect(card).not.toContain('1:53 PM');
    expect(card).not.toContain('placeholder=');
    // a click on it opens the task's thread, by its id
    const { ThreadTask } = await import('../ChatView');
    const onOpen = vi.fn();
    const el = ThreadTask({ task: await taskOf('j1'), current: false, onOpen }) as React.ReactElement<{ children: React.ReactElement<{ onClick: () => void }> }>;
    el.props.children.props.onClick();
    expect(onOpen).toHaveBeenCalledWith('j1');
    // finished: no Stop, and what the box does then
    page.boxes = {};
    const done = (await views([tool('Read', 'a.ts')], {}, 'done')).opened;
    expect(done).not.toContain('data-stop');
    expect(page.boxes['Tell the agent about Fix the login test'].onStop).toBeUndefined();
    expect(done).toContain('placeholder="Ask for a follow-up…"');
  });

  it('LIVETASK-7: told something once finished, the follow-up it started opens as its own thread', async () => {
    const live = await import('../live');
    const followed = vi.fn();
    await views([tool('Read', 'a.ts')], {}, 'done', followed);
    vi.mocked(live.tellAgent).mockResolvedValueOnce('j9');
    page.obid = true;
    page.boxes['Tell the agent about Fix the login test'].onSend('one more thing', {});
    await vi.waitFor(() => expect(followed).toHaveBeenCalledWith('j9'));
    expect(live.tellAgent).toHaveBeenCalledWith('j1', 'one more thing', undefined);
  });

  it('LIVETASK-6: thinking shows only what its tool gave', async () => {
    const thought = (more: Partial<TranscriptEntry>) => sessionOf([wire(1, { kind: 'thought', summary: 'Thinking', state: undefined, ...more })])[0] as Extract<Entry, { kind: 'thought' }>;
    // going on
    expect(thoughtWords(thought({ state: 'running' }))).toBe('Thinking');
    // Claude Code: how long, with no words
    expect(thoughtWords(thought({ state: 'done', seconds: 8 }))).toBe('Thought for 8s');
    // Codex or Cursor: their words, and never a made-up time
    expect(thoughtWords(thought({ state: 'done', text: 'Only the project file sets this.' }))).toBe('Only the project file sets this.');
    expect(thoughtWords(thought({ state: 'done' }))).toBe('Thought');
    const { opened } = await views([thought({ state: 'running' }), thought({ state: 'done', seconds: 8 }), thought({ state: 'done', text: 'Only the project file.' })]);
    expect(opened).toContain('sheen">Thinking');
    expect(opened).toContain('<span>Thought for 8s</span>');
    expect(opened).toContain('<p>Only the project file.</p>');
    expect(opened).not.toContain('Thought for 0');
  });

  it('LIVETASK-11: a line opens to what it ran and gave back, from obid\'s own detail', () => {
    expect(detailOf({ input: { command: 'cargo test' }, output: { text: 'FAILED', lines: 1840, truncated: true, exit: 101 } })).toMatchObject({ input: 'cargo test', output: 'FAILED', lines: 1840, exit: 101 });
    expect(detailOf({ input: { file_path: '/w/a.ts', offset: 5, limit: 10 }, output: { text: 'x' } })).toMatchObject({ input: '/w/a.ts', range: 'Lines 5–14' });
    expect(detailOf({ input: { pattern: 'usage', path: 'src' } }).input).toBe('usage  in src');
    expect(detailOf({ input: { file_path: 'a.ts' }, diff: [{ path: 'a.ts', patch: '@@ -1 +1 @@\n-a\n+b' }, { path: 'b.ts', patch: '@@ -2 +2 @@' }] }).diff).toBe('@@ -1 +1 @@\n-a\n+b\n@@ -2 +2 @@');
    expect(detailOf({ input: { server: 'x' } }).input).toContain('"server": "x"');
    expect(detailOf({ kept: false }).input).toContain('didn’t keep');
    expect(detailOf()).toEqual({});
    // obid's number for it comes along, so it can be fetched
    expect(sessionOf([wire(7)])[0]).toMatchObject({ n: 7 });
  });

  it('LIVETASK-12: a stopped task says why, in obid\'s words, on one line', async () => {
    const { card } = await views([], { status: 'stopped', reason: 'Claude Code stopped without finishing (exit 1).\nThe end of its log', summary: 'I was reading the code.' }, 'done');
    expect(card).toContain('Claude Code stopped without finishing (exit 1).');
    expect(card).toContain('aria-label="Stopped"');
    expect(card).not.toContain('The end of its log');
    expect(card).not.toContain('I was reading the code.');
  });

  it('LIVETASK-13: one to-do list, the newest, updated in place', async () => {
    const lines = sessionOf([
      wire(2, { kind: 'plan', summary: '0 of 2 done', state: undefined, todos: [{ text: 'A', state: 'running' }, { text: 'B', state: 'pending' }] }),
      wire(3), wire(5, { kind: 'plan', summary: '1 of 2 done', state: undefined, todos: [{ text: 'A', state: 'done' }, { text: 'B', state: 'running' }] }),
    ]);
    expect(lines.filter((l) => l.kind === 'plan')).toEqual([{ n: 5, kind: 'plan', todos: [{ text: 'A', state: 'done' }, { text: 'B', state: 'running' }] }]);
  });

  it('LIVETASK-15: a line keeps obid\'s number as the list before it changes, and an unchanged one stays the same object', () => {
    const plan = (n: number) => wire(n, { kind: 'plan', state: undefined, todos: [{ text: 'A', state: 'running' }] });
    const tool = wire(3, { state: 'done' }), before = [plan(2), tool, wire(4, { state: 'done' })];
    const [, a1] = sessionOf(before);
    const after = sessionOf([...before, plan(5)]);
    // the older list went, so its place shifted; its number didn't
    expect(after.map((l) => l.n)).toEqual([3, 4, 5]);
    expect(after[0]).toBe(a1);
    // one at work is made anew, for its clock
    const running = wire(6);
    expect(sessionOf([running])[0]).not.toBe(sessionOf([running])[0]);
  });

  it('LIVETASK-5: the mock\'s running card has a conversation to review', async () => {
    const { SESSIONS } = await import('../session');
    // t1: a running job the mock's first conversation has a card for
    const { opened } = await views(SESSIONS.t1);
    expect(opened).toContain('Fix the flags route');
    expect(opened).toContain('Thought for 6s');
    // s1, at work, in the conversation that has a card needing you too (screen 03)
    const s1 = (await views(SESSIONS.s1)).opened;
    expect(s1).toContain('core/crates/obid/src/publish.rs');
    expect(s1).toContain('data-stop="yes"');
  });

  it('LIVETASK-9: waiting on you, the card says so, and its own thread carries the decision over its box', async () => {
    const { card, opened } = await views([plan], { ask }, 'needs');
    expect(card).toContain('>Needs you · Claude Code');
    // the card says so with its mark, no running sheen, and no decision under it (LIVETASK-25)
    expect(card).toMatch(/^<div class="msg-task-work is-live"><button class="msg-task is-link".*status-dot is-needs/);
    expect(card).not.toContain('sheen');
    expect(card).not.toContain('tc-ask');
    // in its own thread: the decision over its box, then the box with send and no Stop
    expect(opened).toMatch(/^<div class="chat-layer agent-layer">.*<\/section>.*class="decision-bar".*Run npm test\?.*>Allow<.*>Deny<.*placeholder="Tell Claude Code…"/);
    expect(opened).not.toContain('data-stop');
    expect(page.boxes['Tell the agent about Fix the login test'].onStop).toBeUndefined();
  });

  it('LIVETASK-15: a card goes with its job, its decision too', async () => {
    expect(await conversation()).toContain('class="decision-bar"');
    const gone = await conversation(['j1']);
    expect(gone).not.toContain('Run the tests');
    expect(gone).not.toContain('decision-bar');
  });

  it('LIVETASK-16: no card shows a decision, at work or waiting on you, and none opens in place', async () => {
    const html = await conversation();
    expect(cardOf(html, 'Run the tests')).not.toContain('tc-ask');
    expect(cardOf(html, 'Fix the login test')).not.toContain('tc-ask');
    expect(html).not.toContain('lt-thread');
    expect(html).not.toContain('aria-expanded="true"');
  });

  it('LIVETASK-24: a conversation\'s pending decision waits over its message box, on one line, from its task, with its buttons and Later', async () => {
    const html = await conversation();
    // after the thread, before the box
    expect(html).toMatch(/<\/section>.*class="decision-bar".*<input placeholder="Reply to Oby…"/);
    const bar = html.slice(html.indexOf('class="decision-bar"'), html.indexOf('<input'));
    // one line: its kind, its headline and the task it's from; obid's words behind Details
    expect(bar).toMatch(/class="tc-ask-line"><span class="tc-ask-kind is-permission">Permission<\/span><p class="tc-ask-head" title="Run npm test\?">Run npm test\?<span class="tc-ask-from"> · Run the tests<\/span><\/p><button type="button" class="tc-ask-more" aria-expanded="false">Details/);
    expect(bar).not.toContain('It wants to run the tests.');
    expect(bar).toMatch(/>Allow<.*>Deny<.*>Later</);
    // one decision: no count
    expect(bar).not.toContain('decision-count');
  });

  it('LIVETASK-24: several pending decisions show one at a time, "1 of 2", with the next one\'s arrow; a permission\'s command on one line', async () => {
    stub();
    const { default: ChatView } = await import('../ChatView');
    const { replaceTasks } = await import('../tasks');
    // obid's why after the command, on a line of its own
    const bash = { ...ask, headline: 'Run a command?', detail: 'npm test -- --watch=false\nAllowing it lets this task run the tests.' };
    replaceTasks([{ ...await taskOf('j1', { ask: bash }, 'needs'), title: 'First' }, { ...await taskOf('j2', { ask }, 'needs'), title: 'Second' }]);
    page.chats = [{ id: 'c1', title: 'Two', said: [{ who: 'oby', text: 'On it.', ago: 4, jobs: ['j1', 'j2'] }] } satisfies Chat];
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={() => {}} onGone={() => {}} onOpenTask={() => {}} />));
    expect(html).toMatch(/Run a command\?<span class="tc-ask-from"> · First<\/span><\/p><span class="decision-count">1 of 2<button type="button" class="decision-next" aria-label="Next decision"/);
    expect(html).toContain('<code class="tc-ask-command" title="npm test -- --watch=false\nAllowing it lets this task run the tests.">npm test -- --watch=false</code>');
    expect(html).not.toContain('>Allowing it');
    expect(html).not.toContain('· Second');
  });

  it('LIVETASK-24: a task of the conversation whose card isn\'t placed yet (its turns not loaded) still waits in its drawer', async () => {
    stub();
    const { default: ChatView } = await import('../ChatView');
    const { replaceTasks } = await import('../tasks');
    replaceTasks([{ ...await taskOf('j2', { ask, conversation: 'c1' }, 'needs'), title: 'Run the tests' }]);
    page.chats = [{ id: 'c1', title: 'Two', said: [] } satisfies Chat];
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={() => {}} onGone={() => {}} onOpenTask={() => {}} />));
    expect(html).toMatch(/class="decision-bar".*Run npm test\?<span class="tc-ask-from"> · Run the tests/);
    // another conversation's task isn't this one's
    page.chats = [{ id: 'c2', title: 'Other', said: [] } satisfies Chat];
    const other = read(renderToStaticMarkup(<ChatView kind="oby" id="c2" leaving={false} onBack={() => {}} onGone={() => {}} onOpenTask={() => {}} />));
    expect(other).not.toContain('decision-bar');
  });

  it('CHATFIRST-8: what the conversation asks with no task joins its drawer; another conversation\'s never does, nor what its thread shows already', async () => {
    stub();
    const { default: ChatView } = await import('../ChatView');
    const { replaceTasks } = await import('../tasks');
    const failed = { kind: 'question' as const, headline: 'The routine failed', text: '', options: ['Try again', 'Done'] };
    const share = { kind: 'question' as const, headline: 'Share it?', text: '', options: ['Confirm'] };
    replaceTasks([], { work: [], asks: [
      { id: 'own', title: 'Nightly', ask: failed, talk: 'c1' },
      { id: 'other', title: 'Elsewhere', ask: { ...failed, headline: 'Another one failed' }, talk: 'c2' },
      { id: 'shown', title: 'Share', ask: share, talk: 'c1' },
      { id: 'nowhere', title: 'No conversation', ask: { ...failed, headline: 'GitHub wants you' } },
    ] });
    // the Confirm is in the thread already, under Oby's answer
    page.chats = [{ id: 'c1', title: 'One', said: [{ who: 'oby', text: '', ago: 1, confirm: { id: 'shown', ask: share } }] } satisfies Chat];
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={() => {}} onGone={() => {}} onOpenTask={() => {}} />));
    const bar = html.slice(html.indexOf('class="decision-bar"'), html.indexOf('<input'));
    expect(bar).toMatch(/The routine failed<span class="tc-ask-from"> · Nightly/);
    expect(bar).not.toContain('decision-count');
    for (const word of ['Another one failed', 'GitHub wants you', 'Share it?']) expect(bar).not.toContain(word);
    replaceTasks([], { work: [], asks: [] });
  });

  it('LIVETASK-25: the card stays lean: its head alone, whatever it waits on, while the decision waits over the box', async () => {
    const merge = { kind: 'merge' as const, label: 'Ready to merge', headline: 'Merge oby-scratch #4 into main?', text: 'No check ran on its head (this repository has none).\nIts base is main, which Oby merges into only on your word.', options: ['Merge', 'Dismiss'] };
    const stages = [{ id: 'work', title: 'Work', state: 'done' }, { id: 'publish', title: 'Publish', state: 'current' }] as JobView['stages'];
    const { card, opened } = await views([], { ask: merge, stages, repo: 'acme/oby-scratch' }, 'needs');
    // title, "Needs you · Claude Code · oby-scratch", its stages with the one it waits on orange: nothing else
    expect(card).toContain('>Needs you · Claude Code · oby-scratch<');
    expect(card).toMatch(/aria-label="Stage 2 of 2: Publish, needs you"><span class="is-done"><\/span><span class="is-needs"><\/span>/);
    for (const word of ['Merge oby-scratch', 'No check ran', 'Its base is main', '>Merge<', '>Later<']) expect(card).not.toContain(word);
    // the decision waits over the thread's box, on one line, its holds behind Details
    expect(opened).toMatch(/class="decision-bar".*<span class="tc-ask-kind is-merge">Ready to merge<\/span><p class="tc-ask-head" title="Merge oby-scratch #4 into main\?">.*>Details<.*>Merge<.*>Dismiss<.*>Later</);
    expect(opened).not.toContain('No check ran');
  });

  it('LIVETASK-27: answered in its own thread, one line stays where it was asked: its kind and the answer', async () => {
    stub();
    const { default: TaskThread } = await import('../TaskThread');
    const { replaceTasks } = await import('../tasks');
    const { SESSIONS } = await import('../session');
    SESSIONS.j1 = [tool('Read', 'a.ts')];
    replaceTasks([{ ...await taskOf('j1', { ask }), answer: 'Allow' }]);
    const html = read(renderToStaticMarkup(<TaskThread id="j1" leaving={false} onBack={() => {}} onGone={() => {}} onOpenAgent={() => {}} />));
    expect(html).toMatch(/<p class="lt-answered"><svg.*<\/svg>Permission · Allow<\/p>/);
    expect(html).not.toContain('decision-bar');
    // put off (Later, from an earlier build or obid): no record, it still waits
    replaceTasks([{ ...await taskOf('j1', { ask }, 'needs'), answer: 'Later' }]);
    const waiting = read(renderToStaticMarkup(<TaskThread id="j1" leaving={false} onBack={() => {}} onGone={() => {}} onOpenAgent={() => {}} />));
    expect(waiting).not.toContain('lt-answered');
    expect(waiting).toMatch(/class="decision-bar".*>Allow<.*>Deny<.*>Later</);
  });

  it('LIVETASK-17: words in a card\'s box reach that job alone; the foot box\'s reach Oby, whatever card is open', async () => {
    const { replyTarget } = await import('../replyTarget');
    expect(replyTarget({ foot: { kind: 'oby', id: 'c1' } })).toStrictEqual({ oby: 'c1' });
    expect(replyTarget({ foot: { kind: 'oby', id: 'c1' } })).not.toHaveProperty('job');
    expect(replyTarget({ card: 'a' })).toStrictEqual({ job: 'a' });
    expect(replyTarget({ card: 'b' })).toStrictEqual({ job: 'b' });
    expect(replyTarget({ foot: { kind: 'task', id: 't1' } })).toStrictEqual({ job: 't1' });
    expect(replyTarget({ foot: { kind: 'peer', id: 'p1' } })).toStrictEqual({ peer: 'p1' });
  });

  it('LIVETASK-17: in a conversation with a card that needs you, its box talks to Oby alone; the task\'s thread\'s box to its job alone', async () => {
    const chats = await import('../chats'), live = await import('../live'), tasks = await import('../tasks');
    await conversation();
    // the conversation's one box
    expect(Object.keys(page.boxes)).toEqual(['Reply to Oby']);
    expect(page.boxes['Reply to Oby'].placeholder).toBe('Reply to Oby…');
    page.boxes['Reply to Oby'].onSend('hi', {});
    expect(chats.tellChat).toHaveBeenCalledTimes(1);
    expect(chats.tellChat).toHaveBeenCalledWith('c1', 'hi', {});
    expect(live.tellAgent).not.toHaveBeenCalled();
    expect(tasks.tell).not.toHaveBeenCalled();
    // the needing task's own thread: its job and nothing else, on obid and off it
    const { default: TaskThread } = await import('../TaskThread');
    renderToStaticMarkup(<TaskThread id="j2" leaving={false} onBack={() => {}} onGone={() => {}} onOpenAgent={() => {}} />);
    page.obid = true;
    page.boxes['Tell the agent about Run the tests'].onSend('go on', {});
    expect(live.tellAgent).toHaveBeenCalledTimes(1);
    expect(live.tellAgent).toHaveBeenCalledWith('j2', 'go on', undefined);
    page.obid = false;
    page.boxes['Tell the agent about Run the tests'].onSend('go on', {});
    expect(tasks.tell).toHaveBeenCalledTimes(1);
    expect(tasks.tell).toHaveBeenCalledWith('j2', 'go on', {});
    expect(chats.tellChat).toHaveBeenCalledTimes(1);
  });

  it('LIVETASK-18: the task\'s page opens from the card\'s menu, View details', async () => {
    stub();
    const { default: TaskMenu } = await import('../TaskMenu');
    const { ThreadTask } = await import('../ChatView');
    const task = await taskOf('j1');
    const menu = renderToStaticMarkup(<TaskMenu task={task} at={new DOMRect(0, 0, 0, 0)} onClose={() => {}} onViewDetails={() => {}} />);
    expect(menu).toContain('>View details<');
    expect(menu).toContain('>Stop task<');
    expect(menu).toContain('>Delete task<');
    expect(menu).not.toContain('Open conversation');
    expect(menu).not.toContain('Rename');
    // a right-click on the card: its menu, at the pointer
    const onMenu = vi.fn();
    const card = ThreadTask({ task, current: false, onOpen: () => {}, onMenu }) as React.ReactElement<{ children: React.ReactElement<{ onContextMenu: (e: object) => void }> }>;
    const preventDefault = vi.fn();
    card.props.children.props.onContextMenu({ preventDefault, clientX: 40, clientY: 60 });
    expect(preventDefault).toHaveBeenCalled();
    expect(onMenu).toHaveBeenCalledWith(expect.objectContaining({ x: 40, y: 60 }));
  });

  it('LIVETASK-10: a step reads as what it is doing, then as what it did', () => {
    const lines = sessionOf([
      wire(1, { summary: 'Reading a.ts', state: 'running' }), wire(2, { summary: 'Reading a.ts', state: 'done' }), wire(3, { summary: 'Running npm test', state: 'failed', exit: 1 }),
      wire(4, { summary: 'Running git push', state: 'declined' }), wire(5, { summary: 'Fix the timer', state: 'done' }),
    ]);
    // refused, it never ran
    expect(lines.map((l) => (l as Extract<Entry, { kind: 'tool' }>).verb)).toEqual(['Reading', 'Read', 'Ran', 'Running', 'Fix']);
    // refused isn't failed
    expect(lines[3]).toMatchObject({ meta: 'Declined', state: 'declined' });
    expect(lines[2]).toMatchObject({ state: 'failed' });
    expect(lastSteps(lines, 2)).toHaveLength(2);
  });
});
