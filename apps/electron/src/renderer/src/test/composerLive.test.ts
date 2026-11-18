import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';

// The message box's words through live.ts to obid (docs/contracts/composer.md): the chips as
// `defaults` (COMPOSER-3), the words waiting for a turn as obid's delivery says (COMPOSER-8), one
// cancelled alone (COMPOSER-9) and @'s files (COMPOSER-14), each only from an obid with composer-v1.

const calls: { method: string; path: string; body?: unknown }[] = [];
const flush = () => new Promise((r) => setTimeout(r, 0));
const at = '2026-11-17T10:00:00Z';
const conversation = { id: 'c1', title: 'Composer', pinned: false, created: at, lastActivity: at, cursor: '', folders: ['/Users/me/oby'] };
const turn = (id: string, who: 'user' | 'oby', status?: string) => ({ id, who, text: `words ${id}`, at, jobs: [], ...(status ? { delivery: { status } } : {}) });

async function onObid(capabilities: string[]) {
  vi.resetModules();
  calls.length = 0;
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'POST' && path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'reply', text: 'On it.', jobs: [], conversation: 'c1' }) };
    if (method === 'POST' && path === '/v1/plans/cancel') return { status: 200, text: JSON.stringify({ cancelled: true, dropped: (body as { turn: string }).turn === 't3' ? ['t3'] : [] }) };
    if (method === 'GET' && path.startsWith('/v1/conversations/c1/files')) return { status: 200, text: JSON.stringify({ files: [{ folder: '/Users/me/oby', path: 'src/app.ts' }] }) };
    if (path.startsWith('/v1/conversation?')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  };
  const bridge = liveBridge();
  vi.stubGlobal('window', { app: { obid: { call, confirm: async () => ({ status: 200, text: '{}' }), ...bridge.obid } } });
  const live = await import('../live');
  live.connect();
  bridge.sync({ status: { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0, capabilities }, conversations: [conversation] });
  for (let i = 0; i < 5; i++) await flush();
  expect(live.liveState().connection).toBe('live');
  calls.length = 0;
  return { live, bridge };
}
const plans = () => calls.filter((c) => c.method === 'POST' && c.path === '/v1/plans');
const chips = { mode: 'plan', agent: 'claude', model: 'claude-opus-5-5', effort: 'high' };

describe('the box’s words through live.ts', () => {
  // the first import is a cold transform a busy Mac can stretch past a test's own time (#1839)
  beforeAll(async () => { await onObid([]); vi.unstubAllGlobals(); }, 300_000);
  afterEach(() => { vi.unstubAllGlobals(); });

  it('composer_3_the_chips_go_with_the_words', async () => {
    const { live } = await onObid(['composer-v1']);
    live.ask('Fix the login page', 'c1', undefined, undefined, { defaults: chips });
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    expect(plans()[0].body).toMatchObject({ said: 'Fix the login page', conversation: 'c1', defaults: chips });
  });

  it('composer_3_an_obid_without_composer_v1_gets_no_defaults', async () => {
    const { live } = await onObid(['folders-v1']);
    live.ask('Fix the login page', 'c1', undefined, undefined, { defaults: chips });
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    expect(plans()[0].body).not.toHaveProperty('defaults');
  });

  it('composer_8_waiting_words_show_over_the_box', async () => {
    const { live } = await onObid(['composer-v1']);
    const s = live.liveState();
    const of = (turns: unknown[]) => live.chatOf({ ...s, turns: { c1: turns } } as never, conversation as never);
    // words queued behind words a turn has taken: over the box, not in the conversation
    const waiting = of([turn('t1', 'user', 'running'), turn('t2', 'user', 'queued'), turn('t3', 'user', 'queued')]);
    expect(waiting.queued?.map((q) => q.id)).toEqual(['t2', 't3']);
    expect(waiting.said.map((m) => m.text)).toEqual(['words t1']);
    // queued with nothing under way: about to be taken, shown in the conversation as ever
    expect(of([turn('t1', 'user', 'answered'), turn('o1', 'oby'), turn('t2', 'user', 'queued')]).queued).toBeUndefined();
    // a cancelled message shows nowhere; an obid with no delivery, as before
    expect(of([turn('t1', 'user', 'running'), turn('t2', 'user', 'cancelled')]).said.map((m) => m.text)).toEqual(['words t1']);
    expect(of([turn('t1', 'user'), turn('t2', 'user')]).queued).toBeUndefined();
  });

  it('composer_9_cancel_drops_the_waiting_words', async () => {
    const { live } = await onObid(['composer-v1']);
    expect(await live.cancelWords('c1', 't3')).toBe(true);
    expect(calls).toEqual([{ method: 'POST', path: '/v1/plans/cancel', body: { conversation: 'c1', turn: 't3' } }]);
    // a turn took them already: obid dropped nothing
    expect(await live.cancelWords('c1', 't2')).toBe(false);
  });

  it('composer_14_at_lists_the_conversations_files', async () => {
    const on = await onObid(['composer-v1']);
    expect(await on.live.conversationFiles('c1', 'app')).toEqual([{ folder: '/Users/me/oby', path: 'src/app.ts' }]);
    expect(calls.at(-1)).toMatchObject({ method: 'GET', path: '/v1/conversations/c1/files?q=app' });
    const off = await onObid([]);
    expect(await off.live.conversationFiles('c1', 'app')).toEqual([]);
    expect(calls).toEqual([]);
  });
});
