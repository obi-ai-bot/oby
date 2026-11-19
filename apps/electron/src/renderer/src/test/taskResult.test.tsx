import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Job as JobView } from '../view';

// A task's page shows the agent's summary under Result read as Markdown, as the conversation reads
// it, never its marks as text (docs/contracts/tasks.md, TASKS-33)
async function page(summary: string) {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', {});
  vi.stubGlobal('document', { createElement: () => ({}) });
  const { default: TaskCard } = await import('../TaskCard');
  const { BRANDS } = await import('../brands');
  const job = {
    id: 'j1', type: 'build', title: 'Oby drive 22:51', request: 'Add a line', agent: 'claude', account: 'Default', model: 'Opus',
    repo: 'well-prado/tetrix-e2e', branch: 'oby/build-1', base: 'main', status: 'done', step: '', stages: [], created: 20, finished: 1,
    commits: 1, published: 'https://github.com/well-prado/tetrix-e2e/pull/170', summary,
  } as JobView;
  const task = { id: 'j1', type: 'build', title: job.title, brand: BRANDS.claude, status: 'done', job } as never;
  return renderToStaticMarkup(<TaskCard task={task} alone />).replaceAll('<!-- -->', '');
}

describe('a task page reads its Result as Markdown', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it('TASKS-33: bold, code and a code block render, with no marks left as text', async () => {
    const html = await page('**What:** I added a line to `docs/oby-drive.md`.\n\n```\n- follow-up\n```');
    expect(html).toContain('Result');
    expect(html).toContain('<strong>');
    expect(html).toContain('class="tok tok-code"');
    expect(html).toContain('class="ss-code"');
    expect(html).not.toContain('**What:**');
    expect(html).not.toContain('```');
  });

  it('TASKS-33: HTML in a summary shows as the text it is', async () => {
    const html = await page('Done. <img src=x onerror=alert(1)>');
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img');
  });
});
