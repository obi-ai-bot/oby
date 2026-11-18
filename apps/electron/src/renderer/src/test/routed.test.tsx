import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { Job as JobView } from '../view';

// A task routed off the agent your words named says why on its card (docs/contracts/accounts.md, ACCOUNTS-31)
async function card(more: Partial<JobView>) {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', {});
  vi.stubGlobal('document', { createElement: () => ({}) });
  const { default: TaskCard } = await import('../TaskCard');
  const { AGENTS } = await import('../view');
  const job = {
    id: 'j1', type: 'issues', title: 'Add cube function', request: 'On Claude Code: add cube', agent: 'codex', account: 'Personal', model: 'GPT',
    repo: 'well-prado/oby-scratch', branch: 'oby/build-1', base: 'main', status: 'running', step: 'Starting Codex', stages: [], created: 1,
    ...more,
  } as JobView;
  const task = { id: 'j1', type: 'issues', title: job.title, brand: AGENTS.codex.brand, status: 'running', job } as never;
  return renderToStaticMarkup(<TaskCard task={task} />).replaceAll('<!-- -->', '');
}

describe('a task routed elsewhere', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it('ACCOUNTS-31: its card shows obid\'s line saying where it runs and why', async () => {
    const line = 'Claude Code is at 97% of its week until Thu 9:00 AM, so this runs on Codex · Personal.';
    const html = await card({ routed: line });
    expect(html).toContain(`<p class="tc-routed">${line}</p>`);
  });

  it('ACCOUNTS-31: one that runs where you asked has no such line', async () => {
    expect(await card({})).not.toContain('tc-routed');
  });
});
