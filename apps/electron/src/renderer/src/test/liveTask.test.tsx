import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Job as JobView } from '../mock';
import type { Entry } from '../session';
import { around, knownUpTo, lastSteps, thoughtWords, withEntries, withLine } from '../liveTask';
import { parseEvent } from '../../../shared/obid-wire';
import { detailOf, sessionOf } from '../jobPage';
import type { TranscriptEntry } from '../obid';

// A task's live card and its agent's own conversation (docs/contracts/live-task.md)
vi.mock('../Composer', () => ({ default: ({ placeholder }: { placeholder: string }) => <input placeholder={placeholder} />, typingIn: () => false }));
const read = (html: string) => html.replaceAll('<!-- -->', '');
async function views(entries: Entry[], more: Partial<JobView> = {}, status: 'running' | 'needs' | 'done' = 'running') {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', {});
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('performance', { now: () => 0 });
  const { ThreadTask } = await import('../ChatView');
  const { default: AgentPane } = await import('../AgentPane');
  const { SESSIONS } = await import('../session');
  const { replaceTasks } = await import('../tasks');
  const { AGENTS } = await import('../mock');
  SESSIONS.j1 = entries;
  const job = {
    id: 'j1', type: 'issues', title: 'Fix the login test', request: 'Fix it', agent: 'claude', account: 'Personal', model: 'Opus',
    repo: 'deskree/obi', branch: 'oby/fix-1', base: 'dev', status: status === 'done' ? 'done' : 'running', step: 'Working', stages: [], created: 12, ...more,
  } as JobView;
  const task = { id: 'j1', type: 'issues', title: job.title, brand: AGENTS.claude.brand, status, job } as never;
  replaceTasks([task]);
  return {
    card: read(renderToStaticMarkup(<ThreadTask task={task} current={false} onOpen={() => {}} />)),
    pane: read(renderToStaticMarkup(<AgentPane id="j1" onClose={() => {}} onDetails={() => {}} onFollowUp={() => {}} />)),
  };
}
const tool = (verb: string, target: string, state: 'running' | 'done' | 'failed' = 'done'): Entry => ({ kind: 'tool', tool: 'Read', verb, target, state, seconds: 1 });
const plan: Entry = { kind: 'plan', todos: [{ text: 'Read the test', state: 'done' }, { text: 'Replace the timer', state: 'running' }, { text: 'Run the check', state: 'pending' }] };
const wire = (n: number, more: Partial<TranscriptEntry> = {}): TranscriptEntry => ({ n, at: '2026-11-02T10:00:00Z', kind: 'tool', summary: 'Reading a.ts', state: 'running', ...more });

describe('a task at work', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it('LIVETASK-1: the card shows its to-dos with the one it is on lit, and the last things it did', async () => {
    const { card } = await views([{ kind: 'said', text: 'Looking.', at: '1:50 PM' }, tool('Read', 'a.ts'), tool('Read', 'b.ts'), plan, { kind: 'thought', text: '', seconds: 6 }, tool('Editing', 'session.ts', 'running')]);
    expect(card).toContain('Running · Claude Code · Personal · obi · 12m');
    expect(card).toContain('What it’s doing');
    expect(card).toMatch(/is-done.*Read the test/);
    expect(card).toMatch(/is-on is-working.*sheen">Replace the timer/);
    expect(card).toMatch(/is-up.*Run the check/);
    // the last three, under the one it's on, the one at work with its working dot
    expect(card).toContain('b.ts');
    expect(card).toContain('Thought for 6s');
    expect(card).toMatch(/lt-tool is-running.*Editing.*session\.ts/);
    expect(card).not.toContain('a.ts');
  });

  it('LIVETASK-1: a long list on a card shows the part around the one it is on', () => {
    const todos = Array.from({ length: 20 }, (_, i) => ({ text: `T${i}`, state: i < 10 ? 'done' as const : i === 10 ? 'running' as const : 'pending' as const }));
    const { todos: shown, before, after } = around(todos);
    expect(shown.map((t) => t.text)).toEqual(['T8', 'T9', 'T10', 'T11', 'T12', 'T13', 'T14', 'T15']);
    expect([before, after]).toEqual([8, 4]);
    expect(around(todos.slice(0, 5)).before).toBe(0);
  });

  it('LIVETASK-2: with no list from the agent it shows only its steps, and invents no to-do', async () => {
    const { card, pane } = await views([tool('Read', 'a.ts'), tool('Running', 'npm test', 'running')]);
    expect(card).not.toContain('What it’s doing');
    expect(pane).not.toContain('agent-pane-todos');
    expect(card).toContain('npm test');
    // and before it has done anything, what obid says it's on
    expect((await views([], { step: 'Waiting for a free slot' })).card).toContain('Waiting for a free slot');
    expect((await views([], { step: '', waiting: undefined })).card).not.toContain('lt-now');
  });

  it('RECOVERY-5: a job being tried again says so, under the steps it already has', async () => {
    const trying = 'Trying again in 5 minutes (2 of 3)';
    const reading = { ...tool('Read', 'a.ts'), summary: 'Reading a.ts' } as Entry;
    expect((await views([reading], { step: trying })).card).toContain(trying);
    expect((await views([reading, plan], { step: trying })).card).toContain(trying);
    // what its session holds isn't said twice, nor that it's working
    const line = (card: string) => card.includes('lt-thought');
    expect(line((await views([reading], { step: 'Reading a.ts' })).card)).toBe(false);
    expect(line((await views([reading], { step: 'Working' })).card)).toBe(false);
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
    expect(card).toContain('>Done · Claude Code · Personal');
    expect(card).toContain('2 commits on oby/fix-1');
    expect(card).toContain('obi#9');
    expect(card).not.toContain('lt-now');
    expect((await views([plan], { status: 'stopped' }, 'done')).card).toContain('>Stopped · Claude Code');
    expect((await views([plan], { status: 'failed' }, 'done')).card).toContain('>Failed · Claude Code');
  });

  it('LIVETASK-5: its own conversation has its words, steps, thinking and to-dos, and a box to tell it', async () => {
    const { pane } = await views([
      { kind: 'start', text: 'Started on Claude Code', at: '1:50 PM' }, { kind: 'you', text: 'Fix the login test', at: '1:50 PM' }, tool('Read', 'a.ts'),
      { kind: 'said', text: 'It races a timer.', at: '1:53 PM' }, plan, { kind: 'thought', text: '', seconds: 8 }, tool('Editing', 'session.ts', 'running'),
    ]);
    expect(pane).toContain('aria-label="Conversation with Claude Code"');
    expect(pane).toContain('Fix the login test');
    expect(pane.replace(/<[^>]+>/g, '')).toContain('It races a timer.');
    expect(pane).toContain('1:53 PM');
    expect(pane).toContain('Thought for 8s');
    expect(pane).toContain('session.ts');
    expect(pane).toMatch(/agent-pane-todos.*Replace the timer/);
    // the list is on top, not a line of the conversation as well
    expect(pane).not.toContain('ss-plan');
    expect(pane).toContain('placeholder="Tell Claude Code…"');
    expect(pane).toContain('>Stop<');
    expect(pane).toContain('aria-label="Close this conversation"');
    expect(pane).toContain('aria-label="Open the task’s page"');
    // finished: no Stop, and what the box does then
    const done = (await views([tool('Read', 'a.ts')], {}, 'done')).pane;
    expect(done).not.toContain('>Stop<');
    expect(done).toContain('placeholder="Ask for a follow-up…"');
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
    const { pane } = await views([thought({ state: 'running' }), thought({ state: 'done', seconds: 8 }), thought({ state: 'done', text: 'Only the project file.' })]);
    expect(pane).toContain('sheen">Thinking');
    expect(pane).toContain('<span>Thought for 8s</span>');
    expect(pane).toContain('<p>Only the project file.</p>');
    expect(pane).not.toContain('Thought for 0');
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
    const { pane, card } = await views(SESSIONS.t1);
    expect(card).toContain('Fix the flags route');
    expect(pane).toContain('Thought for 6s');
  });

  it('LIVETASK-8: in a narrow window it lies over the chat, beside it otherwise', () => {
    const css = readFileSync(new URL('../liveTask.css', import.meta.url), 'utf8');
    expect(css).toMatch(/\.has-agent-pane > \.chat-scroll[^{]*\{ margin-right: var\(--agent-pane\); \}/);
    expect(css).toMatch(/@container \(max-width: 760px\) \{\s*\.agent-pane \{ width: 100%;/);
  });

  it('LIVETASK-9: waiting on you, the card says for what and the conversation carries the decision', async () => {
    const ask = { kind: 'permission' as const, headline: 'Run npm test?', text: 'It wants to run the tests.', options: ['Allow', 'Deny'] };
    const { card, pane } = await views([plan], { ask }, 'needs');
    expect(card).toContain('>Needs you · Claude Code');
    expect(card).toContain('Run npm test?');
    expect(card).toMatch(/is-on is-needs.*status-dot is-needs|status-dot is-needs.*Replace the timer/);
    expect(card).not.toMatch(/sheen">Replace the timer/);
    expect(pane).toContain('Allow');
    expect(pane).not.toContain('>Stop<');
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
