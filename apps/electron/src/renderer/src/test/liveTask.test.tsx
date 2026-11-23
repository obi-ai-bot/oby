import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Job as JobView } from '../view';
import type { Entry } from '../session';
import { knownUpTo, withEntries, withLine } from '../liveTask';
import { parseEvent } from '../../../shared/obid-wire';
import { detailOf, sessionOf } from '../jobPage';
import type { TranscriptEntry } from '../obid';
import type { Chat } from '../chats';
import type { Task } from '../tasks';

// A task in its conversation (docs/contracts/live-task.md): its card, progress only, and what it did under
// it, in the conversation (TaskLog); the decisions over the message box.
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
vi.mock('../tasks', async (original) => ({ ...await original<typeof import('../tasks')>(), tell: vi.fn() }));
vi.mock('../lean', async (original) => ({ ...await original<typeof import('../lean')>(), get onObid() { return page.obid; } }));
vi.mock('../live', async (original) => ({ ...await original<typeof import('../live')>(), tellJob: vi.fn() }));
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
// the card, what it did under it (TaskLog), and the conversation it's in (c1, whose answer started it)
const WORKING = [{ id: 'prepare', title: 'Prepare', state: 'done' }, { id: 'work', title: 'Work', state: 'current' }, { id: 'check', title: 'Check', state: 'upcoming' }] as JobView['stages'];
async function views(entries: Entry[], more: Partial<JobView> = {}, status: 'running' | 'needs' | 'done' = 'running', extra: Partial<Task> = {}) {
  stub();
  const { default: ChatView, ThreadTask } = await import('../ChatView');
  const { default: TaskLog } = await import('../TaskLog');
  const { SESSIONS } = await import('../session');
  const { replaceTasks } = await import('../tasks');
  SESSIONS.j1 = entries;
  const task = { ...await taskOf('j1', more, status), ...extra };
  replaceTasks([task]);
  page.chats = [{ id: 'c1', title: 'One fix', said: [{ who: 'you', text: 'fix it', ago: 5 }, { who: 'oby', text: 'On it.', ago: 4, jobs: ['j1'] }] } satisfies Chat];
  const none = () => {};
  return {
    card: read(renderToStaticMarkup(<ThreadTask task={task} />)),
    log: read(renderToStaticMarkup(<TaskLog task={task} />)),
    convo: read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={none} onGone={none} />)),
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
  return read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={none} onGone={none} />));
}
// one card's own part of a conversation's markup, up to what it did, the next card or the message's time
const cardOf = (html: string, title: string) => html.slice(html.indexOf(`<div class="msg-task" role="group" aria-label="${title}"`)).split(/class="lt-log"|class="msg-task-work|class="msg-time"/)[0];
// what a task did, as the conversation shows it under its card
const logOf = (html: string, title: string) => html.slice(html.indexOf(`aria-label="${title}"`)).split(/class="lt-log"/)[1]?.split(/class="msg-task-work|class="msg-time"/)[0] ?? '';
const tool = (verb: string, target: string, state: 'running' | 'done' | 'failed' = 'done'): Entry => ({ kind: 'tool', tool: 'Read', verb, target, state, seconds: 1 });
const plan: Entry = { kind: 'plan', todos: [{ text: 'Read the test', state: 'done' }, { text: 'Replace the timer', state: 'running' }, { text: 'Run the check', state: 'pending' }] };
const wire = (n: number, more: Partial<TranscriptEntry> = {}): TranscriptEntry => ({ n, at: '2026-11-02T10:00:00Z', kind: 'tool', summary: 'Reading a.ts', state: 'running', ...more });

describe('a task at work', () => {
  beforeEach(() => { page.boxes = {}; page.obid = false; vi.clearAllMocks(); });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('LIVETASK-1: the card says how it stands and its stages; under it, its newest list with the one it is on, and its last lines', async () => {
    const { card, log } = await views([{ kind: 'said', text: 'Looking.', at: '1:50 PM' }, tool('Read', 'a.ts'), tool('Read', 'b.ts'), plan, { kind: 'thought', text: '', seconds: 6 }, tool('Editing', 'session.ts', 'running')], { step: 'Editing session.ts', stages: WORKING });
    expect(card).toContain('>Running · Claude Code · obi<');
    expect(card).toMatch(/tc-progress msg-task-stages" role="img" aria-label="Stage 2 of 3: Work"><span class="is-done"><\/span><span class="is-current"><\/span><span><\/span>/);
    // the card holds no list and no line: what it did is under it
    for (const word of ['Replace the timer', 'session.ts', 'ss-entries', 'sheen']) expect(card).not.toContain(word);
    expect(log).toMatch(/ss-plan-head">To do · 1 of 3 done<.*is-done.*Read the test.*is-running.*Replace the timer.*is-pending.*Run the check/);
    // its last four lines, the one before them a click away
    expect(log).toContain('>Show 1 more<');
    expect(log).not.toContain('Looking.');
    for (const word of ['a.ts', 'b.ts', 'Thought for 6s', 'session.ts']) expect(log).toContain(word);
    // what it was told to start with is Oby's answer above it already; what it's told since shows
    const told = (await views([{ kind: 'you', text: 'The whole prompt', at: '1:49 PM' }, tool('Read', 'a.ts'), { kind: 'you', text: 'go on', at: '1:52 PM' }])).log;
    expect(told).not.toContain('The whole prompt');
    expect(told).toContain('go on');
  });

  it('LIVETASK-2: with no list from the agent it shows only its lines, and invents no to-do', async () => {
    const { card, log } = await views([tool('Read', 'a.ts'), tool('Running', 'npm test', 'running')], { step: 'Running npm test' });
    expect(log).not.toContain('ss-plan');
    expect(card).not.toContain('npm test');
    expect(log).toContain('npm test');
    // and before it has done anything, what obid says it's on; nothing said, nothing under the card
    expect((await views([], { step: 'Waiting for a free slot' })).log).toContain('sheen">Waiting for a free slot<');
    expect((await views([], { step: '', waiting: undefined })).log).toBe('');
  });

  it('RECOVERY-5: a job being tried again says so, under the steps it already has', async () => {
    const trying = 'Trying again in 5 minutes (2 of 3)';
    const reading = { ...tool('Read', 'a.ts'), summary: 'Reading a.ts' } as Entry;
    expect((await views([reading], { step: trying })).log).toMatch(/a\.ts.*Trying again in 5 minutes \(2 of 3\)/);
    expect((await views([reading, plan], { step: trying })).log).toContain(trying);
    // under the card, what it's on is said once
    const times = (html: string, text: string) => html.split(text).length - 1;
    expect(times((await views([reading], { step: 'Reading a.ts' })).log, 'Reading a.ts')).toBe(0);
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
    expect((await views([plan], { status: 'stopped' }, 'done')).card).toContain('>Stopped · Claude Code');
    expect((await views([plan], { status: 'failed' }, 'done')).card).toContain('>Failed · Claude Code');
  });

  it('LIVETASK-4: done with its pull request open, the card says the stage it is at in obid\'s words; merged, it says so', async () => {
    const stage = (review: string, merged: string, detail?: string) => [
      { id: 'publish', title: 'Publish', state: 'done', detail: 'Secret scan clean · pushed · pull request opened' },
      { id: 'review', title: 'Review', state: review, detail: review === 'current' ? detail : 'Approved by Oby' },
      { id: 'merged', title: 'Merged into dev', state: merged, ...(merged === 'current' ? { detail } : {}) },
    ] as JobView['stages'];
    const pr = { commits: 1, published: 'https://github.com/deskree/obi/pull/9', finished: 1 };
    const reviewing = (await views([plan], { ...pr, stages: stage('current', 'upcoming', 'Oby is reviewing') }, 'done')).card;
    expect(reviewing).toContain('>Oby is reviewing<');
    const open = (await views([plan], { ...pr, stages: stage('done', 'current', 'Pull request open · waiting for your review on GitHub') }, 'done')).card;
    expect(open).toContain('>Pull request open · waiting for your review on GitHub<');
    expect(open).not.toContain('Merged into dev');
    expect((await views([plan], { ...pr, stages: stage('done', 'done') }, 'done')).card).toContain('>Merged into dev<');
    // no pull request: nothing of the kind
    expect((await views([plan], { commits: 1, finished: 1, stages: stage('done', 'current', 'Merge it on GitHub') }, 'done')).card).not.toContain('Merge it on GitHub');
  });

  it('LIVETASK-31: ended, its last words show under the card (what changed, the test run, the commit), its steps folded behind Show more', async () => {
    const result = 'Changes\n- Added divide(a, b)\n\nTest run\n- 4 passed\n\nCommit abc123\n\nGOAL MET';
    const { card, log } = await views([plan, tool('Read', 'a.ts'), tool('Edited', 'math.ts'), { kind: 'said', text: result, at: '2:10 PM' }], {}, 'done');
    expect(log).toContain('>Show 2 more<');
    for (const word of ['a.ts', 'math.ts', 'ss-plan']) expect(log).not.toContain(word);
    const text = log.replace(/<[^>]+>/g, '');
    for (const word of ['Added divide(a, b)', '4 passed', 'Commit abc123', 'GOAL MET']) expect(text).toContain(word);
    // long words are cut to a few lines with Show more (lt-fold), and none of it is on the card
    expect(log).toContain('class="lt-fold');
    expect(card).not.toContain('GOAL MET');
  });

  it('LIVETASK-5: what it did shows in the conversation under its card, outside it, with the box talking to Oby', async () => {
    const html = await conversation();
    const card = cardOf(html, 'Fix the login test');
    expect(card).not.toContain('ss-entries');
    expect(card).not.toContain('placeholder=');
    expect(card).not.toContain('aria-expanded');
    expect(logOf(html, 'Fix the login test')).toContain('a.ts');
    expect(logOf(html, 'Run the tests').replace(/<[^>]+>/g, '')).toContain('It needs the tests run.');
    // its card and what it did sit together, where opening the conversation at it scrolls to
    expect(html).toMatch(/<div class="msg-task-work is-live" data-task="j1"><div class="msg-task" role="group" aria-label="Fix the login test">.*<\/div><div class="lt-log"/);
    expect(Object.keys(page.boxes)).toEqual(['Reply to Oby']);
  });

  it('LIVETASK-5: words sent once to steer it show once, as the owner\'s message; words told it from elsewhere stay in its lines', async () => {
    stub();
    const { default: ChatView } = await import('../ChatView');
    const { replaceTasks } = await import('../tasks');
    const { SESSIONS } = await import('../session');
    const steer = 'For that task: also end the line with a period.';
    // the agent's transcript holds the words obid told it, between its steps
    SESSIONS.j1 = [tool('Read', 'a.ts'), { kind: 'you', text: steer, at: '1:51 PM' }, { kind: 'you', text: 'Keep the title short.', at: '1:52 PM' }, tool('Edited', 'b.ts')];
    replaceTasks([await taskOf('j1')]);
    page.chats = [{ id: 'c1', title: 'One fix', said: [
      { who: 'you', text: 'fix it', ago: 5 }, { who: 'oby', text: 'On it.', ago: 4, jobs: ['j1'] },
      { who: 'you', text: steer, ago: 2 }, { who: 'oby', text: 'I told it to end the line with a period.', ago: 1 },
    ] } satisfies Chat];
    const none = () => {};
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={none} onGone={none} />));
    expect(html.split(`>${steer}<`).length - 1).toBe(1);
    expect(logOf(html, 'Fix the login test')).not.toContain(steer);
    // what the thread doesn't show (told it from its card, on another device) stays among its lines
    expect(logOf(html, 'Fix the login test')).toContain('>Keep the title short.<');
  });

  it('LIVETASK-6: thinking shows only what its tool gave', async () => {
    const thought = (more: Partial<TranscriptEntry>) => sessionOf([wire(1, { kind: 'thought', summary: 'Thinking', state: undefined, ...more })])[0] as Extract<Entry, { kind: 'thought' }>;
    const { log } = await views([thought({ state: 'running' }), thought({ state: 'done', seconds: 8 }), thought({ state: 'done', text: 'Only the project file.' })]);
    expect(log).toContain('sheen">Thinking');
    expect(log).toContain('<span>Thought for 8s</span>');
    expect(log).toContain('<p>Only the project file.</p>');
    expect(log).not.toContain('Thought for 0');
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

  it('LIVETASK-9: waiting on you, the card says so with no sheen and no decision; the decision waits over the box', async () => {
    const { card, convo } = await views([plan], { ask }, 'needs');
    expect(card).toContain('>Needs you · Claude Code');
    expect(card).toMatch(/^<div class="msg-task" role="group" aria-label="Fix the login test">.*status-dot is-needs/);
    expect(card).not.toContain('sheen');
    expect(card).not.toContain('tc-ask');
    expect(convo).toMatch(/<\/section>.*class="decision-bar".*Run npm test\?.*>Allow<.*>Deny<.*placeholder="Reply to Oby…"/);
  });

  it('LIVETASK-15: a card goes with its job, its decision too', async () => {
    expect(await conversation()).toContain('class="decision-bar"');
    const gone = await conversation(['j1']);
    expect(gone).not.toContain('Run the tests');
    expect(gone).not.toContain('decision-bar');
  });

  it('LIVETASK-16: no card shows a decision, at work or waiting on you, and nothing opens in place', async () => {
    const html = await conversation();
    expect(cardOf(html, 'Run the tests')).not.toContain('tc-ask');
    expect(cardOf(html, 'Fix the login test')).not.toContain('tc-ask');
    expect(html).not.toContain('aria-expanded="true"');
    expect(html).not.toContain('is-link');
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
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={() => {}} onGone={() => {}} />));
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
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={() => {}} onGone={() => {}} />));
    expect(html).toMatch(/class="decision-bar".*Run npm test\?<span class="tc-ask-from"> · Run the tests/);
    // another conversation's task isn't this one's
    page.chats = [{ id: 'c2', title: 'Other', said: [] } satisfies Chat];
    const other = read(renderToStaticMarkup(<ChatView kind="oby" id="c2" leaving={false} onBack={() => {}} onGone={() => {}} />));
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
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={() => {}} onGone={() => {}} />));
    const bar = html.slice(html.indexOf('class="decision-bar"'), html.indexOf('<input'));
    expect(bar).toMatch(/The routine failed<span class="tc-ask-from"> · Nightly/);
    expect(bar).not.toContain('decision-count');
    for (const word of ['Another one failed', 'GitHub wants you', 'Share it?']) expect(bar).not.toContain(word);
    replaceTasks([], { work: [], asks: [] });
  });

  it('LIVETASK-25: the card stays lean: its head alone, whatever it waits on, while the decision waits over the box', async () => {
    const merge = { kind: 'merge' as const, label: 'Ready to merge', headline: 'Merge oby-scratch #4 into main?', text: 'No check ran on its head (this repository has none).\nIts base is main, which Oby merges into only on your word.', options: ['Merge', 'Dismiss'] };
    const stages = [{ id: 'work', title: 'Work', state: 'done' }, { id: 'publish', title: 'Publish', state: 'current' }] as JobView['stages'];
    const { card, convo } = await views([], { ask: merge, stages, repo: 'acme/oby-scratch' }, 'needs');
    // title, "Needs you · Claude Code · oby-scratch", its stages with the one it waits on orange: nothing else
    expect(card).toContain('>Needs you · Claude Code · oby-scratch<');
    expect(card).toMatch(/aria-label="Stage 2 of 2: Publish, needs you"><span class="is-done"><\/span><span class="is-needs"><\/span>/);
    for (const word of ['Merge oby-scratch', 'No check ran', 'Its base is main', '>Merge<', '>Later<']) expect(card).not.toContain(word);
    // the decision waits over the conversation's box, on one line, its holds behind Details
    expect(convo).toMatch(/class="decision-bar".*<span class="tc-ask-kind is-merge">Ready to merge<\/span><p class="tc-ask-head" title="Merge oby-scratch #4 into main\?">.*>Details<.*>Merge<.*>Dismiss<.*>Later</);
    expect(convo).not.toContain('No check ran');
  });

  it('LIVETASK-27: answered, one line stays under its card where it was asked: its kind and the answer', async () => {
    const { log, convo } = await views([tool('Read', 'a.ts')], { ask }, 'running', { answer: 'Allow' });
    expect(log).toMatch(/<p class="lt-answered"><svg.*<\/svg>Permission · Allow<\/p>/);
    expect(convo).not.toContain('decision-bar');
    // put off (Later, from an earlier build or obid): no record, it still waits
    const waiting = await views([tool('Read', 'a.ts')], { ask }, 'needs', { answer: 'Later' });
    expect(waiting.log).not.toContain('lt-answered');
    expect(waiting.convo).toMatch(/class="decision-bar".*>Allow<.*>Deny<.*>Later</);
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

  it('LIVETASK-17: in a conversation with a card that needs you, its box talks to Oby alone; a task\'s own thread\'s box to its job alone', async () => {
    const chats = await import('../chats'), tasks = await import('../tasks');
    await conversation();
    // the conversation's one box
    expect(Object.keys(page.boxes)).toEqual(['Reply to Oby']);
    expect(page.boxes['Reply to Oby'].placeholder).toBe('Reply to Oby…');
    page.boxes['Reply to Oby'].onSend('hi', {});
    expect(chats.tellChat).toHaveBeenCalledTimes(1);
    expect(chats.tellChat).toHaveBeenCalledWith('c1', 'hi', {});
    expect(tasks.tell).not.toHaveBeenCalled();
    // a task in no conversation (from GitHub): its own thread's box tells its job and nothing else
    const { default: ChatView } = await import('../ChatView');
    renderToStaticMarkup(<ChatView kind="task" id="j2" leaving={false} onBack={() => {}} onGone={() => {}} />);
    page.boxes['Tell the agent about Run the tests'].onSend('go on', {});
    expect(tasks.tell).toHaveBeenCalledTimes(1);
    expect(tasks.tell).toHaveBeenCalledWith('j2', 'go on', {});
    expect(chats.tellChat).toHaveBeenCalledTimes(1);
  });

  it('LIVETASK-22: in a task\'s own thread, all its session in order: what it was told, its lists, its words where it said them', async () => {
    const said = (text: string): Entry => ({ kind: 'said', text, at: '1:50 PM' });
    await views([{ kind: 'you', text: 'Fix the flaky login test', at: '1:40 PM' } as Entry, plan, tool('Read', 'a.ts'), said('Found the timer.'), tool('Edited', 'b.ts')], { finished: 1 }, 'done');
    const { default: TaskLog } = await import('../TaskLog');
    const task = await taskOf('j1', { finished: 1 }, 'done');
    const whole = read(renderToStaticMarkup(<TaskLog task={task} whole />));
    // (the agent's words, a word to a span)
    const at = ['Fix the flaky login test', 'Replace the timer', 'a.ts', '>timer.<', 'b.ts'].map((w) => whole.indexOf(w));
    expect(at.every((i) => i >= 0), `each shows: ${at}`).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    // in the conversation, as before: what it was told and its list left out, its last words its result, last
    const compact = read(renderToStaticMarkup(<TaskLog task={task} />));
    expect(compact).not.toContain('Fix the flaky login test');
    expect(compact).toContain('>timer.<');
    expect(compact).toContain('>Show 2 more<');
    expect(compact).not.toContain('Replace the timer');
  });

  it('LIVETASK-22: in a task\'s own thread, the words it was started with show once (the thread\'s first message); other words told it first still show', async () => {
    stub();
    const { default: TaskLog } = await import('../TaskLog');
    const { SESSIONS } = await import('../session');
    const task = await taskOf('j1');
    const count = (html: string, w: string) => html.split(w).length - 1;
    // its session opens with the round's start and the request the thread already shows as its first message
    SESSIONS.j1 = [{ kind: 'start', text: 'Started on Claude Code', at: '1:40 PM' }, { kind: 'you', text: 'Fix the flaky login test', at: '1:40 PM' }, tool('Read', 'a.ts'), { kind: 'you', text: 'Fix the flaky login test', at: '1:45 PM' }];
    const once = read(renderToStaticMarkup(<TaskLog task={task} whole first="Fix the flaky login test" />));
    expect(once).toContain('Started on Claude Code');
    expect(once).toContain('a.ts');
    // the same words told again later stay, where they were said
    expect(count(once, 'Fix the flaky login test')).toBe(1);
    expect(once.indexOf('Fix the flaky login test')).toBeGreaterThan(once.indexOf('a.ts'));
    // a different first message (it came in from GitHub, its prompt its own): the session's words show
    const other = read(renderToStaticMarkup(<TaskLog task={task} whole first="Issue #140: flaky login" />));
    expect(count(other, 'Fix the flaky login test')).toBe(2);
  });

  it('LIVETASK-22: told something in a finished task\'s own thread, the follow-up obid starts is opened (onFollowUp); words for a running one start none', async () => {
    stub();
    const live = await import('../live');
    const { default: ChatView } = await import('../ChatView');
    const { replaceTasks } = await import('../tasks');
    replaceTasks([await taskOf('j1', { finished: 1 }, 'done')]);
    const followed = vi.fn();
    renderToStaticMarkup(<ChatView kind="task" id="j1" leaving={false} onBack={() => {}} onGone={() => {}} onFollowUp={followed} />);
    page.obid = true;
    vi.mocked(live.tellJob).mockResolvedValueOnce({ id: 'j9' } as never);
    await page.boxes['Tell the agent about Fix the login test'].onSend('and the docs', {});
    expect(live.tellJob).toHaveBeenCalledWith('j1', 'and the docs', undefined, true, undefined);
    expect(followed).toHaveBeenCalledWith('j9');
    // obid answered with the task itself (no follow-up): nothing to open
    vi.mocked(live.tellJob).mockResolvedValueOnce({ id: 'j1' } as never);
    await page.boxes['Tell the agent about Fix the login test'].onSend('thanks', {});
    expect(followed).toHaveBeenCalledTimes(1);
    // stopped at its safety limit (interrupted, its Confirm showing: needs you): obid decides, and its
    // follow-up opens all the same (RECOVERY-18)
    const box = async (status: 'running' | 'needs', more: Partial<JobView>) => {
      page.obid = false;
      replaceTasks([await taskOf('j1', more, status)]);
      renderToStaticMarkup(<ChatView kind="task" id="j1" leaving={false} onBack={() => {}} onGone={() => {}} onFollowUp={followed} />);
      page.obid = true;
      return page.boxes['Tell the agent about Fix the login test'];
    };
    vi.mocked(live.tellJob).mockResolvedValueOnce({ id: 'j10' } as never);
    await (await box('needs', { status: 'interrupted' })).onSend('go on past it', {});
    expect(followed).toHaveBeenCalledTimes(2);
    expect(followed).toHaveBeenLastCalledWith('j10');
    // running: its words reach it, obid answers with the task itself, and nothing opens
    vi.mocked(live.tellJob).mockResolvedValueOnce({ id: 'j1' } as never);
    await (await box('running', {})).onSend('also the docs', {});
    expect(live.tellJob).toHaveBeenLastCalledWith('j1', 'also the docs', undefined, true, undefined);
    expect(followed).toHaveBeenCalledTimes(2);
  });

  it('LIVETASK-29: the card is progress only: its ⋯ and a right-click open its menu, which tells its agent or stops it', async () => {
    stub();
    const { default: TaskMenu } = await import('../TaskMenu');
    const { ThreadTask } = await import('../ChatView');
    const task = await taskOf('j1');
    const onMenu = vi.fn();
    const html = read(renderToStaticMarkup(<ThreadTask task={task} onMenu={onMenu} />));
    expect(html).toMatch(/<button type="button" class="icon-button msg-task-more" aria-label="More for Fix the login test" aria-haspopup="menu">/);
    // no link and no chevron: a click on it opens nothing
    expect(html).not.toContain('is-link');
    expect(html).not.toContain('msg-task-go');
    // a right-click: its menu, at the pointer
    const card = ThreadTask({ task, onMenu }) as React.ReactElement<{ onContextMenu: (e: object) => void }>;
    const preventDefault = vi.fn();
    card.props.onContextMenu({ preventDefault, clientX: 40, clientY: 60 });
    expect(preventDefault).toHaveBeenCalled();
    expect(onMenu).toHaveBeenCalledWith(expect.objectContaining({ x: 40, y: 60 }));
    // its menu: tell its agent, Stop, Delete; never a page of its own
    const menu = renderToStaticMarkup(<TaskMenu task={task} at={new DOMRect(0, 0, 0, 0)} onClose={() => {}} onTell={() => {}} />);
    expect(menu).toMatch(/>Tell Claude Code…<.*>Stop task<.*>Delete task</);
    expect(menu).not.toContain('View details');
    expect(menu).not.toContain('Open conversation');
    // ended, telling it asks for a follow-up
    const done = renderToStaticMarkup(<TaskMenu task={await taskOf('j1', {}, 'done')} at={new DOMRect(0, 0, 0, 0)} onClose={() => {}} onTell={() => {}} />);
    expect(done).toContain('>Ask for a follow-up…<');
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
  });
});

// Reading a reply in the thread (docs/contracts/chat-first.md, CHATFIRST-37 and 41): where its
// controls are; what they do is replies.test.tsx
describe('reading a reply in the thread', () => {
  beforeEach(() => { page.boxes = {}; page.obid = false; vi.clearAllMocks(); });
  afterEach(() => { vi.unstubAllGlobals(); });
  const copies = (html: string) => html.match(/aria-label="Copy"/g)?.length ?? 0;

  it('CHATFIRST-37: Oby\'s reply and its task\'s agent\'s words each have their Copy; yours and words still coming in have none', async () => {
    const { log, convo } = await views([tool('Read', 'a.ts'), { kind: 'said', text: 'All **done**.', at: '2:10 PM' }], {}, 'done');
    expect(log).toMatch(/<li class="ss-said">.*<strong>.*done.*<\/strong>.*<button type="button" class="reply-copy msg-copy" aria-label="Copy"/);
    // Oby's "On it." and the agent's last words under its card; "fix it" is yours
    expect(copies(convo)).toBe(2);
    // Oby's words end with a task's card: their Copy is under the whole message, past the card and
    // its time, never inside the words, where it would sit on the card's corner
    expect(convo).toMatch(/<div class="msg-bubble"><p>.*On.*it\..*<\/p><\/div>/);
    // (this message alone: from its words to the next one; the agent's own Copy is inside the card)
    const rest = convo.slice(convo.indexOf('<div class="msg-bubble"><p><span class="w">On</span>'));
    const mine = rest.slice(0, rest.indexOf('role="listitem"') > 0 ? rest.indexOf('role="listitem"') : undefined);
    const card = mine.indexOf('class="msg-task'), time = mine.indexOf('class="msg-time"'), copy = mine.lastIndexOf('class="reply-copy msg-copy"');
    expect(card).toBeGreaterThan(-1);
    expect(time).toBeGreaterThan(card);
    expect(copy).toBeGreaterThan(time);
    const { default: ChatView } = await import('../ChatView');
    page.chats = [{ id: 'c1', title: 'One fix', said: [{ who: 'you', text: 'fix it', ago: 5 }, { who: 'oby', text: 'Halfway th', ago: 0, streaming: true }] } satisfies Chat];
    const none = () => {};
    expect(copies(read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={none} onGone={none} />)))).toBe(0);
  });

  it('CHATFIRST-41: a picture that went with a message is a button that shows it full size', async () => {
    stub();
    const { default: ChatView } = await import('../ChatView');
    page.chats = [{ id: 'c1', title: 'A picture', said: [{ who: 'you', text: 'look', ago: 5, files: [{ name: 'shot.png', preview: 'data:image/png;base64,AA==' }, { name: 'notes.txt' }] }] } satisfies Chat];
    const none = () => {};
    const html = read(renderToStaticMarkup(<ChatView kind="oby" id="c1" leaving={false} onBack={none} onGone={none} />));
    expect(html).toContain('<button type="button" class="photo-open" aria-label="Show shot.png full size"><img class="msg-photo" src="data:image/png;base64,AA==" alt="shot.png"/></button>');
    expect(html).not.toContain('aria-label="Show notes.txt full size"');
  });
});
