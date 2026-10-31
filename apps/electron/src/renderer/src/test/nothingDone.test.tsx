import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Job as JobView } from '../mock';

// A task that ended with nothing done reads that way, from obid's own fact (docs/contracts/tasks.md)
async function cards(more: Partial<JobView>) {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', {});
  // ChatView's words make a scratch textarea as they load
  vi.stubGlobal('document', { createElement: () => ({}) });
  const { default: TaskCard } = await import('../TaskCard');
  const { ThreadTask } = await import('../ChatView');
  const { AGENTS } = await import('../mock');
  const job = {
    id: 'j1', type: 'issues', title: 'Resolve issue 140', request: 'Do issue 140', agent: 'claude', account: 'Default', model: 'Opus',
    repo: 'well-prado/tetrix-e2e', branch: 'oby/fix-1', base: 'dev', status: 'done', step: '', stages: [], created: 20, finished: 1,
    commits: 0, ...more,
  } as JobView;
  const task = { id: 'j1', type: 'issues', title: job.title, brand: AGENTS.claude.brand, status: 'done', job } as never;
  // the words as they read, without React's markers between them
  const read = (html: string) => html.replaceAll('<!-- -->', '');
  return {
    thread: read(renderToStaticMarkup(<ThreadTask task={task} current onOpen={() => {}} />)),
    home: read(renderToStaticMarkup(<TaskCard task={task} />)),
  };
}

describe('a task that ended with nothing done', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it('TASKS-12: its card says Nothing done and No changes, never Done', async () => {
    const { thread, home } = await cards({ nothingDone: true });
    expect(thread).toContain('Nothing done · Claude Code · Default');
    expect(thread).not.toContain('>Done · ');
    expect(thread).toContain('No changes');
    expect(home).toContain('No changes');
  });

  it('TASKS-12: one obid says was done still reads Done', async () => {
    const { thread, home } = await cards({ commits: 1, published: 'https://github.com/well-prado/tetrix-e2e/pull/7' });
    expect(thread).toContain('>Done · Claude Code · Default');
    expect(thread).not.toContain('Nothing done');
    expect(home).not.toContain('No changes');
  });
});
