import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Job } from '../obid';
import type { Task } from '../tasks';

// TASKS-25: a failed or interrupted task raises no card. Its reason is a plain line on its card and
// page, its mark a red Failed or Stopped, and its menu offers Try again and Stop here
// (docs/contracts/tasks.md). Render the screens, not helpers.

// a menu's portal has no server markup: its items, in place
vi.mock('../Menu', () => ({ Menu: ({ children }: { children: React.ReactNode }) => <div>{children}</div> }));
vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));

let live: typeof import('../live');
let ui: { TaskCard: typeof import('../TaskCard').default; StatusDot: typeof import('../StatusDot').default; TaskMenu: typeof import('../TaskMenu').default };
beforeAll(async () => {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', { app: { obid: { call: vi.fn() } } });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  live = await import('../live');
  ui = { TaskCard: (await import('../TaskCard')).default, StatusDot: (await import('../StatusDot')).default, TaskMenu: (await import('../TaskMenu')).default };
});
beforeEach(() => {
  const s = live.liveState();
  s.jobs = {};
  s.inbox = [];
});

const at = '2026-11-14T10:00:00Z';
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Fix the login', agent: 'claude', account: 'a1', repo: 'o/r', branch: 'oby/fix-1', base: 'dev', issue: null, kind: 'task',
  status: 'failed', step: 'Failed', summary: '', reason: 'Not logged in\nmore', created: at, finished: at, commits: null, published: null, rounds: 1, round: 1,
  check: null, model: null, dismissed: null, permission: null, pipeline: null, title: 'Fix the login', ...more,
});
const taskFor = (j: Job): Task => {
  const s = live.liveState();
  s.jobs = { [j.id]: j };
  s.seen.add(j.id);
  return live.taskOf(s, j);
};
const read = (html: string) => html.replaceAll('<!-- -->', '');
const rect = { top: 0, bottom: 0, left: 0, right: 0, width: 0, height: 0, x: 0, y: 0 } as DOMRect;
const menu = (task: Task) => read(renderToStaticMarkup(<ui.TaskMenu task={task} at={rect} onClose={() => {}} onOpenChat={() => {}} onRename={() => {}} />));

describe('TASKS-25: a failed task shows its reason as a plain line, Try again in its menu', () => {
  it('tasks_25_its_card_and_page_show_the_reason_and_no_card_to_answer', () => {
    const task = taskFor(job());
    for (const card of [read(renderToStaticMarkup(<ui.TaskCard task={task} />)), read(renderToStaticMarkup(<ui.TaskCard task={task} alone />))]) {
      expect(card).toContain('<p class="tc-result is-stopped">Not logged in</p>');
      // its first line only
      expect(card).not.toMatch(/Not logged in\s*more/);
      for (const word of ['Give it another round', 'Stop here', 'Later']) expect(card).not.toContain(word);
    }
  });
  it('tasks_25_its_mark_is_a_red_failed_or_stopped', () => {
    const failed = read(renderToStaticMarkup(<ui.StatusDot task={taskFor(job())} />));
    expect(failed).toContain('aria-label="Failed"');
    expect(failed).toContain('is-failed');
    const interrupted = read(renderToStaticMarkup(<ui.StatusDot task={taskFor(job({ status: 'interrupted' }))} />));
    expect(interrupted).toContain('aria-label="Stopped"');
    expect(interrupted).toContain('is-failed');
    // stopped by the owner: a finished one's check, as before
    expect(read(renderToStaticMarkup(<ui.StatusDot task={taskFor(job({ status: 'stopped' }))} />))).toContain('aria-label="Done"');
  });
  it('tasks_25_its_menu_offers_try_again_and_stop_here', () => {
    const html = menu(taskFor(job()));
    expect(html).toContain('>Try again</button>');
    expect(html).toContain('>Stop here</button>');
    expect(html.indexOf('Rename')).toBeLessThan(html.indexOf('Try again'));
    expect(html.indexOf('Stop here')).toBeLessThan(html.indexOf('Archive task'));
  });
  it('tasks_25_a_running_or_let_go_tasks_menu_offers_neither', () => {
    for (const j of [job({ status: 'running', step: 'Working', finished: null }), job({ dismissed: at })]) {
      const html = menu(taskFor(j));
      expect(html).not.toContain('Try again');
      expect(html).not.toContain('Stop here');
    }
  });
});

describe('TASKS-25 on the mock (?everything): Codex keeps its sandbox roots on resume', () => {
  it('tasks_25_the_mocks_failed_task_shows_its_line_and_red_dot_and_offers_try_again', async () => {
    vi.resetModules();
    vi.stubGlobal('location', { search: '?everything' });
    vi.stubGlobal('window', {});
    const tasks = await import('../tasks');
    const { default: TaskCard } = await import('../TaskCard');
    const { default: StatusDot } = await import('../StatusDot');
    const { default: TaskMenu } = await import('../TaskMenu');
    const s2 = tasks.tasksOf('tasks').find((t) => t.title === 'Codex keeps its sandbox roots on resume')!;
    // not in Needs you
    expect(s2.status).toBe('done');
    expect(tasks.tasksWith('needs').map((t) => t.id)).not.toContain(s2.id);
    const card = read(renderToStaticMarkup(<TaskCard task={s2} />));
    expect(card).toContain('<p class="tc-result is-stopped">Two sandbox tests still fail after 2 rounds</p>');
    expect(card).not.toContain('tc-ask');
    expect(read(renderToStaticMarkup(<StatusDot task={s2} live />))).toContain('aria-label="Failed"><span class="status-dot is-failed"');
    const html = read(renderToStaticMarkup(<TaskMenu task={s2} at={rect} onClose={() => {}} onOpenChat={() => {}} onRename={() => {}} />));
    expect(html).toContain('>Try again</button>');
    expect(html).toContain('>Stop here</button>');
    // Try again on the mock: it runs again at once, and its menu no longer offers it (TASKS-10)
    tasks.tryAgain(s2.id);
    const again = tasks.tasksOf('tasks').find((t) => t.id === s2.id)!;
    expect(again).toMatchObject({ status: 'running', job: { status: 'running', step: 'Getting ready' } });
    expect(again.job.canTryAgain).toBeUndefined();
  });
});
