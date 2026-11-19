// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { InboxItem } from '../obid';
import { ACCEPTS, SCOPES } from '../../../shared/sphere';

// Another Oby's task (docs/contracts/handoff.md, HANDOFF-6): this app's card takes one and its
// delegation may send one; Home shows its item as the review request's card, titled `Task from …`,
// with the goal quoted and the item's own answers. obid's bridge is stood in for.
const call = vi.fn();
let live: typeof import('../live');
let stack: typeof import('../AskStack');
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call } } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  live = await import('../live');
  stack = await import('../AskStack');
});
afterAll(() => vi.unstubAllGlobals());

// as obid makes it (core/crates/obid/src/sphere/inbox.rs, handoff_item)
const handoff: InboxItem = {
  id: 'obi-h1', kind: 'obiRequest', job: '', title: 'Task from ana', created: '2026-11-16T12:00:00Z', state: 'open',
  text: 'ana’s Obi (obi-ana) asks your Oby to do a task in demo/greeter from main, quoted below. Conditions: tests pass.',
  options: ['Accept', 'Accept, and remember for this repo', 'Decline', 'Decline, and remember for this repo'],
  details: null, later: null, headline: null, recommended: null, triage: 'needs_you',
  obi: { type: 'handoff', fromOwner: 'ana', repo: 'demo/greeter', baseBranch: 'main', quoted: {
    goal: 'Let greet take a name', summary: 'For the demo.', acceptance: [{ index: 0, note: null }],
  } },
} as InboxItem;

describe('another Oby’s task', () => {
  it('HANDOFF-6: the card takes a handoff and the delegation may send one', () => {
    expect(ACCEPTS).toContain('handoff');
    expect(SCOPES.split(' ')).toContain('send:handoff');
  });

  it('HANDOFF-6: Home shows it as a task, its goal quoted, with its own answers', async () => {
    const task = live.reviewOf(handoff)!;
    expect(task).toMatchObject({ kind: 'task', from: 'ana', repo: 'demo/greeter', goal: 'Let greet take a name', summary: 'For the demo.' });
    const host = document.createElement('div');
    const root = createRoot(host);
    await act(async () => root.render(<stack.ReviewBlock review={task} />));
    expect(host.querySelector('.tc-ask-kind')!.textContent).toBe('Task');
    expect([...host.querySelectorAll('blockquote p')].map((p) => p.textContent)).toEqual(['Let greet take a name', 'For the demo.']);
    expect([...host.querySelectorAll('button')].map((b) => b.textContent)).toEqual([
      'Accept', 'Accept, and remember for this repo', 'Decline', 'Decline, and remember for this repo', 'Later',
    ]);
    await act(async () => root.unmount());
  });
});
