// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';
import type { Artifact, Turn } from '../obid';

// the message box and the character: not what these are about
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));

// The pane follows the agent and carries the owner's words (docs/contracts/artifacts-app.md,
// ARTIFACTSAPP-5 to 7), through the real live.ts, the artifacts store and the pane, on main's bridge
// (liveBridge) and a stand-in obid: its Code tab shows each file of the version, a new version from
// the agent moves the pane to it, and words said with it open carry it to obid.

const C = 'c1', ID = '3f0c2a1e-1111-4222-8333-944455556666', OTHER = '9b1d2c3e-2222-4333-8444-a55566667777', IMPORTED = '7e2a9c4d-3333-4444-8555-b66677778888';
const at = '2026-11-21T09:00:00Z';
const version = (n: number) => ({ version: n, created: at, files: ['style.css', 'index.html', 'logo.png'], bytes: 80, job: 'j1' });
const page = (latest: number, id = ID, title = 'page'): Artifact => ({ id, title, kind: 'html', conversation: C, job: 'j1', created: at, versions: Array.from({ length: latest }, (_, i) => version(i + 1)) });
const convo = { id: C, title: 'Billing', pinned: false, created: at, lastActivity: at, cursor: '', folders: [] };
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities });

// Code loads with its first use, and its file is read from obid: on a busy machine both take more
// than waitFor's own second
const waitFor = <T,>(f: () => T | Promise<T>) => vi.waitFor(f, { timeout: 10_000 });
let root: Root | undefined;
beforeAll(async () => { await import('../live'); await import('../artifacts/Pane'); await import('../ChatView'); await import('../Sidebar'); }, 300_000);
// more roots, for the conversation, the side panel or a card beside the pane
let views: Root[] = [];
afterEach(async () => { await act(async () => { views.forEach((v) => v.unmount()); root?.unmount(); }); root = undefined; views = []; document.body.innerHTML = ''; });

// obid as the stand-in holds it: `kept` the artifacts by id, and every call made
// what obid answers a Save, when it refuses one
let refuse: { status: number; text: string } | null = null;
// what else obid lists: its jobs, more conversations, the conversation's turns; search: the page's (?everything: the mock)
type More = { jobs?: unknown[]; conversations?: unknown[]; turns?: Turn[]; search?: string };
async function app(capabilities = ['artifact-turn-v1'], kept: Record<string, Artifact> = { [ID]: page(1) }, more: More = {}) {
  refuse = null;
  vi.resetModules();
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const preview = vi.fn(async () => ({ shown: true as const }));
  const exported = vi.fn(async (_id: string, _v: number, as: string) => ({ saved: `/Users/me/Downloads/page-v1.${as}` }));
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'POST' && path === '/v1/artifacts/import') {
      if (refuse) return refuse;
      const made = { ...page(1, IMPORTED, 'Imported'), conversation: null, job: null };
      kept[IMPORTED] = made;
      return { status: 201, text: JSON.stringify(made) };
    }
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
    if (path === '/v1/artifacts') return { status: 200, text: JSON.stringify(Object.values(kept)) };
    const of = /^\/v1\/artifacts\?conversation=([^&]+)$/.exec(path);
    if (of) return { status: 200, text: JSON.stringify(Object.values(kept).filter((a) => a.conversation === of[1])) };
    if (path.startsWith('/v1/artifacts?')) return { status: 200, text: JSON.stringify(Object.values(kept)) };
    if (method === 'POST' && path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'reply', text: 'On it.', jobs: [], conversation: C }) };
    if (method === 'GET' && path.startsWith(`/v1/conversation?id=${C}&`)) return { status: 200, text: JSON.stringify(more.turns ?? []) };
    if (method === 'GET' && path.startsWith('/v1/conversation')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  (window as { app?: unknown }).app = { obid: { call, ...bridge.obid }, artifactPreview: preview, artifactExport: exported, onFullScreen: () => () => {}, onSphere: () => () => {} };
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: more.search ?? '' });
  const live = await import('../live');
  const store = await import('../artifacts/store');
  const { ArtifactPane } = await import('../artifacts/Pane');
  live.connect();
  bridge.sync({ status: STATUS(capabilities), conversations: [convo, ...(more.conversations ?? [])], jobs: more.jobs ?? [] });
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<ArtifactPane width={480} />));
  return { calls, preview, exported, kept, bridge, live, store, host };
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

describe('ARTIFACTSAPP-11', () => {
  it('artifactsapp_11_download_saves_the_versions_zip_or_html_where_the_owner_picks_and_copy_takes_the_file', async () => {
    const { store, host, preview, exported } = await app(undefined, { [ID]: page(2) });
    await act(async () => store.openArtifact(page(2), 1));
    const exportButton = host.querySelector('[aria-label="Export"]') as HTMLButtonElement;
    await act(async () => exportButton.click());
    // Download, as board 1c has it; Preview's view steps aside while the menu is open
    expect([...host.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent)).toEqual(['Project archive (.zip)', 'Standalone HTML']);
    expect(host.querySelector('.art-export-head')!.textContent).toBe('Download');
    expect(preview).toHaveBeenLastCalledWith(null);
    // Esc closes it, Preview comes back and focus goes back to Export; a click inside it keeps it open
    await act(async () => { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); });
    expect(host.querySelector('[role="menu"]')).toBeNull();
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 1 }));
    expect(document.activeElement).toBe(exportButton);
    await act(async () => exportButton.click());
    await act(async () => { host.querySelector('[role="menuitem"]')!.dispatchEvent(new Event('pointerdown', { bubbles: true })); });
    expect(host.querySelector('[role="menu"]')).not.toBeNull();
    // a click outside it closes it, as the window changing size does
    for (const away of [() => document.body.dispatchEvent(new Event('pointerdown', { bubbles: true })), () => window.dispatchEvent(new Event('resize'))]) {
      if (!host.querySelector('[role="menu"]')) await act(async () => exportButton.click());
      await act(async () => { away(); });
      expect(host.querySelector('[role="menu"]')).toBeNull();
      expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 1 }));
    }
    await act(async () => exportButton.click());
    await act(async () => (host.querySelector('[role="menuitem"]') as HTMLButtonElement).click());
    // main gets the artifact, the version on view and which: nothing else
    expect(exported).toHaveBeenLastCalledWith(ID, 1, 'zip');
    await waitFor(() => expect(host.querySelector('.art-saved')?.textContent).toBe('Saved to /Users/me/Downloads/page-v1.zip'));
    expect(host.querySelector('[role="menu"]')).toBeNull();
    expect(preview).toHaveBeenLastCalledWith(expect.objectContaining({ id: ID, version: 1 }));
    await act(async () => exportButton.click());
    await act(async () => ([...host.querySelectorAll('[role="menuitem"]')][1] as HTMLButtonElement).click());
    expect(exported).toHaveBeenLastCalledWith(ID, 1, 'html');
    // a cancelled save says nothing; one main couldn't make says why
    exported.mockImplementationOnce(async () => ({ cancelled: true }) as never);
    await act(async () => exportButton.click());
    await act(async () => (host.querySelector('[role="menuitem"]') as HTMLButtonElement).click());
    await waitFor(() => expect(host.querySelector('.art-saved')).toBeNull());
    // Copy, in Code: the file on view
    await act(async () => tabNamed(host, 'Code').click());
    await waitFor(() => expect(host.querySelector('[aria-label="Copy index.html"]')).not.toBeNull());
  });
});

describe('ARTIFACTSAPP-12', () => {
  // a file dropped on the conversation's thread, as the owner's drag delivers it
  async function drop(target: Element, file: File) {
    const data = { items: [{ kind: 'file' }], files: [file], dropEffect: 'none' };
    for (const type of ['dragover', 'drop']) {
      const e = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(e, 'dataTransfer', { value: data });
      await act(async () => { target.dispatchEvent(e); });
    }
  }
  async function thread() {
    const { useImportDrop } = await import('../artifacts/Import');
    return function Thread() {
      const { handlers, bar } = useImportDrop(true);
      return <div className="chat-scroll" data-thread {...handlers}>{bar}</div>;
    };
  }

  it('artifactsapp_12_a_dropped_page_is_imported_on_import_and_opens_beside_the_conversation', async () => {
    const { host, calls } = await app(undefined, {});
    const Thread = await thread();
    const where = document.createElement('div');
    document.body.append(where);
    const other = createRoot(where);
    await act(async () => other.render(<Thread />));
    const bytes = new TextEncoder().encode('<!doctype html><h1>Mine</h1>');
    await drop(where.querySelector('[data-thread]')!, new File([bytes], 'mine.html', { type: 'text/html' }));
    expect(where.querySelector('[data-artifact-import]')!.textContent).toBe('Import mine.html as an artifact?Import');
    // nothing goes before the owner's click on Import
    expect(calls.some((c) => c.path === '/v1/artifacts/import')).toBe(false);
    await act(async () => button(where, 'Import')!.click());
    await waitFor(() => expect(host.querySelector('.art-strip-title b')?.textContent).toBe('Imported'));
    const sent = calls.find((c) => c.path === '/v1/artifacts/import')!;
    expect(sent.method).toBe('POST');
    expect([...(sent.body as Uint8Array)]).toEqual([...bytes]);
    expect(where.querySelector('[data-artifact-import]')).toBeNull();
    await act(async () => other.unmount());
  });

  it('artifactsapp_12_only_an_archive_a_page_or_a_document_is_offered_and_a_refusal_says_why', async () => {
    const { calls } = await app(undefined, {});
    const Thread = await thread();
    const where = document.createElement('div');
    document.body.append(where);
    const other = createRoot(where);
    await act(async () => other.render(<Thread />));
    await drop(where.querySelector('[data-thread]')!, new File([new Uint8Array([0x89, 0x50])], 'chart.png'));
    expect(where.querySelector('[data-artifact-import]')).toBeNull();
    refuse = { status: 422, text: JSON.stringify({ error: 'Not published: a file holds what looks like a secret.' }) };
    await drop(where.querySelector('[data-thread]')!, new File(['# Notes'], 'notes.md'));
    await act(async () => button(where, 'Import')!.click());
    await waitFor(() => expect(where.querySelector('[data-artifact-import]')!.textContent).toBe('Not published: a file holds what looks like a secret.'));
    expect(calls.filter((c) => c.path === '/v1/artifacts/import')).toHaveLength(1);
    await act(async () => other.unmount());
  });
});

// The card in the message that tells of it, and the row under its conversation (ARTIFACTSAPP-16, 17):
// the real ChatView and Sidebar on the same live.ts, store and stand-in obid
const none = () => {};
async function see(node: React.ReactNode) {
  const where = document.createElement('div');
  document.body.append(where);
  const v = createRoot(where);
  views.push(v);
  await act(async () => v.render(<>{node}</>));
  return where;
}
const conversation = async () => { const { default: ChatView } = await import('../ChatView'); return see(<ChatView kind="oby" id={C} leaving={false} onBack={none} onGone={none} />); };
const panel = async (opened: string[] = []) => {
  const { default: Sidebar } = await import('../Sidebar');
  return see(<Sidebar open atHome width={362} activeTalk={C} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none}
    onOpenMember={none} onOpenTalk={(id) => opened.push(id)} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />);
};
const turn = (id: string, who: 'user' | 'oby', text: string, minute: number, more: Partial<Turn> = {}): Turn => ({ id, who, text, at: `2026-11-21T09:0${minute}:00Z`, jobs: [], conversation: C, ...more });
const job = (id: string, more: Record<string, unknown> = {}) => ({
  id, request: 'Make a page', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/page', base: 'dev', issue: null, kind: 'work',
  status: 'running', step: 'Working', summary: '', created: at, finished: null, commits: 0, published: null, rounds: 1, round: 1, check: null,
  model: null, dismissed: null, permission: null, pipeline: null, title: 'Make a page', archived: null, conversation: C, turn: 'u1', ...more,
});
const messageSaying = (where: HTMLElement, words: string) => [...where.querySelectorAll<HTMLElement>('.chat-thread > .msg')].find((m) => m.querySelector('.msg-bubble')?.textContent?.includes(words))!;
const before = (a: Element, b: Element) => !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);

describe('ARTIFACTSAPP-16', () => {
  it('artifactsapp_16_an_artifact_oby_published_shows_in_the_message_that_published_it', async () => {
    const turns = [turn('u1', 'user', 'Make a page', 1), turn('o1', 'oby', 'Here it is.', 2, { artifacts: [{ id: ID, version: 1 }] }),
      turn('u2', 'user', 'Make it red', 3), turn('o2', 'oby', 'Red now.', 4, { artifacts: [{ id: ID, version: 2 }] })];
    await app(undefined, { [ID]: { ...page(2), job: null } }, { turns });
    const where = await conversation();
    await waitFor(() => expect(where.querySelectorAll('.art-card')).toHaveLength(1));
    const end = messageSaying(where, 'Here it is.').querySelector(':scope > .msg-end')!;
    const bubble = end.querySelector(':scope > .msg-bubble')!, card = end.querySelector(':scope > button.art-card')!, time = end.querySelector('.msg-time')!;
    expect(before(bubble, card) && before(card, time)).toBe(true);
    // the later turn naming v2 adds none: the one card stays where it first showed, on v2
    expect(messageSaying(where, 'Red now.').querySelector('.art-card')).toBeNull();
    expect(card.querySelector('.art-card-meta')!.textContent).toBe('Oby artifact · Page · v2');
  });

  it('artifactsapp_16_a_tasks_artifact_shows_in_the_message_that_reports_its_end_not_under_its_card', async () => {
    const turns = [turn('u1', 'user', 'Make a page', 1), turn('o1', 'oby', 'On it.', 2, { jobs: ['j1'] }), turn('o2', 'oby', 'The page is done.', 5, { ended: true, jobs: ['j1'] }),
      // Oby's next words, in the same run: the report keeps its card with its words all the same
      turn('o3', 'oby', 'Anything else?', 6)];
    await app(undefined, { [ID]: page(1) }, { turns, jobs: [job('j1', { status: 'done', finished: at })] });
    const where = await conversation();
    await waitFor(() => expect(where.querySelectorAll('.art-card')).toHaveLength(1));
    const work = where.querySelector('.msg-task-work')!;
    expect(work.querySelector('.art-card')).toBeNull();
    expect(work.closest('.msg')!.querySelector('.art-card')).toBeNull();
    // and no wrapper of a task's card and its artifacts: only the card's own pieces are named for it
    expect(new Set([...where.querySelectorAll('[class*="art-card"]')].flatMap((e) => [...e.classList].filter((c) => c.startsWith('art-card')))))
      .toEqual(new Set(['art-card', 'art-card-row', 'art-card-text', 'art-card-title', 'art-card-meta']));
    const card = messageSaying(where, 'The page is done.').querySelector('.msg-end > .msg-bubble + button.art-card')!;
    expect(card.querySelector('.art-card-meta')!.textContent).toBe('Oby artifact · Page · v1 · made by Claude Code');
  });

  it('artifactsapp_16_with_no_message_to_hold_it_the_card_shows_after_the_last_turn', async () => {
    const A1 = '11111111-1111-4111-8111-111111111111', A2 = '22222222-2222-4222-8222-222222222222', A3 = '33333333-3333-4333-8333-333333333333';
    const kept = { [A1]: { ...page(1, A1, 'one'), job: null }, [A2]: { ...page(1, A2, 'two'), job: 'j2' }, [A3]: { ...page(1, A3, 'three'), job: null } };
    const turns = [turn('u1', 'user', 'Make a page', 1), turn('o1', 'oby', 'On it.', 2, { jobs: ['j2'] }),
      turn('o2', 'oby', 'Published.', 3, { artifacts: [null, 3, { version: 1 }] as never })];
    await app(undefined, kept, { turns, jobs: [job('j2')] });
    const where = await conversation();
    await waitFor(() => expect(where.querySelectorAll('.art-card')).toHaveLength(3));
    const last = [...where.querySelectorAll('.chat-thread > [role="listitem"]')].at(-1)!;
    expect([...last.querySelectorAll('.art-card-title')].map((t) => t.textContent)).toEqual(['one', 'two', 'three']);
    expect(messageSaying(where, 'Published.').querySelector('.art-card')).toBeNull();
    expect(messageSaying(where, 'On it.').querySelector('.art-card')).toBeNull();
  });
});

const artRows = (where: HTMLElement) => [...where.querySelectorAll<HTMLButtonElement>('button.convo-kid.art-kid')];
const artRow = (where: HTMLElement, title: string) => artRows(where).find((r) => r.querySelector('b')!.textContent === title)!;
const withDot = (where: HTMLElement) => artRows(where).filter((r) => r.querySelector(':scope > span.status[role="img"][aria-label="New"] > span.status-dot.is-new')).map((r) => r.querySelector('b')!.textContent);

describe('ARTIFACTSAPP-17', () => {
  it('artifactsapp_17_the_new_dot_shows_until_the_owner_opens_it', async () => {
    const NEW = '44444444-4444-4444-8444-444444444444', NOTES = '55555555-5555-4555-8555-555555555555';
    const kept: Record<string, Artifact> = { [ID]: page(1), [NOTES]: page(1, NOTES, 'notes') };
    const { bridge, host } = await app(undefined, kept);
    const opened: string[] = [];
    const where = await panel(opened);
    await waitFor(() => expect(artRows(where).map((r) => r.querySelector('b')!.textContent)).toEqual(['page', 'notes']));
    expect(withDot(where)).toEqual([]);
    // a new artifact, a new version of another, and a rename
    kept[NEW] = page(1, NEW, 'site');
    await act(async () => bridge.event('artifact', { id: NEW, version: 1, title: 'site', kind: 'html', conversation: C, job: 'j1' }));
    kept[ID] = page(2);
    await act(async () => bridge.event('artifact', { id: ID, version: 2, title: 'page', kind: 'html', conversation: C, job: 'j1' }));
    kept[NOTES] = page(1, NOTES, 'Notes');
    await act(async () => bridge.event('artifact', { id: NOTES, renamed: true, title: 'Notes', kind: 'html', conversation: C, job: null }));
    await waitFor(() => expect(artRows(where).map((r) => r.querySelector('b')!.textContent)).toEqual(['page', 'Notes', 'site']));
    expect(withDot(where)).toEqual(['page', 'site']);
    // opened from its card, and from its row: each dot goes
    const { ArtifactCard } = await import('../artifacts/Card');
    const card = await see(<ArtifactCard artifact={kept[NEW]} />);
    await act(async () => (card.querySelector('button.art-card') as HTMLButtonElement).click());
    expect(withDot(where)).toEqual(['page']);
    await act(async () => artRow(where, 'page').click());
    expect(withDot(where)).toEqual([]);
    expect(opened).toEqual([C]);
    expect(artRow(where, 'page').className).toContain('is-on');
    expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v2');
    // after a restart, nothing is New
    await act(async () => { views.forEach((v) => v.unmount()); root?.unmount(); });
    views = [];
    await app(undefined, kept);
    const again = await panel();
    await waitFor(() => expect(artRows(again)).toHaveLength(3));
    expect(withDot(again)).toEqual([]);
  });

  it('artifactsapp_17_the_side_panel_reads_the_artifacts_once_not_per_row', async () => {
    const listed = (calls: { method: string; path: string }[]) => calls.filter((c) => c.method === 'GET' && c.path === '/v1/artifacts').length;
    const three = [{ ...convo, id: 'c2', title: 'Landing' }, { ...convo, id: 'c3', title: 'Notes' }];
    const kept = { [ID]: page(1), [OTHER]: { ...page(1, OTHER, 'notes'), conversation: 'c2' } };
    const { calls, bridge } = await app(undefined, kept, { conversations: three });
    await panel();
    await waitFor(() => expect(listed(calls)).toBe(1));
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    expect(listed(calls)).toBe(1);
    expect(calls.some((c) => c.path.startsWith('/v1/artifacts?'))).toBe(false);
    await act(async () => bridge.event('artifact', { id: ID, version: 2, title: 'page', kind: 'html', conversation: C, job: 'j1' }));
    await waitFor(() => expect(listed(calls)).toBe(2));
    await act(async () => bridge.event('artifact', { id: OTHER, renamed: true, title: 'Notes', kind: 'html', conversation: 'c2', job: null }));
    await waitFor(() => expect(listed(calls)).toBe(3));
    // on the mock: none
    await act(async () => { views.forEach((v) => v.unmount()); root?.unmount(); });
    views = [];
    const mock = await app(undefined, kept, { search: '?everything&chat' });
    await panel();
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    expect(listed(mock.calls)).toBe(0);
  });
});
