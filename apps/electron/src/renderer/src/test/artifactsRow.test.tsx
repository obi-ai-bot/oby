// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';
import type { Artifact } from '../obid';

// An artifact's row under its conversation (docs/contracts/artifacts-app.md, ARTIFACTSAPP-17): the
// whole of Home, its side panel and the conversations it opens, on the real live.ts and artifacts
// store, with main's bridge stood in for as obid. A row's click from another conversation opens its
// conversation and the artifact beside it, which stays open once the conversation left has gone.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));

const at = (minute: number) => `2026-11-21T09:${String(minute).padStart(2, '0')}:00Z`;
const A = 'c1', B = 'c2', ONLY = 'c3';
const PAGE = '3f0c2a1e-1111-4222-8333-944455556666', NOTES = '9b1d2c3e-2222-4333-8444-a55566667777';
const artifact = (id: string, title: string, conversation: string, job: string | null): Artifact => ({ id, title, kind: 'html', conversation, job, created: at(0),
  versions: [1, 2].map((version) => ({ version, created: at(version), files: ['index.html'], bytes: 40 })) });
const KEPT = [artifact(PAGE, 'page', B, 'jb'), artifact(NOTES, 'notes', ONLY, null)];
const convo = (id: string, title: string, minute: number) => ({ id, title, pinned: false, created: at(0), lastActivity: at(minute), cursor: '', folders: [] });
const job = (id: string, conversation: string) => ({
  id, request: 'Make a page', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/page', base: 'dev', issue: null, kind: 'work',
  status: 'done', step: '', summary: '', created: at(1), finished: at(2), commits: 0, published: null, rounds: 1, round: 1, check: null,
  model: null, dismissed: null, permission: null, pipeline: null, title: 'Make a page', archived: null, conversation, turn: null,
});

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
});

const until = async (check: () => void) => { await act(async () => { await vi.waitFor(check, { timeout: 10_000 }); }); };
const click = async (el: Element) => { await act(async () => (el as HTMLElement).click()); };

describe('ARTIFACTSAPP-17', () => {
  it('artifactsapp_17_the_row_under_its_conversation_says_what_it_is_and_opens_it', async () => {
    const call = vi.fn(async (method: string, path: string) => {
      if (method === 'GET' && path === '/v1/artifacts') return { status: 200, text: JSON.stringify(KEPT) };
      const of = /^\/v1\/artifacts\?conversation=([^&]+)$/.exec(path);
      if (of) return { status: 200, text: JSON.stringify(KEPT.filter((a) => a.conversation === of[1])) };
      if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: '[]' };
      return { status: 404, text: '{"error":"Not found."}' };
    });
    const bridge = liveBridge(call);
    (window as { app?: unknown }).app = { obid: { call, ...bridge.obid }, artifactPreview: async () => ({ shown: true }), onFullScreen: () => () => {}, onSphere: () => () => {} };
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('location', { search: '' });
    localStorage.setItem('oby.setUp', '1');
    const { default: Home } = await import('../Home');
    const live = await import('../live');
    live.connect();
    // A, the latest, is where the window opens; B has a task and its page; ONLY has an artifact alone
    bridge.sync({ status: { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0, capabilities: [] },
      conversations: [convo(A, 'Billing', 30), convo(B, 'Landing', 20), convo(ONLY, 'Notes', 10)], jobs: [job('jb', B)] });
    const host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<Home />));
    const nest = (title: string) => [...host.querySelectorAll('.convo-nest')].find((n) => n.querySelector('.connector-name')?.textContent === title)!;
    const layer = () => host.querySelector('.chat-layer:not(.is-leaving) .chat')?.getAttribute('aria-label');
    await until(() => expect(layer()).toBe('Billing'));

    // the conversation with only an artifact: its twist, and its group
    await until(() => expect(nest('Notes').querySelector('.convo-twist')).not.toBeNull());
    expect(nest('Notes').className).toContain('has-kids');
    await click(nest('Notes').querySelector('.convo-twist')!);
    expect(nest('Notes').querySelectorAll('.convo-kids > button.convo-kid.art-kid')).toHaveLength(1);

    // B's: after its task's row, in its group
    await click(nest('Landing').querySelector('.convo-twist')!);
    const kids = [...nest('Landing').querySelectorAll('.convo-kids > button.convo-kid')];
    expect(kids.map((k) => k.classList.contains('art-kid'))).toEqual([false, true]);
    const row = kids[1] as HTMLButtonElement;
    expect(row.title).toBe('page, made in this conversation');
    const icon = row.querySelector(':scope > .art-kid-icon > svg.lucide-frame')!;
    expect([icon.getAttribute('width'), icon.getAttribute('height')]).toEqual(['14', '14']);
    expect(row.querySelector('.convo-kid-text')!.innerHTML).toBe('<b>page</b> · Page');
    expect(row.className).not.toContain('is-on');

    // its click, from A: B opens, and its page beside it on its latest version, still open once A has
    // gone its way out (LEAVE_MS)
    await click(row);
    await until(() => expect(layer()).toBe('Landing'));
    await act(async () => { await new Promise((r) => setTimeout(r, 400)); });
    expect(host.querySelector('.chat-layer.is-leaving')).toBeNull();
    expect(host.querySelector('.art-strip-title b')!.textContent).toBe('page');
    expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v2');
    expect([...nest('Landing').querySelectorAll('button.art-kid')][0].className).toContain('is-on');
  });
});
