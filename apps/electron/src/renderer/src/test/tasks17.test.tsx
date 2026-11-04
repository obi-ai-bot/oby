import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';

// TASKS-17: Oby's escalation on the AskStack shows its whole text and, with a job, Tell it what
// to do (docs/contracts/tasks.md). Render the card, not helpers.
vi.mock('react', async (original) => ({
  ...await original<typeof import('react')>(),
  useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
}));

let answers: Record<string, unknown> = {};
let feed: (text: string) => void = () => {};
const flush = async () => { for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0)); };
const at = '2026-10-30T10:00:00Z';
const LONG = Array.from({ length: 8 }, (_, i) => `Line ${i + 1}: Oby looked at the pull request and recommends a rebase onto main.`).join('\n');
const job = (id: string, more: Record<string, unknown> = {}) => ({
  id, request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix', base: 'dev', issue: null, kind: 'work',
  status: 'running', step: 'Working', summary: '', created: at, finished: null, commits: 0, published: null, rounds: 1, round: 1, check: null,
  model: null, dismissed: null, permission: null, pipeline: null, title: 'Fix the login', archived: null, conversation: 'c1', ...more,
});
const escalation = (more: Record<string, unknown> = {}) => ({
  id: 'orchestrator-abc123def456', kind: 'question', job: 'j1', title: 'Fix the login needs your decision', text: LONG,
  options: ['Done'], details: null, created: at, state: 'open', later: null, headline: null, recommended: null, ...more,
});

async function stack(inbox: unknown[], jobs: unknown[] = [job('j1')]) {
  vi.resetModules();
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('innerWidth', 1000);
  vi.stubGlobal('addEventListener', () => {});
  vi.stubGlobal('removeEventListener', () => {});
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  vi.stubGlobal('window', { app: { obid: {
    call: async (method: string, path: string) => {
      const a = answers[`${method} ${path}`];
      return a === undefined ? { status: 404, text: '{"error":"Not found."}' } : { status: 200, text: JSON.stringify(a) };
    },
    follow: (onChunk: (text: string) => void) => { feed = onChunk; return () => {}; },
  } } });
  answers = {
    'GET /v1/status': { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0 },
    'GET /v1/jobs?limit=100': jobs,
    'GET /v1/inbox': inbox,
    'GET /v1/conversations?limit=200': [{ id: 'c1', title: 'Talk', pinned: false, created: at, lastActivity: at, cursor: '' }],
    'GET /v1/limit-decisions': [],
  };
  const live = await import('../live');
  live.connect();
  feed('');
  await flush();
  expect(live.liveState().connection).toBe('live');
  const { default: AskStack } = await import('../AskStack');
  const none = () => {};
  return {
    live,
    html: renderToStaticMarkup(<AskStack active onOpen={none} onOpenTalk={none} />).replaceAll('<!-- -->', ''),
  };
}

describe('TASKS-17: Oby’s escalation card is answerable on the stack', () => {
  beforeEach(() => { answers = {}; });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('tasks_17_escalation_card_shows_whole_text_and_tell_it_what_to_do', async () => {
    const { html, live } = await stack([escalation()]);
    const ask = live.askOf(live.liveState(), live.liveState().jobs.j1!);
    expect(ask).toMatchObject({
      kind: 'question',
      headline: 'Fix the login needs your decision',
      text: LONG,
      options: ['Done'],
      fullText: true,
      steer: true,
    });
    // the card itself: whole text (no clamp class alone), the steer box, Done and Later; never a
    // button of the model's recommendation
    expect(html).toContain('tc-ask is-full is-steer');
    expect(html).toContain(LONG);
    expect(html).toContain('aria-label="Tell it what to do"');
    expect(html).toContain('placeholder="Tell it what to do"');
    expect(html).toMatch(/>Done</);
    expect(html).toMatch(/>Later</);
    expect(html).toContain('needs your decision');
    expect(html).not.toContain('Rebase it</button>');
  });

  it('tasks_17_an_escalation_with_no_job_keeps_done_and_later_with_whole_text', async () => {
    const { html } = await stack([escalation({ job: '', title: 'Oby needs your decision' })], []);
    expect(html).toContain('tc-ask is-full');
    expect(html).not.toContain('is-steer');
    expect(html).toContain(LONG);
    expect(html).toMatch(/>Done</);
    expect(html).toMatch(/>Later</);
    expect(html).not.toContain('Tell it what to do');
  });
});
