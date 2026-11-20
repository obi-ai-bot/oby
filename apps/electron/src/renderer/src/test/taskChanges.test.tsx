// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Job, JobChanges } from '../obid';

// A task's Changes panel and its menu's Undo this round… (docs/contracts/task-changes.md, CHANGES-4,
// 5 and 14), rendered for real on obid's answers stood in for: the page shows what obid read and
// asks again for a round or with whitespace left out; it never signs an Undo itself.

// a menu's portal: its items, in place
vi.mock('../Menu', () => ({ Menu: ({ children }: { children: React.ReactNode }) => <div role="menu">{children}</div> }));
vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));

const DIFF = [
  'diff --git a/hello.txt b/hello.txt', 'new file mode 100644', 'index 0000000..e965047', '--- /dev/null', '+++ b/hello.txt', '@@ -0,0 +1 @@', '+Hello',
  'diff --git a/second.txt b/second.txt', 'new file mode 100644', 'index 0000000..f00c965', '--- /dev/null', '+++ b/second.txt', '@@ -0,0 +1 @@', '+Second', '',
].join('\n');
const ROUNDS = [
  { round: 1, job: 'j0', from: 'a'.repeat(40), to: 'b'.repeat(40), at: '2026-11-17T10:00:00Z' },
  { round: 2, job: 'j1', from: 'b'.repeat(40), to: 'c'.repeat(40), at: '2026-11-17T10:05:00Z' },
];
const WHOLE: JobChanges = {
  base: 'main', head: 'c'.repeat(40), truncated: false, diff: DIFF, round: null, rounds: ROUNDS,
  files: [{ path: 'hello.txt', status: 'added', additions: 1, deletions: 0 }, { path: 'second.txt', status: 'added', additions: 1, deletions: 0 }],
};
const SECOND: JobChanges = { ...WHOLE, round: 2, diff: DIFF.slice(DIFF.indexOf('diff --git a/second')), files: [WHOLE.files[1]] };
const UNDO = 'oby/build-j1 goes back to bbbbbbb, as it was before this round: its 1 commit leaves the branch. Nothing is pushed.';

type Answer = { status: number; body?: unknown };
let answers: Record<string, Answer> = {};
let sent: string[] = [];
const call = vi.fn(async (method: string, path: string) => {
  sent.push(`${method} ${path}`);
  const a = answers[`${method} ${path}`] ?? { status: 404, body: { error: 'Not found.' } };
  return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
});
const undo = vi.fn(async () => ({ status: 200, text: JSON.stringify({ id: 'j1' }) }));

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  (window as unknown as { app: unknown }).app = {
    obid: { call, undo, live: () => () => {} }, openExternal: vi.fn(), editorName: async () => 'Cursor', openWorktree: vi.fn(async () => true),
  };
  localStorage.clear();
  Element.prototype.scrollIntoView = vi.fn();
  answers = {
    'GET /v1/jobs/j1/changes': { status: 200, body: WHOLE },
    'GET /v1/jobs/j1/changes?round=2': { status: 200, body: SECOND },
    'GET /v1/jobs/j1/changes?whitespace=ignore': { status: 200, body: WHOLE },
  };
  sent = [];
  undo.mockClear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const at = '2026-11-17T10:00:00Z';
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Add hello', agent: 'claude', account: 'a1', repo: 'o/r', branch: 'oby/build-j1', base: 'main', issue: null, kind: 'build',
  status: 'done', step: 'Done', summary: '', created: at, finished: at, commits: 1, published: null, rounds: 1, round: 1,
  check: null, model: null, dismissed: null, permission: null, pipeline: null, title: 'Add hello', worktree: '/obid/worktrees/j1', ...more,
});
async function taskFor(j: Job) {
  const live = await import('../live');
  const s = live.liveState();
  s.jobs = { [j.id]: j };
  s.seen.add(j.id);
  return live.taskOf(s, j);
}
async function panel() {
  const { default: ChangesPanel, openChanges } = await import('../ChangesPanel');
  await act(async () => root.render(<ChangesPanel />));
  await act(async () => openChanges('j1'));
  await flush();
}
const button = (name: string) => Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent === name);
const lines = (kind: string) => Array.from(document.querySelectorAll(`.changes-line.is-${kind}`)).map((l) => l.textContent!.trimEnd());

describe('CHANGES-4: the task card\'s Changes opens the panel', () => {
  it('changes_4_a_card_with_a_worktree_shows_changes_and_one_without_none', async () => {
    const { ChangesPill, useChangesOpen } = await import('../ChangesPanel');
    let opened: string | null = null;
    const Seen = () => { opened = useChangesOpen(); return null; };
    const task = await taskFor(job());
    await act(async () => root.render(<><ChangesPill task={task} /><Seen /></>));
    await act(async () => button('Changes')!.click());
    expect(opened).toBe('j1');
    const bare = await taskFor(job({ worktree: null }));
    await act(async () => root.render(<ChangesPill task={bare} />));
    expect(button('Changes')).toBeUndefined();
  });

  it('changes_4_the_panel_lists_the_files_with_their_counts_then_the_diff', async () => {
    await panel();
    expect(sent).toContain('GET /v1/jobs/j1/changes');
    const files = Array.from(document.querySelectorAll('.changes-files button')).map((b) => b.textContent);
    expect(files).toEqual(['Addedhello.txt+1−0', 'Addedsecond.txt+1−0']);
    expect(document.querySelector('.changes-total')!.textContent).toBe('+2 −0');
    expect(lines('add')).toEqual(['+Hello', '+Second']);
    expect(lines('hunk')).toEqual(['@@ -0,0 +1 @@', '@@ -0,0 +1 @@']);
    // git's own header lines aren't shown
    expect(document.body.textContent).not.toContain('index 0000000');
    expect(Array.from(document.querySelectorAll('select option')).map((o) => o.textContent!.split(' · ')[0])).toEqual(['All rounds', 'Round 1', 'Round 2']);
  });
});

describe('CHANGES-5: a round, whitespace and a file', () => {
  it('changes_5_round_2_asks_obid_for_it_and_shows_its_files_only', async () => {
    await panel();
    const select = document.querySelector<HTMLSelectElement>('select[aria-label="Which round"]')!;
    await act(async () => { select.value = '2'; select.dispatchEvent(new Event('change', { bubbles: true })); });
    await flush();
    expect(sent).toContain('GET /v1/jobs/j1/changes?round=2');
    expect(Array.from(document.querySelectorAll('.changes-files .changes-path')).map((p) => p.textContent)).toEqual(['second.txt']);
    expect(lines('add')).toEqual(['+Second']);
  });
  it('changes_5_ignore_whitespace_asks_with_whitespace_ignore', async () => {
    await panel();
    const box = document.querySelector<HTMLInputElement>('.changes-ws input')!;
    await act(async () => box.click());
    await flush();
    expect(sent).toContain('GET /v1/jobs/j1/changes?whitespace=ignore');
  });
  it('changes_5_a_click_on_a_file_goes_to_its_diff', async () => {
    await panel();
    const go = Element.prototype.scrollIntoView as ReturnType<typeof vi.fn>;
    await act(async () => (document.querySelectorAll<HTMLButtonElement>('.changes-files button')[1]).click());
    expect(go).toHaveBeenCalledTimes(1);
    expect((go.mock.contexts[0] as Element).getAttribute('data-file')).toBe('second.txt');
  });
});

describe('CHANGES-14: Undo this round… in the menu', () => {
  const menu = async (j: Job) => {
    const { default: TaskMenu } = await import('../TaskMenu');
    const task = await taskFor(j);
    await act(async () => root.render(<TaskMenu task={task} at={new DOMRect()} onClose={() => {}} />));
    await flush();
  };
  it('changes_14_the_menu_offers_undo_only_while_obid_has_a_sentence_and_sends_that_sentence_to_main', async () => {
    answers['GET /v1/jobs/j1/undo'] = { status: 200, body: { action: UNDO, from: 'b'.repeat(40), to: 'c'.repeat(40), branch: 'oby/build-j1', push: false } };
    await menu(job());
    for (const item of ['Changes', 'Open in Terminal', 'Open in Cursor', 'Undo this round…']) expect(button(item), item).toBeDefined();
    await act(async () => button('Undo this round…')!.click());
    await flush();
    // the page never signs: main gets the job and obid's sentence, and asks in its own dialog
    expect(undo).toHaveBeenCalledWith('j1', UNDO);
    expect(sent.filter((s) => s.startsWith('POST'))).toEqual([]);
  });
  it('changes_14_no_sentence_no_undo_and_no_worktree_none_of_it', async () => {
    answers['GET /v1/jobs/j1/undo'] = { status: 409, body: { error: 'Nothing this task did is left to undo.' } };
    await menu(job());
    expect(button('Undo this round…')).toBeUndefined();
    expect(button('Open in Terminal')).toBeDefined();
    await act(async () => root.unmount());
    root = createRoot(host);
    sent = [];
    await menu(job({ worktree: null }));
    for (const item of ['Changes', 'Open in Terminal', 'Undo this round…']) expect(button(item), item).toBeUndefined();
    expect(sent).not.toContain('GET /v1/jobs/j1/undo');
  });
});

// T3 Code's diffRendering tests ("diff lines drawn as what they are"), held to CHANGES-4: the diff, every changed line of it
describe('T3: the diff as git wrote it', () => {
  it('t3_changes_a_changed_line_that_starts_with_dashes_or_pluses_still_shows', async () => {
    const diff = [
      'diff --git a/query.sql b/query.sql', 'index 1111111..2222222 100644', '--- a/query.sql', '+++ b/query.sql',
      '@@ -1,2 +1,2 @@', '--- old comment', '+-- new comment', ' select 1;',
      'diff --git a/a.c b/a.c', 'index 3333333..4444444 100644', '--- a/a.c', '+++ b/a.c',
      '@@ -1 +1,2 @@', ' int i;', '+++i;', '',
    ].join('\n');
    answers['GET /v1/jobs/j1/changes'] = { status: 200, body: { ...WHOLE, diff, files: [
      { path: 'query.sql', status: 'modified', additions: 1, deletions: 1 }, { path: 'a.c', status: 'modified', additions: 1, deletions: 0 },
    ] } };
    await panel();
    expect(lines('del')).toEqual(['--- old comment']);
    expect(lines('add')).toEqual(['+-- new comment', '+++i;']);
  });
});
