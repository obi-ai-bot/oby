// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';
import type { Artifact } from '../obid';

// The pane follows the agent and carries the owner's words (docs/contracts/artifacts-app.md,
// ARTIFACTSAPP-5 to 7), through the real live.ts, the artifacts store and the pane, on main's bridge
// (liveBridge) and a stand-in obid: its Code tab shows each file of the version, a new version from
// the agent moves the pane to it, and words said with it open carry it to obid.

const C = 'c1', ID = '3f0c2a1e-1111-4222-8333-944455556666', OTHER = '9b1d2c3e-2222-4333-8444-a55566667777';
const at = '2026-11-21T09:00:00Z';
const version = (n: number) => ({ version: n, created: at, files: ['style.css', 'index.html', 'logo.png'], bytes: 80, job: 'j1' });
const page = (latest: number, id = ID, title = 'page'): Artifact => ({ id, title, kind: 'html', conversation: C, job: 'j1', created: at, versions: Array.from({ length: latest }, (_, i) => version(i + 1)) });
const convo = { id: C, title: 'Billing', pinned: false, created: at, lastActivity: at, cursor: '', folders: [] };
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities });

let root: Root | undefined;
beforeAll(async () => { await import('../live'); await import('../artifacts/Pane'); }, 300_000);
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; });

// obid as the stand-in holds it: `kept` the artifacts by id, and every call made
async function app(capabilities = ['artifact-turn-v1'], kept: Record<string, Artifact> = { [ID]: page(1) }) {
  vi.resetModules();
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const preview = vi.fn(async () => ({ shown: true as const }));
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const file = /^\/v1\/artifacts\/([^/]+)\/(\d+)\/(.+)$/.exec(path);
    if (file) return { status: 200, text: file[3] === 'index.html' ? `<h1>Hello v${file[2]}</h1>` : `h1 { color: teal } /* v${file[2]} */` };
    const one = /^\/v1\/artifacts\/([^/?]+)$/.exec(path);
    if (one) return kept[one[1]] ? { status: 200, text: JSON.stringify(kept[one[1]]) } : { status: 404, text: '{"error":"There\'s no such artifact."}' };
    if (path.startsWith('/v1/artifacts?')) return { status: 200, text: JSON.stringify(Object.values(kept)) };
    if (method === 'POST' && path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'reply', text: 'On it.', jobs: [], conversation: C }) };
    if (method === 'GET' && path.startsWith('/v1/conversation')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  (window as { app?: unknown }).app = { obid: { call, ...bridge.obid }, artifactPreview: preview, onFullScreen: () => () => {}, onSphere: () => () => {} };
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const live = await import('../live');
  const store = await import('../artifacts/store');
  const { ArtifactPane } = await import('../artifacts/Pane');
  live.connect();
  bridge.sync({ status: STATUS(capabilities), conversations: [convo] });
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<ArtifactPane width={480} />));
  return { calls, preview, kept, bridge, live, store, host };
}
const plans = (calls: { method: string; path: string; body?: unknown }[]) => calls.filter((c) => c.method === 'POST' && c.path === '/v1/plans').map((c) => c.body as Record<string, unknown>);

describe('ARTIFACTSAPP-5', () => {
  it('artifactsapp_5_code_shows_each_file_of_the_version_index_first_and_preview_stops', async () => {
    const { store, host, preview, calls } = await app();
    await act(async () => store.openArtifact(page(1)));
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 1 }));
    const tab = (name: string) => [...host.querySelectorAll('[role="tab"]')].find((b) => b.textContent === name) as HTMLButtonElement;
    await act(async () => tab('Code').click());
    // Preview's view goes while Code shows
    expect(preview).toHaveBeenLastCalledWith(null);
    // (Code loads with its first use)
    await vi.waitFor(() => expect([...host.querySelectorAll('.art-file')].map((b) => b.textContent)).toEqual(['index.html', 'logo.png', 'style.css']));
    await vi.waitFor(() => expect(host.querySelector('[data-artifact-code="index.html"] .cm-content')?.textContent).toBe('<h1>Hello v1</h1>'));
    // read-only, as obid holds it
    expect(host.querySelector('.cm-content')!.getAttribute('contenteditable')).toBe('false');
    await act(async () => tab('style.css').click());
    await vi.waitFor(() => expect(host.querySelector('[data-artifact-code="style.css"] .cm-content')?.textContent).toBe('h1 { color: teal } /* v1 */'));
    // a picture is named, never read as text
    await act(async () => tab('logo.png').click());
    expect(host.querySelector('.art-code-body')!.textContent).toBe('logo.png isn’t text: Preview shows it.');
    expect(calls.some((c) => c.path.endsWith('/logo.png'))).toBe(false);
    // back to Preview, its view comes back
    await act(async () => tab('Preview').click());
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 1 }));
  });
});

describe('ARTIFACTSAPP-6', () => {
  it('artifactsapp_6_a_new_version_from_the_agent_moves_the_open_pane_to_it', async () => {
    const { store, host, preview, kept, bridge } = await app();
    await act(async () => store.openArtifact(page(1)));
    kept[ID] = page(2);
    await act(async () => bridge.event('artifact', { id: ID, version: 2, title: 'page', kind: 'html', conversation: C, job: 'j2' }));
    await vi.waitFor(() => expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v2'));
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 2 }));
    // a rename retitles it
    kept[ID] = { ...page(2), title: 'Landing' };
    await act(async () => bridge.event('artifact', { id: ID, renamed: true, title: 'Landing', kind: 'html', conversation: C, job: null }));
    await vi.waitFor(() => expect(host.querySelector('.art-strip-title b')!.textContent).toBe('Landing'));
  });

  it('artifactsapp_6_an_older_version_on_view_stays_and_another_artifact_moves_nothing', async () => {
    const { store, host, kept, bridge } = await app(undefined, { [ID]: page(2), [OTHER]: page(1, OTHER, 'notes') });
    await act(async () => store.openArtifact(page(2), 1));
    kept[ID] = page(3);
    await act(async () => bridge.event('artifact', { id: ID, version: 3, title: 'page', kind: 'html', conversation: C, job: 'j2' }));
    await act(async () => bridge.event('artifact', { id: OTHER, version: 2, title: 'notes', kind: 'html', conversation: C, job: 'j2' }));
    await act(async () => { await new Promise((r) => setTimeout(r, 20)); });
    expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v1');
    expect(host.querySelector('.art-strip-title b')!.textContent).toBe('page');
  });

  it('artifactsapp_6_an_artifact_the_agent_makes_in_the_conversation_on_view_opens', async () => {
    const { store, host, kept, bridge } = await app(undefined, {});
    store.viewing(C);
    kept[ID] = page(1);
    await act(async () => bridge.event('artifact', { id: ID, version: 1, title: 'page', kind: 'html', conversation: C, job: 'j1' }));
    await vi.waitFor(() => expect(host.querySelector('.art-strip-title b')?.textContent).toBe('page'));
    // another conversation's opens nothing; nor does one with none on view
    await act(async () => store.closeArtifact());
    kept[OTHER] = { ...page(1, OTHER, 'notes'), conversation: 'c2' };
    await act(async () => bridge.event('artifact', { id: OTHER, version: 1, title: 'notes', kind: 'html', conversation: 'c2', job: 'j3' }));
    store.viewing(null);
    await act(async () => bridge.event('artifact', { id: ID, version: 1, title: 'page', kind: 'html', conversation: C, job: 'j1' }));
    await act(async () => { await new Promise((r) => setTimeout(r, 20)); });
    expect(host.querySelector('.art-strip-title')).toBeNull();
  });
});

describe('ARTIFACTSAPP-7', () => {
  it('artifactsapp_7_words_said_with_the_pane_open_carry_its_artifact_and_version', async () => {
    const { store, live, calls } = await app(undefined, { [ID]: page(2) });
    await act(async () => store.openArtifact(page(2), 1));
    live.ask('Make the heading red', C);
    await vi.waitFor(() => expect(plans(calls)).toHaveLength(1));
    expect(plans(calls)[0]).toMatchObject({ said: 'Make the heading red', conversation: C, artifact: { id: ID, version: 1 } });
    // closed, the words go without it
    await act(async () => store.closeArtifact());
    live.ask('Thanks', C);
    await vi.waitFor(() => expect(plans(calls)).toHaveLength(2));
    expect(plans(calls)[1]).not.toHaveProperty('artifact');
  });

  it('artifactsapp_7_not_to_an_obid_without_it_nor_to_another_conversation', async () => {
    const older = await app([], { [ID]: page(1) });
    await act(async () => older.store.openArtifact(page(1)));
    older.live.ask('Make the heading red', C);
    await vi.waitFor(() => expect(plans(older.calls)).toHaveLength(1));
    expect(plans(older.calls)[0]).not.toHaveProperty('artifact');
    const theirs = await app();
    await act(async () => theirs.store.openArtifact({ ...page(1), conversation: 'c2' }));
    expect(theirs.store.openArtifactOf(C)).toBeNull();
    expect(theirs.store.openArtifactOf('c2')).toEqual({ id: ID, version: 1 });
  });
});
