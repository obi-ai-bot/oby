// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Chat } from '../chats';
import { liveBridge } from './liveBridge';

// ⌘K's Restart agent (docs/contracts/composer.md, COMPOSER-28; docs/api/skills.md): in a conversation,
// once obid lists skills-v1, the palette ends the agent waiting for its next turn, and says so in a
// line. The real side panel, main's bridge (window.app.obid.call) stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));

let root: Root | null = null;
let host: HTMLElement;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
});

const at = '2026-11-17T10:00:00Z';
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0, capabilities });

async function palette(capabilities: string[], activeTalk: string | null, restarted = true) {
  const calls: string[] = [];
  const call = vi.fn(async (method: string, path: string) => {
    calls.push(`${method} ${path}`);
    if (method === 'POST' && path === '/v1/conversations/c1/restart') return { status: 200, text: JSON.stringify({ restarted }) };
    if (method === 'GET' && path.startsWith('/v1/conversation')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  vi.resetModules();
  (window as { app?: unknown }).app = { obid: { call, ...bridge.obid }, onSphere: () => () => {}, onFullScreen: () => () => {} };
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const { default: Sidebar } = await import('../Sidebar');
  const live = await import('../live');
  live.connect();
  bridge.sync({ status: STATUS(capabilities), conversations: [{ id: 'c1', title: 'Release notes', pinned: false, created: at, lastActivity: at, cursor: '' }] });
  (await import('../chats')).replaceChats([{ id: 'c1', title: 'Release notes', said: [], at: 5 } as Chat]);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  const none = () => {};
  await act(async () => root!.render(
    <Sidebar open atHome={!activeTalk} activeTalk={activeTalk} width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none}
      onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
  ));
  await act(async () => (host.querySelector('[aria-label="Search"]') as HTMLElement).click());
  const field = host.querySelector<HTMLInputElement>('input[aria-label="Search"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(field, 'restart');
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
  return { calls, live };
}
const restart = () => host.querySelector<HTMLElement>('[data-pick="action:agent.restart"]');

describe('COMPOSER-28: ⌘K restarts the conversation’s agent', () => {
  it('composer_28_cmd_k_restarts_the_conversations_agent', async () => {
    const { calls, live } = await palette(['skills-v1'], 'c1');
    expect(restart()?.textContent).toBe('Restart agent');
    await act(async () => restart()!.click());
    await vi.waitFor(() => expect(live.liveState().problem).toBe('Agent restarted: your next message starts it fresh.'));
    expect(calls).toContain('POST /v1/conversations/c1/restart');
  });

  it('composer_28_nothing_waiting_says_so', async () => {
    const { live } = await palette(['skills-v1'], 'c1', false);
    await act(async () => restart()!.click());
    await vi.waitFor(() => expect(live.liveState().problem).toBe('Nothing to restart: your next message starts the agent fresh anyway.'));
  });

  it('composer_28_only_in_a_conversation_on_an_obid_with_skills_v1', async () => {
    // no skills-v1: not offered
    await palette(['composer-v1'], 'c1');
    expect(restart()).toBeNull();
    await act(async () => root!.unmount());
    root = null;
    // no conversation open: not offered
    await palette(['skills-v1'], null);
    expect(restart()).toBeNull();
  });
});
