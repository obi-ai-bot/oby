// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';
import { wordsOf } from '../../../shared/words';

// A conversation's workspace in the app (docs/contracts/folders.md, WORKSPACE-2 to 5): Home's box
// offers "This folder" or "A new worktree" with folders picked, and sends the choice with the words
// that start the conversation, in a shape main signs (WORDS-19); a conversation's finishing line
// shows what obid says each folder can do, and a click goes through main's own dialog and
// signature (window.app.obid.finish). The real Home and FinishLine on live.ts, main's bridge stood
// in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, handle }: { label: string; onText: (t: string) => void; handle: React.Ref<unknown> }) {
    const field = useRef<HTMLTextAreaElement>(null);
    useImperativeHandle(handle, () => ({
      text: () => field.current?.value ?? '', clear: () => { if (field.current) field.current.value = ''; },
      setText: (t: string) => { if (field.current) field.current.value = t; }, focus: () => {}, blur: () => {}, mention: () => {},
    }));
    return <textarea ref={field} aria-label={label} data-box="" onChange={(e) => onText(e.currentTarget.value)} />;
  },
}));

let root: Root | null = null;
let host: HTMLElement;
vi.setConfig({ testTimeout: 120_000 });
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
});

type Call = { method: string; path: string; body?: unknown };
type Answer = { status: number; text: string };
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0, capabilities });
const API = '/Users/me/a/api';
const place = (actions: string[], more: object = {}) => ({ folder: API, name: 'api', path: API, worktree: false, branch: 'main', actions, ...more });

function obid(workspace: unknown = { workspace: 'folder', places: [] }) {
  const calls: Call[] = [];
  const said = { workspace };
  const call = vi.fn(async (method: string, path: string, body?: unknown): Promise<Answer> => {
    calls.push({ method, path, body });
    if (method === 'POST' && path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'none', jobs: [] }) };
    if (method === 'GET' && path === '/v1/conversations/c1/workspace') return { status: 200, text: JSON.stringify(said.workspace) };
    if (method === 'GET' && path === '/v1/projects') return { status: 200, text: '[]' };
    if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const finish = vi.fn(async (_c: string, _folder: string, _action: string): Promise<Answer> => ({ status: 200, text: JSON.stringify(said.workspace) }));
  const bridge = liveBridge(call);
  const app = { obid: { call, finish, ...bridge.obid }, onSphere: () => () => {}, sphereLeave: async () => null, onFullScreen: () => () => {}, pickFolders: vi.fn(async () => [API]) };
  return { app, calls, said, bridge, finish };
}

async function live(o: ReturnType<typeof obid>, capabilities: string[]) {
  (window as { app?: unknown }).app = o.app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  localStorage.setItem('oby.setUp', '1');
  const l = await import('../live');
  l.connect();
  o.bridge.sync({ status: STATUS(capabilities), conversations: [] });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
}

const until = async (check: () => void) => { await act(async () => { await vi.waitFor(check); }); };
const click = async (el: Element) => { await act(async () => (el as HTMLElement).click()); };
const button = (text: string, within: ParentNode = host) => [...within.querySelectorAll('button')].find((b) => b.textContent === text || b.getAttribute('aria-label') === text);
const homeBox = () => host.querySelector('form.composer:not(.chat-composer)')!;
const choice = () => homeBox().querySelector('[role="radiogroup"][aria-label="Where it works"]');
const checkedChoice = () => [...(choice()?.querySelectorAll('[role="radio"]') ?? [])].filter((r) => r.getAttribute('aria-checked') === 'true').map((r) => r.textContent);
const plans = (calls: Call[]) => calls.filter((c) => c.method === 'POST' && c.path === '/v1/plans').map((c) => c.body as Record<string, unknown>);
async function send(words: string) {
  const field = homeBox().querySelector<HTMLTextAreaElement>('[data-box]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field, words);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await act(async () => { homeBox().dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
}
async function home(o: ReturnType<typeof obid>, capabilities: string[]) {
  await live(o, capabilities);
  const { default: Home } = await import('../Home');
  await act(async () => root!.render(<Home />));
}
async function pickFolder() {
  await click(button('Add to the message', homeBox())!);
  await click(button('Select working folder…', homeBox())!);
  await until(() => expect(homeBox().querySelector('.composer-tag')?.textContent).toBe('api'));
}

describe('WORKSPACE-3: Home’s box chooses where the conversation works', () => {
  it('workspace_3_the_choice_goes_with_the_first_words_in_a_shape_main_signs', async () => {
    const o = obid();
    await home(o, ['folders-v1', 'workspace-v1']);
    // no folder, no choice
    expect(choice()).toBeNull();
    await pickFolder();
    expect([...choice()!.querySelectorAll('[role="radio"]')].map((r) => r.textContent)).toEqual(['This folder', 'A new worktree']);
    expect(checkedChoice()).toEqual(['This folder']);
    await click(button('A new worktree', homeBox())!);
    expect(checkedChoice()).toEqual(['A new worktree']);
    await send('Tidy the notes');
    await until(() => expect(plans(o.calls)).toHaveLength(1));
    const body = plans(o.calls)[0];
    expect(body).toMatchObject({ said: 'Tidy the notes', new: true, folders: [API], workspace: 'worktree' });
    // the shape main signs: its words alone, the choice with the body (WORDS-19)
    expect(wordsOf('POST', '/v1/plans', body)).toEqual(['Tidy the notes']);
  });

  it('workspace_3_left_alone_it_is_this_folder', async () => {
    const o = obid();
    await home(o, ['folders-v1', 'workspace-v1']);
    await pickFolder();
    await send('Tidy the notes');
    await until(() => expect(plans(o.calls)).toHaveLength(1));
    expect(plans(o.calls)[0]).toMatchObject({ new: true, workspace: 'folder' });
    expect(wordsOf('POST', '/v1/plans', plans(o.calls)[0])).toEqual(['Tidy the notes']);
  });

  it('workspace_3_an_obid_without_workspace_v1_gets_no_choice', async () => {
    const o = obid();
    await home(o, ['folders-v1']);
    await pickFolder();
    expect(choice()).toBeNull();
    await send('Tidy the notes');
    await until(() => expect(plans(o.calls)).toHaveLength(1));
    expect(plans(o.calls)[0]).not.toHaveProperty('workspace');
  });
});

describe('WORKSPACE-4: the finishing line', () => {
  async function line(o: ReturnType<typeof obid>, capabilities = ['workspace-v1']) {
    await live(o, capabilities);
    const { FinishLine } = await import('../ChatView');
    await act(async () => root!.render(<FinishLine conversation="c1" busy={false} />));
  }
  const group = () => host.querySelector('[role="group"][aria-label="Finish"]');
  const shown = () => [...(group()?.querySelectorAll('button') ?? [])].map((b) => b.textContent);

  it('workspace_4_one_button_per_action_obid_names_and_a_click_goes_through_main', async () => {
    const o = obid({ workspace: 'folder', places: [place(['commit'])] });
    await line(o);
    await until(() => expect(shown()).toEqual(['Commit']));
    // main asks in its own dialog and signs: the page only names the action and the folder
    o.said.workspace = { workspace: 'folder', places: [place(['publishRepository'])] };
    await click(button('Commit')!);
    expect(o.finish).toHaveBeenCalledWith('c1', API, 'commit');
    expect(o.calls.filter((c) => c.method === 'POST')).toEqual([]);
    // obid's answer is the line now: once committed, Publish repository
    await until(() => expect(shown()).toEqual(['Publish repository']));
  });

  it('workspace_4_a_refusal_is_said_and_nothing_changes', async () => {
    const o = obid({ workspace: 'folder', places: [place(['commit'])] });
    o.finish.mockResolvedValueOnce({ status: 409, text: JSON.stringify({ error: 'Nothing done: you said not now.' }) });
    await line(o);
    await until(() => expect(shown()).toEqual(['Commit']));
    await click(button('Commit')!);
    await until(() => expect(group()!.querySelector('[role="alert"]')?.textContent).toBe('Nothing done: you said not now.'));
    expect(shown()).toEqual(['Commit']);
  });

  it('workspace_4_a_folder_worked_in_only_from_the_room_says_why_and_offers_nothing', async () => {
    const why = 'Oby couldn’t make a worktree of /Users/me/a/api (it’s in the way), so this conversation works only in its own room there.';
    const o = obid({ workspace: 'worktree', places: [place([], { path: null, branch: null, why })] });
    await line(o);
    await until(() => expect(group()?.textContent).toBe(why));
    expect(shown()).toEqual([]);
  });

  it('workspace_4_nothing_shows_without_workspace_v1', async () => {
    const o = obid({ workspace: 'folder', places: [place(['commit'])] });
    await line(o, []);
    expect(group()).toBeNull();
    expect(o.calls.filter((c) => c.path.endsWith('/workspace'))).toEqual([]);
  });
});
