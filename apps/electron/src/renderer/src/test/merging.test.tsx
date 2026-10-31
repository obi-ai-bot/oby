import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { stagesOf } from '../jobPage';
import { mergeAlertTitle, type InboxItem } from '../../../shared/obid-wire';
import type { Job } from '../obid';
import type { Job as JobView, Stage } from '../mock';

// The task card's Review and Merged stages in obid's own words (docs/contracts/merging.md)
const job = (more: Partial<Job> = {}): Job => ({
  id: 'j1', request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'done', step: '', summary: 'Fixed it.', created: '2026-10-30T10:00:00Z', finished: '2026-10-30T10:20:00Z',
  commits: 1, published: 'https://github.com/deskree-inc/oby/pull/7', rounds: 1, round: 1, check: null, model: null, dismissed: null,
  permission: null, title: 'Fix it',
  pipeline: { number: 5, total: 6, stages: [
    { id: 'publish', title: 'Publish', state: 'done', detail: 'Secret scan clean · pushed · pull request opened' },
    { id: 'review', title: 'Review', state: 'done', detail: 'Approved by Oby' },
    { id: 'merged', title: 'Merged into dev', state: 'current', detail: 'Not merged: it conflicts with dev. Ask Oby to rebase it onto dev, or settle it on GitHub.' },
  ] },
  ...more,
});

async function card(stages: Stage[]) {
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('window', {});
  const { default: TaskCard } = await import('../TaskCard');
  const { AGENTS } = await import('../mock');
  const view = {
    id: 'j1', type: 'tasks', title: 'Fix it', request: 'Fix it', agent: 'claude', account: 'Personal', model: 'Opus',
    repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', status: 'done', step: '', stages, created: 20, finished: 1,
  } as JobView;
  return renderToStaticMarkup(<TaskCard alone task={{ id: 'j1', type: 'tasks', title: 'Fix it', brand: AGENTS.claude.brand, status: 'done', job: view } as never} />);
}

describe('merging on the task card', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it("MERGING-10: a done task's Review stage says what obid says, never only \"Waiting for review\"", async () => {
    for (const detail of ['Oby is reviewing', 'Waiting for review · no account can review it', 'Pull request open · waiting for your review on GitHub']) {
      const html = await card(stagesOf(job({ pipeline: { number: 5, total: 6, stages: [{ id: 'review', title: 'Review', state: 'current', detail }] } })));
      expect(html).toContain(detail);
    }
  });

  it("MERGING-10: the Merged stage says why it isn't merged", async () => {
    const html = await card(stagesOf(job()));
    expect(html).toContain('Not merged: it conflicts with dev. Ask Oby to rebase it onto dev, or settle it on GitHub.');
  });

  it("MERGING-6: a pull request obid couldn't read says why, under the stage that waits on it", () => {
    const why = "Couldn't refresh the pull request: GitHub's CLI (gh) isn't installed.";
    const stages = stagesOf(job({ pullRequestError: why }));
    expect(stages.find((s) => s.id === 'merged')?.sub).toBe(why);
    expect(stages.find((s) => s.id === 'review')?.sub).toBeUndefined();
    // still being reviewed: under Review
    const reviewing = stagesOf(job({ pullRequestError: why, pipeline: { number: 5, total: 6, stages: [
      { id: 'review', title: 'Review', state: 'current', detail: 'Oby is reviewing' },
      { id: 'merged', title: 'Merged into dev', state: 'upcoming' },
    ] } }));
    expect(reviewing.find((s) => s.id === 'review')?.sub).toBe(why);
    // nothing published: nothing to read
    expect(stagesOf(job({ pullRequestError: why, published: null })).every((s) => !s.sub)).toBe(true);
  });

  it("MERGING-12: the alert for a merge waiting on the owner is titled as its card is", () => {
    const item = (more: Partial<InboxItem>) => ({ id: 'm1', kind: 'merge', job: 'j1', title: 'Ready to merge: Fix it', text: '', options: [], details: null,
      created: '', state: 'open', later: null, headline: null, recommended: null, ...more }) as InboxItem;
    expect(mergeAlertTitle([item({})], 'j1')).toBe('Ready to merge');
    expect(mergeAlertTitle([item({ title: 'Needs a teammate: Fix it' })], 'j1')).toBe('Needs a teammate');
    expect(mergeAlertTitle([item({ title: 'Not merged: Fix it' })], 'j1')).toBe('Not merged');
    // another job's, a card answered already, or none: the default
    expect(mergeAlertTitle([item({ job: 'j2' }), item({ state: 'done' })], 'j1')).toBe('Not merged');
    expect(mergeAlertTitle([], 'j1')).toBe('Not merged');
  });
});
