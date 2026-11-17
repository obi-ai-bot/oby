// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { AgentQuestion, InboxItem, Job } from '../obid';

// An agent's question as T3 Code shows one (docs/contracts/permissions.md, BYPASS-10 and 11): each
// option's description, the recommended one, and several picked at once; through live.ts as obid
// sends it (the agentQuestion item and its job's permission.question) to the drawer over the
// message box, and what goes back to obid, which main must be able to sign (shared/words.ts).
const call = vi.fn();
let live: typeof import('../live');
let bar: typeof import('../DecisionBar');
let words: typeof import('../../../shared/words');
let clock = 0;
beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  window.app = { obid: { call } } as never;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.spyOn(performance, 'now').mockImplementation(() => clock);
  live = await import('../live');
  bar = await import('../DecisionBar');
  words = await import('../../../shared/words');
}, 120_000);
afterAll(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

// as obid sends them (sphere/inbox.rs permission_item; permissions.rs Question): the item's options
// the labels, its details their descriptions; the job's permission.question the whole question
function asked(question: AgentQuestion): { item: InboxItem; job: Job } {
  const item: InboxItem = {
    id: 'j1-permission-p1', kind: 'agentQuestion', job: 'j1', title: 'Fix the login', text: question.question,
    options: question.options.map((o) => o.label), details: question.options.some((o) => o.description) ? question.options.map((o) => o.description ?? '') : null,
    created: '2026-11-17T10:00:00Z', state: 'open', later: null, headline: null, recommended: null, triage: 'needs_you',
  };
  const job = {
    id: 'j1', status: 'running', title: 'Fix the login', conversation: 'c1',
    permission: { id: 'p1', tool: 'AskUserQuestion', title: 'Answer the agent’s question', detail: question.question, rule: null, always: null, question },
  } as unknown as Job;
  return { item, job };
}

let root: Root | undefined;
let host: HTMLElement;
async function show(question: AgentQuestion) {
  const { item, job } = asked(question);
  const s = live.liveState();
  s.jobs = { j1: job };
  s.inbox = [item];
  const ask = live.askOf(s, job)!;
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<bar.default pending={[{ key: 'j1', task: { id: 'j1', job: { ask } } }]} />));
  clock += 1000;
  call.mockResolvedValue({ status: 200, text: JSON.stringify({ ...item, state: 'done' }) });
  return { ask, item };
}
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; call.mockReset(); });
const choices = () => [...host.querySelectorAll('.tc-choice')];
const pick = (label: string) => [...host.querySelectorAll<HTMLButtonElement>('.tc-choice-pick')].find((b) => b.textContent === label)!;
const sent = () => call.mock.calls.find(([method, path]) => method === 'POST' && path === '/v1/inbox/j1-permission-p1/answer')?.[2] as { answer: string } | undefined;

describe('BYPASS-10: an agent’s question shows each option’s description and the one it recommends', () => {
  const STORE: AgentQuestion = {
    question: 'Which store?', header: 'Store', multiSelect: false,
    options: [{ label: 'SQLite (Recommended)', description: 'One file, nothing to run' }, { label: 'Postgres', description: 'A server to keep up' }, { label: 'Both' }],
  };

  it('bypass_10_maps_obids_question_with_its_descriptions_and_recommended', () => {
    const { item, job } = asked(STORE);
    expect(live.itemAsk(item, undefined, job.permission!.question)).toEqual({
      kind: 'question', headline: 'Which store?', text: '', options: ['SQLite (Recommended)', 'Postgres', 'Both', 'Tell it something'],
      about: ['One file, nothing to run', 'A server to keep up', ''], recommended: 0,
    });
  });

  it('bypass_10_each_option_with_its_description_the_recommended_ringed_one_pick_answers', async () => {
    await show(STORE);
    expect(choices().map((c) => c.textContent)).toEqual(['SQLite (Recommended)One file, nothing to run', 'PostgresA server to keep up', 'Both']);
    expect(choices().map((c) => c.classList.contains('is-recommended'))).toEqual([true, false, false]);
    // Tell it something and Later stay in the row under them
    expect([...host.querySelectorAll('.tc-options .tc-option')].map((b) => b.textContent)).toEqual(['Tell it something', 'Later']);
    await act(async () => pick('Postgres').click());
    expect(sent()).toMatchObject({ answer: 'Postgres' });
    // the button holds its label alone, so main signs exactly what went
    expect(words.owners('POST', '/v1/inbox/j1-permission-p1/answer', { answer: 'Postgres' }, [pick('Postgres').textContent!], [])).toEqual([0]);
  });

  it('bypass_10_a_recommended_label_goes_whole_as_obid_matches_it', async () => {
    await show(STORE);
    await act(async () => pick('SQLite (Recommended)').click());
    expect(sent()).toMatchObject({ answer: 'SQLite (Recommended)' });
  });
});

describe('BYPASS-11: a multiple-choice question takes several answers', () => {
  const CHECKS: AgentQuestion = {
    question: 'Which checks?', header: 'Checks', multiSelect: true,
    options: [{ label: 'Unit', description: 'Fast' }, { label: 'Lint' }, { label: 'E2E', description: 'Slow' }],
  };

  it('bypass_11_ticks_go_into_the_answer_and_send_answers_with_all_of_them', async () => {
    await show(CHECKS);
    const box = host.querySelector<HTMLInputElement>('input[aria-label="Your answer"]')!;
    const send = [...host.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent === 'Send')!;
    expect(send.disabled).toBe(true);
    // ticked out of order, sent in the options' order
    await act(async () => pick('E2E').click());
    await act(async () => pick('Unit').click());
    expect(pick('Unit').getAttribute('aria-checked')).toBe('true');
    expect(box.value).toBe('Unit, E2E');
    // a second tick takes it out
    await act(async () => pick('Lint').click());
    await act(async () => pick('Lint').click());
    expect(box.value).toBe('Unit, E2E');
    await act(async () => box.form!.requestSubmit());
    // as obid takes several (permissions.rs question_answer: labels with commas)
    expect(sent()).toMatchObject({ answer: 'Unit, E2E' });
    // the box held the words at Send, so main signs exactly what went
    expect(words.owners('POST', '/v1/inbox/j1-permission-p1/answer', { answer: 'Unit, E2E' }, ['Send', box.value], [])).toEqual([1]);
  });

  it('bypass_11_a_question_obid_says_takes_one_shows_no_ticks', () => {
    const { item } = asked(CHECKS);
    // the job's question is another one now (the next of the call): its multiSelect isn't this one's
    expect(live.itemAsk(item, undefined, { ...CHECKS, question: 'Something else?' }).multi).toBeUndefined();
    expect(live.itemAsk(item, undefined, CHECKS).multi).toBe(true);
    expect(live.itemAsk(item).multi).toBeUndefined();
  });
});
