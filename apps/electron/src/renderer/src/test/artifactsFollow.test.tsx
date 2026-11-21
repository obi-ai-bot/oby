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

// Code loads with its first use, and its file is read from obid: on a busy machine both take more
// than waitFor's own second
const waitFor = <T,>(f: () => T | Promise<T>) => vi.waitFor(f, { timeout: 10_000 });
let root: Root | undefined;
beforeAll(async () => { await import('../live'); await import('../artifacts/Pane'); }, 300_000);
afterEach(async () => { await act(async () => root?.unmount()); root = undefined; document.body.innerHTML = ''; });

// obid as the stand-in holds it: `kept` the artifacts by id, and every call made
// what obid answers a Save, when it refuses one
let refuse: { status: number; text: string } | null = null;
async function app(capabilities = ['artifact-turn-v1'], kept: Record<string, Artifact> = { [ID]: page(1) }) {
  refuse = null;
  vi.resetModules();
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const preview = vi.fn(async () => ({ shown: true as const }));
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const saved = /^\/v1\/artifacts\/([^/]+)\/versions$/.exec(path);
    if (method === 'POST' && saved) {
      if (refuse) return refuse;
      const a = kept[saved[1]], next = a.versions.length + 1;
      kept[saved[1]] = { ...a, versions: [...a.versions, { ...version(next), job: undefined }] };
      return { status: 201, text: JSON.stringify({ id: saved[1], version: next }) };
    }
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
const saves = (calls: { method: string; path: string; body?: unknown }[]) => calls.filter((c) => c.method === 'POST' && c.path.endsWith('/versions')).map((c) => c.body);
const tabNamed = (host: HTMLElement, name: string) => [...host.querySelectorAll('[role="tab"]')].find((b) => b.textContent?.replace(' •', '') === name) as HTMLButtonElement;
const button = (host: HTMLElement, name: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === name) as HTMLButtonElement | undefined;
// the owner typing: the whole of the file on view in Code becomes `text`
async function type(host: HTMLElement, path: string, text: string) {
  const { EditorView } = await import('@codemirror/view');
  await waitFor(() => expect(host.querySelector(`[data-artifact-code="${path}"] .cm-editor`)).not.toBeNull());
  const view = EditorView.findFromDOM(host.querySelector(`[data-artifact-code="${path}"] .cm-editor`) as HTMLElement)!;
  await act(async () => view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } }));
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
    await waitFor(() => expect([...host.querySelectorAll('.art-file')].map((b) => b.textContent)).toEqual(['index.html', 'logo.png', 'style.css']));
    await waitFor(() => expect(host.querySelector('[data-artifact-code="index.html"] .cm-content')?.textContent).toBe('<h1>Hello v1</h1>'));
    // the latest version: the owner's to edit (ARTIFACTSAPP-8; an older one is read-only, ARTIFACTSAPP-9)
    expect(host.querySelector('.cm-content')!.getAttribute('contenteditable')).toBe('true');
    await act(async () => tab('style.css').click());
    await waitFor(() => expect(host.querySelector('[data-artifact-code="style.css"] .cm-content')?.textContent).toBe('h1 { color: teal } /* v1 */'));
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
    await waitFor(() => expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v2'));
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 2 }));
    // a rename retitles it
    kept[ID] = { ...page(2), title: 'Landing' };
    await act(async () => bridge.event('artifact', { id: ID, renamed: true, title: 'Landing', kind: 'html', conversation: C, job: null }));
    await waitFor(() => expect(host.querySelector('.art-strip-title b')!.textContent).toBe('Landing'));
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
    await waitFor(() => expect(host.querySelector('.art-strip-title b')?.textContent).toBe('page'));
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
    await waitFor(() => expect(plans(calls)).toHaveLength(1));
    expect(plans(calls)[0]).toMatchObject({ said: 'Make the heading red', conversation: C, artifact: { id: ID, version: 1 } });
    // closed, the words go without it
    await act(async () => store.closeArtifact());
    live.ask('Thanks', C);
    await waitFor(() => expect(plans(calls)).toHaveLength(2));
    expect(plans(calls)[1]).not.toHaveProperty('artifact');
  });

  it('artifactsapp_7_not_to_an_obid_without_it_nor_to_another_conversation', async () => {
    const older = await app([], { [ID]: page(1) });
    await act(async () => older.store.openArtifact(page(1)));
    older.live.ask('Make the heading red', C);
    await waitFor(() => expect(plans(older.calls)).toHaveLength(1));
    expect(plans(older.calls)[0]).not.toHaveProperty('artifact');
    const theirs = await app();
    await act(async () => theirs.store.openArtifact({ ...page(1), conversation: 'c2' }));
    expect(theirs.store.openArtifactOf(C)).toBeNull();
    expect(theirs.store.openArtifactOf('c2')).toEqual({ id: ID, version: 1 });
  });
});

describe('ARTIFACTSAPP-8', () => {
  it('artifactsapp_8_an_edit_shows_in_preview_before_it_is_saved_and_save_makes_the_next_version', async () => {
    const { store, host, preview, calls } = await app();
    await act(async () => store.openArtifact(page(1)));
    await act(async () => tabNamed(host, 'Code').click());
    await waitFor(() => expect(tabNamed(host, 'style.css')).toBeTruthy());
    await act(async () => tabNamed(host, 'style.css').click());
    await waitFor(() => expect(host.querySelector('[data-artifact-code="style.css"] .cm-content')?.textContent).toBe('h1 { color: teal } /* v1 */'));
    expect(button(host, 'Save as v2')).toBeUndefined();
    await type(host, 'style.css', 'h1 { color: crimson }');
    // the file is marked, and Save names the version it makes
    expect(tabNamed(host, 'style.css').className).toContain('is-edited');
    expect(button(host, 'Save as v2')).toBeTruthy();
    // Preview shows it as it would be once saved: main lays the edit over the version's files
    await act(async () => tabNamed(host, 'Preview').click());
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 1, edits: { 'style.css': 'h1 { color: crimson }' } }));
    await act(async () => button(host, 'Save as v2')!.click());
    expect(saves(calls)).toEqual([{ base: 1, files: { 'style.css': 'h1 { color: crimson }' } }]);
    await waitFor(() => expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v2'));
    expect(button(host, 'Save as v3')).toBeUndefined();
    expect(preview).toHaveBeenLastCalledWith({ id: ID, version: 2, bounds: expect.anything() });
  });

  it('artifactsapp_8_typing_the_text_back_unedits_it', async () => {
    const { store, host } = await app();
    await act(async () => store.openArtifact(page(1)));
    await act(async () => tabNamed(host, 'Code').click());
    await waitFor(() => expect(host.querySelector('[data-artifact-code="index.html"] .cm-content')?.textContent).toBe('<h1>Hello v1</h1>'));
    await type(host, 'index.html', '<h1>Hi</h1>');
    expect(button(host, 'Save as v2')).toBeTruthy();
    await type(host, 'index.html', '<h1>Hello v1</h1>');
    expect(button(host, 'Save as v2')).toBeUndefined();
    expect(tabNamed(host, 'index.html').className).not.toContain('is-edited');
  });
});

describe('ARTIFACTSAPP-9', () => {
  it('artifactsapp_9_the_version_menu_opens_any_version_the_latest_marked_an_older_one_read_only', async () => {
    const { store, host, preview } = await app(undefined, { [ID]: page(2) });
    await act(async () => store.openArtifact(page(2)));
    const menu = host.querySelector('select[aria-label="Version"]') as HTMLSelectElement;
    expect([...menu.options].map((o) => o.textContent)).toEqual(['v2, the latest', 'v1']);
    expect(menu.value).toBe('2');
    await act(async () => { menu.value = '1'; menu.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v1');
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 1 }));
    await act(async () => tabNamed(host, 'Code').click());
    await waitFor(() => expect(host.querySelector('[data-artifact-code="index.html"] .cm-content')?.textContent).toBe('<h1>Hello v1</h1>'));
    expect(host.querySelector('.cm-content')!.getAttribute('contenteditable')).toBe('false');
    // one version: no menu
    const one = await app();
    await act(async () => one.store.openArtifact(page(1)));
    expect(one.host.querySelector('select[aria-label="Version"]')).toBeNull();
  });
});

describe('ARTIFACTSAPP-10', () => {
  it('artifactsapp_10_a_version_that_comes_meanwhile_keeps_the_edits_and_obids_refusal_shows_with_them_kept', async () => {
    const { store, host, kept, bridge, calls } = await app();
    await act(async () => store.openArtifact(page(1)));
    await act(async () => tabNamed(host, 'Code').click());
    await type(host, 'index.html', '<h1>Mine</h1>');
    // the agent's version 2 comes while the owner's edit is unsaved: the pane stays, the edit too
    kept[ID] = page(2);
    await act(async () => bridge.event('artifact', { id: ID, version: 2, title: 'page', kind: 'html', conversation: C, job: 'j2' }));
    await waitFor(() => expect((host.querySelector('select[aria-label="Version"]') as HTMLSelectElement | null)?.options.length).toBe(2));
    expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v1');
    expect(host.querySelector('[data-artifact-code="index.html"] .cm-content')?.textContent).toBe('<h1>Mine</h1>');
    // no Save on an older version: it would be refused; the pane says the latest came
    expect(button(host, 'Save as v2')).toBeUndefined();
    expect(host.querySelector('.art-refused')?.textContent).toBe('Version 2 came after the one you’re editing.');
    // a Save obid refuses (its secret scan): its words show, the edit stays
    await act(async () => store.showVersion(2));
    await act(async () => tabNamed(host, 'Code').click());
    await type(host, 'index.html', '<h1>ghp_made_up</h1>');
    refuse = { status: 422, text: JSON.stringify({ error: 'Not saved: a file holds what looks like a secret.' }) };
    await act(async () => button(host, 'Save as v3')!.click());
    await waitFor(() => expect(host.querySelector('.art-refused')?.textContent).toBe('Not saved: a file holds what looks like a secret.'));
    expect(button(host, 'Save as v3')).toBeTruthy();
    expect(host.querySelector('[data-artifact-code="index.html"] .cm-content')?.textContent).toBe('<h1>ghp_made_up</h1>');
    // and a 409, a version that came after: said, kept
    refuse = { status: 409, text: JSON.stringify({ error: 'Version 3 came after the one this edit was made on: open it, then edit.' }) };
    await act(async () => button(host, 'Save as v3')!.click());
    await waitFor(() => expect(host.querySelector('.art-refused')?.textContent).toBe('Version 3 came after the one this edit was made on: open it, then edit.'));
    expect(saves(calls)).toHaveLength(2);
    expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v2');
  });
});

