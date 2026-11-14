import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fresh, launch, mac, served } from './macAlerts';

// The Electron app's request key (docs/contracts/receipts.md, RECEIPTS-16): one per click, sent
// only once obid says it takes keys, and the same again when obid didn't answer at all.

// obid's answers, in turn, to each method and path (the last one stays); status 0: no answer at all
type Answer = { status: number; body?: unknown };
let answers: Record<string, Answer[]> = {};
const calls: { method: string; path: string; body?: unknown }[] = [];
let feed: (text: string) => void = () => {};
const sends = (path: string) => calls.filter((c) => c.method === 'POST' && c.path === path);
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const flush = () => new Promise((r) => setTimeout(r, 0));
const job = (more: Record<string, unknown> = {}) => ({
  id: 'j1', request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix-1', base: 'dev', issue: null,
  kind: 'work', status: 'done', step: 'Done', summary: '', created: '2026-10-30T10:00:00Z', finished: '2026-10-30T10:20:00Z',
  commits: 1, published: null, rounds: 1, round: 1, check: null, model: null, dismissed: null, permission: null, pipeline: null,
  title: 'Fix it', archived: null, ...more,
});
const question = (id: string) => ({
  id, kind: 'question', job: '', title: 'Which store?', text: 'Which one?', options: ['SQLite', 'Postgres'], details: null,
  created: '2026-10-30T10:00:00Z', state: 'open', later: null, headline: null, recommended: null,
});

function memory(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); },
  };
}

// The app on obid, following its events, with obid's status as given (receipts or not)
async function onObid(status: Record<string, unknown>, inbox: unknown[] = []) {
  vi.resetModules();
  calls.length = 0;
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', memory());
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const queue = answers[`${method} ${path}`];
    const a = (queue && queue.length > 1 ? queue.shift() : queue?.[0]) ?? { status: 404, body: { error: 'Not found.' } };
    return { status: a.status, text: a.status === 0 ? 'obid closed the connection.' : a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  const follow = (onChunk: (text: string) => void) => { feed = onChunk; return () => {}; };
  vi.stubGlobal('window', { app: { obid: { call, confirm: async () => ({ status: 200, text: '{}' }), follow } } });
  const live = await import('../live');
  answers['GET /v1/status'] = [{ status: 200, body: { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0, ...status } }];
  answers['GET /v1/jobs?limit=100'] = [{ status: 200, body: [job()] }];
  answers['GET /v1/inbox'] = [{ status: 200, body: inbox }];
  answers['GET /v1/conversations?limit=200'] = [{ status: 200, body: [] }];
  live.connect();
  feed('');
  for (let i = 0; i < 5; i++) await flush();
  expect(live.liveState().connection).toBe('live');
  calls.length = 0;
  return live;
}

describe('RECEIPTS-16: the app sends a request key', () => {
  beforeEach(() => { answers = {}; vi.setSystemTime('2026-10-30T11:00:00Z'); });
  afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

  it('receipts_16_a_resent_message_keeps_its_key', async () => {
    const live = await onObid({ receipts: true });
    // the words to the ended task went, and obid's answer was lost; sent again, obid starts the follow-up
    answers['POST /v1/jobs/j1/messages'] = [{ status: 0 }, { status: 201, body: job({ id: 'j2', status: 'running', finished: null }) }];
    answers['GET /v1/jobs/j1'] = [{ status: 200, body: job() }];
    const followUp = await live.tellJob('j1', 'and this');
    expect(followUp?.id).toBe('j2');
    const [first, again] = sends('/v1/jobs/j1/messages');
    expect(sends('/v1/jobs/j1/messages')).toHaveLength(2);
    expect(JSON.stringify(again.body)).toBe(JSON.stringify(first.body));
    expect(first.body).toEqual({ text: 'and this', clientRequestId: expect.stringMatching(UUID) });
    expect(Object.keys(live.liveState().jobs).filter((id) => id !== 'j1')).toEqual(['j2']);
    // another click is another key
    answers['POST /v1/jobs/j1/messages'] = [{ status: 201, body: job({ id: 'j3', status: 'running', finished: null }) }];
    await live.tellJob('j1', 'and this');
    const third = sends('/v1/jobs/j1/messages')[2].body as { clientRequestId: string };
    expect(third.clientRequestId).toMatch(UUID);
    expect(third.clientRequestId).not.toBe((first.body as { clientRequestId: string }).clientRequestId);
  });

  it('receipts_16_no_key_without_receipts', async () => {
    for (const status of [{}, { receipts: false }]) {
      const live = await onObid(status);
      // obid doesn't answer: an unkeyed send goes once, as it always did
      answers['POST /v1/jobs/j1/messages'] = [{ status: 0 }];
      answers['POST /v1/jobs/j1/stop'] = [{ status: 0 }];
      answers['POST /v1/inbox/i1/answer'] = [{ status: 0 }];
      await live.tellJob('j1', 'and this', [], true);
      await live.stopJob('j1');
      await live.answerQuestion('i1', 'SQLite').catch(() => {});
      expect(calls, JSON.stringify(status)).toEqual([
        { method: 'POST', path: '/v1/jobs/j1/messages', body: { text: 'and this', conversation: true } },
        { method: 'POST', path: '/v1/jobs/j1/stop', body: undefined },
        { method: 'POST', path: '/v1/inbox/i1/answer', body: { answer: 'SQLite' } },
      ]);
      vi.unstubAllGlobals();
    }
  });

  it('receipts_16_stop_and_answers_carry_a_key', async () => {
    const live = await onObid({ receipts: true }, [question('i1'), question('i2')]);
    answers['POST /v1/jobs/j1/stop'] = [{ status: 0 }, { status: 200, body: job() }];
    answers['POST /v1/inbox/i1/answer'] = [{ status: 0 }, { status: 200, body: { ...question('i1'), state: 'done' } }];
    answers['POST /v1/inbox/i2/answer'] = [{ status: 0 }, { status: 200, body: { ...question('i2'), state: 'later' } }];
    await live.stopJob('j1');
    await live.answerJob('i1', 'SQLite');
    await live.answerJob('i2', 'Later');
    const keys = new Set<string>();
    for (const [path, shape] of [
      ['/v1/jobs/j1/stop', {}], ['/v1/inbox/i1/answer', { answer: 'SQLite' }], ['/v1/inbox/i2/answer', { later: expect.any(String) }],
    ] as const) {
      const [first, again, ...more] = sends(path);
      expect(more, path).toEqual([]);
      expect(first.body, path).toEqual({ ...shape, clientRequestId: expect.stringMatching(UUID) });
      expect(JSON.stringify(again.body), path).toBe(JSON.stringify(first.body));
      keys.add((first.body as { clientRequestId: string }).clientRequestId);
    }
    expect(keys.size).toBe(3);
    // any answer of obid's is final, a refusal too
    answers['POST /v1/jobs/j1/stop'] = [{ status: 409, body: { error: 'It already ended.' } }];
    calls.length = 0;
    await live.stopJob('j1');
    expect(calls).toHaveLength(1);
  });
});

describe('RECEIPTS-16: a notification\'s button', () => {
  beforeEach(fresh);

  it('receipts_16_a_notification_answer_carries_a_key', async () => {
    for (const receipts of [true, false]) {
      fresh();
      mac.receipts = receipts;
      await launch();
      mac.served = [served('i1', { title: 'Which store?', category: 'question', item: 'i1', answers: ['SQLite', 'Allow'] })];
      mac.event('job', { id: 'j1', title: 'Fix it', status: 'running' });
      await vi.waitFor(() => expect(mac.posted).toHaveLength(1), { timeout: 5000 });
      // obid didn't answer the first: with keys, the same body and key again; without, once
      for (const [words, signed] of [['SQLite', false], ['Allow', true]] as const) {
        mac.sent = [];
        mac.lost = 1;
        await mac.posted[0].press(words);
        if (receipts) {
          expect(mac.sent, words).toEqual([
            { method: 'POST', path: '/v1/inbox/i1/answer', body: { answer: words, clientRequestId: expect.stringMatching(UUID) }, signed },
            { method: 'POST', path: '/v1/inbox/i1/answer', body: mac.sent[0].body, signed },
          ]);
        } else {
          expect(mac.sent, words).toEqual([{ method: 'POST', path: '/v1/inbox/i1/answer', body: { answer: words }, signed }]);
        }
      }
    }
  }, 20_000);
});
