// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';

// T3 Code's fix 7a03a74189 ("messages cut by a restart no longer stay marked as streaming"), held to
// CONVERSATION-AGENT-50 and LIVE (docs/contracts/live-state.md): once obid goes down, nothing in the
// window says a conversation is still at work or still writing.

afterEach(() => { delete (window as { app?: unknown }).app; vi.resetModules(); });

const at = '2026-11-17T10:00:00Z';
const convo = { id: 'c1', title: 'Plan', pinned: false, created: at, lastActivity: at, cursor: '' };

describe('T3: obid going down mid-turn', () => {
  it('t3_live_obid_going_down_mid_turn_leaves_nothing_working_or_writing', async () => {
    const bridge = liveBridge();
    (window as { app?: unknown }).app = { obid: { call: async () => ({ status: 404, text: '{}' }), ...bridge.obid }, onFullScreen: () => () => {} };
    const live = await import('../live');
    live.connect();
    bridge.sync({ conversations: [convo] });
    bridge.event('turn', { id: 'u1', who: 'user', text: 'Plan it', at, jobs: [], conversation: 'c1' });
    bridge.event('step', { conversation: 'c1', turn: 'u1', text: 'Reading a.ts', state: 'running' });
    bridge.event('delta', { conversation: 'c1', turn: 'u1', text: 'Half an ans' });
    const s = live.liveState();
    expect([s.working.c1, s.words.c1]).toEqual(['Reading a.ts', 'Half an ans']);
    bridge.send({ kind: 'connection', connection: 'down', problem: 'Oby’s engine stopped.' });
    const down = live.liveState();
    expect([down.working.c1, down.words.c1, down.pace.c1]).toEqual([undefined, undefined, undefined]);
    // back up: still nothing under way until obid says so again
    bridge.sync({ conversations: [convo] });
    expect(live.liveState().working.c1).toBeUndefined();
  });
});
