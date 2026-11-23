// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';
import { AGENTS_SHOWN } from '../panelFlags';

// Conversations under their project, and a project to pick, make or change (docs/contracts/projects.md,
// PROJECTS-16 to 31): the real Home, with its Sidebar and Composer, on what obid lists through
// live.ts, main's bridge (window.app.obid.call) stood in for as obid.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));
// the box's words: a plain field holding them, as the real editor's handle gives them
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

async function reset() {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
}
afterEach(async () => {
  await reset();
  localStorage.clear();
});

type Project = { id: string; name: string; folders: string[]; repos: string[] };
const OBY: Project = { id: '6f1c2b9e-0000-4000-8000-000000000001', name: 'Oby app', folders: ['/Users/me/oby'], repos: ['obi-ai-bot/oby', 'obi-ai-bot/oby-plan'] };
const TETRIX: Project = { id: '6f1c2b9e-0000-4000-8000-000000000002', name: 'Tetrix', folders: ['/Users/me/tetrix'], repos: ['deskree-inc/tetrix-ee', 'deskree-inc/tetrix-front-end', 'deskree-inc/tetrix-collectors'] };
const UNLISTED = '6f1c2b9e-0000-4000-8000-0000000000aa';
type Answer = { status: number; text: string };
type Call = { method: string; path: string; body?: unknown };
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0, capabilities });
const at = '2026-11-16T10:00:00Z';
const convo = (id: string, title: string, more: object = {}) => ({ id, title, pinned: false, created: at, lastActivity: at, cursor: '', project: null, ...more });

// main's bridge, as obid answers: its projects (a create is listed, a change kept, a delete gone), a
// conversation's move (its answer, or a refusal), words (taken), its repositories and one helper; the rest 404
function obid(projects: Project[], conversations: ReturnType<typeof convo>[] = []) {
  const said = { projects, create: undefined as Answer | undefined, patch: undefined as Answer | undefined, edit: undefined as Answer | undefined, del: undefined as Answer | undefined,
    repos: [{ repo: 'o/website-cms', defaultBranch: 'main', listed: true }, { repo: 'o/docs', defaultBranch: 'main', listed: true }] };
  const calls: Call[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (method === 'GET' && path === '/v1/projects') return { status: 200, text: JSON.stringify(said.projects) };
    if (method === 'POST' && path === '/v1/projects') {
      if (said.create) return said.create;
      const made = { id: `6f1c2b9e-0000-4000-8000-0000000000f${said.projects.length}`, ...(body as object) } as Project;
      said.projects = [...said.projects, made];
      return { status: 201, text: JSON.stringify(made) };
    }
    const project = path.match(/^\/v1\/projects\/([^/]+)$/);
    if (method === 'PATCH' && project) {
      if (said.edit) return said.edit;
      const kept = { ...said.projects.find((p) => p.id === project[1])!, ...(body as object) };
      said.projects = said.projects.map((p) => (p.id === kept.id ? kept : p));
      return { status: 200, text: JSON.stringify(kept) };
    }
    if (method === 'DELETE' && project) {
      if (said.del) return said.del;
      said.projects = said.projects.filter((p) => p.id !== project[1]);
      return { status: 204, text: '' };
    }
    const moved = path.match(/^\/v1\/conversations\/([^/]+)$/);
    if (method === 'PATCH' && moved) {
      if (said.patch) return said.patch;
      const c = conversations.find((x) => x.id === moved[1])!;
      return { status: 200, text: JSON.stringify({ ...c, ...(body as object), revision: 2 }) };
    }
    if (method === 'POST' && path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'none', jobs: [] }) };
    if (method === 'GET' && path === '/v1/repos') return { status: 200, text: JSON.stringify({ repos: said.repos }) };
    if (method === 'GET' && path === '/v1/helpers') return { status: 200, text: JSON.stringify([{ id: 'h1', profile: 'p', name: 'Researcher', color: '#c2b5fd', instructions: 'x', tool: 'claude', connectors: [], conversation: 'conv-h1', created: at }]) };
    if (method === 'GET' && path.startsWith('/v1/conversation?id=')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  const app = { obid: { call, ...bridge.obid }, onSphere: () => () => {}, sphereLeave: async () => null, onFullScreen: () => () => {}, pickFolders: vi.fn(async () => ['/Users/me/website']) };
  return { app, calls, said, bridge, conversations };
}

// Home, the window's whole page, on what obid lists
async function showHome(o: ReturnType<typeof obid>, capabilities = ['folders-v1', 'projects-v1']) {
  (window as { app?: unknown }).app = o.app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  localStorage.setItem('oby.setUp', '1');
  const { default: Home } = await import('../Home');
  const live = await import('../live');
  live.connect();
  o.bridge.sync({ status: STATUS(capabilities), conversations: o.conversations });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Home />));
}
// Home on the mock (?everything&chat)
async function showMock() {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '?everything&chat' });
  localStorage.setItem('oby.setUp', '1');
  const { default: Home } = await import('../Home');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Home />));
}

const until = async (check: () => void) => { await act(async () => { await vi.waitFor(check); }); };
const click = async (el: Element) => { await act(async () => (el as HTMLElement).click()); };
const buttons = (within: ParentNode = host) => [...within.querySelectorAll('button')];
const button = (text: string, within: ParentNode = host) => buttons(within).find((b) => b.textContent === text || b.getAttribute('aria-label') === text);
async function fill(selector: string, text: string) {
  const field = host.querySelector<HTMLInputElement | HTMLTextAreaElement>(selector)!;
  const proto = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  await act(async () => {
    Object.getOwnPropertyDescriptor(proto, 'value')!.set!.call(field, text);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
}
// the Projects side: what it lists, in order (a heading, or a row's title)
const panel = () => host.querySelector('#panel-side-panel')!;
const listed = () => [...panel().querySelectorAll('.sidebar-group-head h3, .convo-row .connector-name, .project-new')].map((e) => (e.classList.contains('project-new') ? '+ New project' : e.tagName === 'H3' ? `# ${e.textContent}` : e.textContent));
const group = (name: string) => [...panel().querySelectorAll('.project-group')].find((g) => g.querySelector('h3')!.textContent === name)!;
const rowsOf = (name: string) => [...group(name).querySelectorAll('.convo-row .connector-name')].map((e) => e.textContent);
// Home's box and its chip; a conversation's title bar chip
const homeBox = () => host.querySelector('form.composer:not(.chat-composer)')!;
const homeChip = () => homeBox().querySelector<HTMLButtonElement>('.project-chip');
const barChip = () => host.querySelector<HTMLButtonElement>('.bar .project-chip');
const chipName = (chip: Element | null) => chip?.querySelector('.project-chip-name')?.textContent;
const menu = () => host.querySelector('.row-menu');
const choices = () => [...menu()!.querySelectorAll('.project-choice[role="menuitemradio"]')];
const choice = (name: string) => choices().find((c) => c.querySelector('.project-choice-name')!.textContent === name)!;
const shownChoices = () => choices().map((c) => [c.querySelector('.project-choice-name')!.textContent, c.querySelector('.project-choice-repos')?.textContent ?? '']);
const checked = () => choices().filter((c) => c.getAttribute('aria-checked') === 'true').map((c) => c.querySelector('.project-choice-name')!.textContent);
const dialog = () => host.querySelector('[role="dialog"], [role="alertdialog"]');
const sheetRows = () => [...host.querySelectorAll('.project-row')].map((r) => [r.querySelector('.project-row-name')!.textContent, r.querySelector('.project-row-where')!.textContent, r.getAttribute('title')]);
const addPanel = (form: Element) => form.querySelector('.add-panel');
const posted = (calls: Call[], path: string) => calls.filter((c) => c.method === 'POST' && c.path === path);
const layer = () => host.querySelector('.chat-layer:not(.is-leaving)');
async function send(form: Element, words: string) {
  const field = form.querySelector<HTMLTextAreaElement>('[data-box]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field, words);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await act(async () => { form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
}
async function openRow(title: string) {
  const row = [...panel().querySelectorAll('.convo-row')].find((r) => r.querySelector('.connector-name')!.textContent === title)!;
  await click(row);
  await until(() => expect(host.querySelector('.crumb.is-here')?.textContent).toBe(title));
}

describe('PROJECTS-16', () => {
  it('PROJECTS-16: projects_16_no_projects', async () => {
    const o = obid([], [convo('c1', 'Talk one'), convo('c2', 'Talk two')]);
    await showHome(o);
    await until(() => expect(o.calls.some((c) => c.path === '/v1/projects')).toBe(true));
    // as before: no group, no No project; New project the last row
    expect(listed()).toEqual(['Talk one', 'Talk two', '+ New project']);
    expect(panel().querySelector('.project-group')).toBeNull();
    expect(panel().lastElementChild!.textContent).toBe('New project');
    expect(panel().lastElementChild!.querySelector('svg')).not.toBeNull();
    expect(chipName(homeChip())).toBe('No project');
  });
});

describe('PROJECTS-17', () => {
  it('PROJECTS-17: projects_17_one_project', async () => {
    const o = obid([OBY], [convo('c1', 'In the app', { project: OBY.id, tasks: ['j1'] }), convo('c2', 'Elsewhere', { tasks: ['j2'] })]);
    await showHome(o);
    await until(() => expect(listed()).toEqual(['# Oby app', 'In the app', '# No project', 'Elsewhere', '+ New project']));
    // the project's heading has its folder mark; No project none
    expect(group('Oby app').querySelector('.sidebar-group-head svg.project-mark')).not.toBeNull();
    expect(group('No project').querySelector('.project-mark')).toBeNull();
    // each row as it was: its second line, and its ⋮
    for (const [name, title] of [['Oby app', 'In the app'], ['No project', 'Elsewhere']]) {
      const row = group(name).querySelector('.convo-row')!;
      expect(row.querySelector('.connector-about')!.textContent).toBe('Started 1 task');
      expect(button(`More for ${title}`, row)).toBeDefined();
    }
  });
});

describe('PROJECTS-18', () => {
  it('PROJECTS-18: projects_18_several_folding_and_search', async () => {
    const talks = [
      convo('c1', 'App one', { project: OBY.id }), convo('c2', 'Retry jitter', { project: TETRIX.id }),
      convo('c3', 'Graph cache', { project: TETRIX.id, pinned: true }), convo('c4', 'Loose talk'), convo('c5', 'Lost one', { project: UNLISTED }),
    ];
    await showHome(obid([OBY, TETRIX], talks));
    await until(() => expect(listed()).toEqual([
      '# Oby app', 'App one', '# Tetrix', 'Graph cache', 'Retry jitter', '# No project', 'Loose talk', 'Lost one', '+ New project',
    ]));
    // Tetrix folds by its heading, and stays folded
    const head = () => group('Tetrix').querySelector('.sidebar-group-head')!;
    expect(head().getAttribute('aria-expanded')).toBe('true');
    await click(head());
    expect(head().getAttribute('aria-expanded')).toBe('false');
    expect(group('Tetrix').className).toContain('is-folded');
    expect(JSON.parse(localStorage.getItem('oby.projectsFolded')!)).toEqual([TETRIX.id]);
    await click(group('No project').querySelector('.sidebar-group-head')!);
    expect(JSON.parse(localStorage.getItem('oby.projectsFolded')!)).toEqual([TETRIX.id, '']);
    await click(group('No project').querySelector('.sidebar-group-head')!);

    await reset();
    await showHome(obid([OBY, TETRIX], talks));
    await until(() => expect(listed()[0]).toBe('# Oby app'));
    expect(group('Tetrix').className).toContain('is-folded');
    expect(head().getAttribute('aria-expanded')).toBe('false');
    expect(group('No project').className).not.toContain('is-folded');

    // a search matching only a row in Tetrix: Tetrix alone, open
    await click(button('Search')!);
    await fill('.sidebar-search input', 'jitter');
    expect(listed()).toEqual(['# Tetrix', 'Retry jitter', '+ New project']);
    expect(group('Tetrix').className).not.toContain('is-folded');
    expect(head().getAttribute('aria-expanded')).toBe('true');
  });
});

describe('PROJECTS-19', () => {
  it('PROJECTS-19: projects_19_the_menu', async () => {
    const o = obid([OBY, TETRIX]);
    await showHome(o);
    await until(() => expect(o.calls.some((c) => c.path === '/v1/projects')).toBe(true));
    // a folder picked in the box first: it goes once a project is chosen
    await click(button('Add to the message', homeBox())!);
    await click(button('Select working folder…', homeBox())!);
    await until(() => expect(homeBox().querySelector('.composer-tag')?.textContent).toBe('website'));

    const reads = o.calls.filter((c) => c.path === '/v1/projects').length;
    await click(homeChip()!);
    expect(homeChip()!.getAttribute('aria-expanded')).toBe('true');
    // obid's list again as it opens
    await until(() => expect(o.calls.filter((c) => c.path === '/v1/projects').length).toBe(reads + 1));
    expect(shownChoices()).toEqual([['Oby app', '2 repos'], ['Tetrix', '3 repos'], ['No project', '']]);
    expect(choice('Oby app').querySelector('svg')).not.toBeNull();
    expect(checked()).toEqual(['No project']);
    expect([...menu()!.children].map((e) => e.tagName)).toEqual(['BUTTON', 'BUTTON', 'BUTTON', 'HR', 'BUTTON']);
    expect(menu()!.lastElementChild!.textContent).toBe('New project…');

    await click(choice('Tetrix'));
    expect(menu()).toBeNull();
    expect(chipName(homeChip())).toBe('Tetrix');
    expect(homeBox().querySelector('.composer-tag')).toBeNull();
    await click(button('Add to the message', homeBox())!);
    expect(addPanel(homeBox())).not.toBeNull();
    expect(button('Select working folder…', homeBox())).toBeUndefined();
    await click(homeChip()!);
    expect(checked()).toEqual(['Tetrix']);
    await click(homeChip()!);

    await send(homeBox(), 'Fix the retry jitter');
    await until(() => expect(posted(o.calls, '/v1/plans')).toHaveLength(1));
    const body = posted(o.calls, '/v1/plans')[0].body as Record<string, unknown>;
    expect(body).toMatchObject({ said: 'Fix the retry jitter', new: true, project: TETRIX.id });
    expect(body).not.toHaveProperty('folders');
  });
});

describe('PROJECTS-20', () => {
  it('PROJECTS-20: projects_20_the_sheet', async () => {
    const o = obid([OBY]);
    await showHome(o);
    await until(() => expect(button('New project')).toBeDefined());
    await click(button('New project')!);
    expect(dialog()!.querySelector('h2')!.textContent).toBe('New project');
    expect(dialog()!.querySelector('.modal-lede')!.textContent).toBe('A project is the folders and repositories its conversations work in.It stays in this profile.');
    expect([...dialog()!.querySelectorAll('.modal-label')].map((l) => l.textContent)).toEqual(['Name', 'Folders and repositories']);
    expect(buttons(dialog()!).map((b) => b.textContent)).toEqual(['Add a folder…', 'Cancel', 'Create project']);
    const create = () => button('Create project', dialog()!)!;
    expect(create().disabled).toBe(true);
    await fill('#project-name', '   ');
    expect(create().disabled).toBe(true);
    await fill('#project-name', 'Website');

    // a folder from main's own dialog (the same one again is kept once); × takes one out
    await click(button('Add a folder…', dialog()!)!);
    await until(() => expect(sheetRows()).toEqual([['website', 'On this Mac', '/Users/me/website']]));
    await click(button('Add a folder…', dialog()!)!);
    await until(() => expect(o.app.pickFolders).toHaveBeenCalledTimes(2));
    expect(sheetRows()).toHaveLength(1);
    await click(button('Remove website', dialog()!)!);
    expect(sheetRows()).toEqual([]);
    await click(button('Add a folder…', dialog()!)!);
    await until(() => expect(sheetRows()).toEqual([['website', 'On this Mac', '/Users/me/website']]));

    await click(create());
    await until(() => expect(dialog()).toBeNull());
    expect(posted(o.calls, '/v1/projects')).toEqual([{ method: 'POST', path: '/v1/projects', body: { name: 'Website', folders: ['/Users/me/website'], repos: [] } }]);
    expect(listed().filter((l) => l.startsWith('#'))).toEqual(['# Oby app', '# Website']);
    await click(homeChip()!);
    await until(() => expect(shownChoices().map((c) => c[0])).toEqual(['Oby app', 'Website', 'No project']));
    expect(checked()).toEqual(['No project']);

    // from Home's chip, the one made is the chosen one
    await click(button('New project…')!);
    expect(menu()).toBeNull();
    await fill('#project-name', 'Docs');
    await click(button('Create project', dialog()!)!);
    await until(() => expect(dialog()).toBeNull());
    expect(chipName(homeChip())).toBe('Docs');
    expect(posted(o.calls, '/v1/projects').at(-1)!.body).toEqual({ name: 'Docs', folders: [], repos: [] });

    // Cancel sends nothing
    const sent = posted(o.calls, '/v1/projects').length;
    await click(button('New project')!);
    await fill('#project-name', 'Nothing');
    await click(button('Cancel', dialog()!)!);
    expect(dialog()).toBeNull();
    expect(posted(o.calls, '/v1/projects')).toHaveLength(sent);
    expect(posted(o.calls, '/v1/plans')).toHaveLength(0);
  });
});

describe('PROJECTS-21', () => {
  it('PROJECTS-21: projects_21_a_refused_create', async () => {
    const o = obid([OBY, TETRIX]);
    o.said.create = { status: 422, text: '{"error":"/Users/me/website is one of the places Oby never works in."}' };
    await showHome(o);
    await until(() => expect(listed().filter((l) => l.startsWith('#'))).toEqual(['# Oby app', '# Tetrix']));
    await click(button('New project')!);
    await fill('#project-name', 'Website');
    await click(button('Add a folder…', dialog()!)!);
    await until(() => expect(sheetRows()).toHaveLength(1));
    await click(button('Create project', dialog()!)!);
    await until(() => expect(dialog()!.querySelector('[role="alert"]')?.textContent).toBe('/Users/me/website is one of the places Oby never works in.'));
    expect(dialog()).not.toBeNull();
    expect(host.querySelector<HTMLInputElement>('#project-name')!.value).toBe('Website');
    expect(sheetRows()).toEqual([['website', 'On this Mac', '/Users/me/website']]);
    expect(listed().filter((l) => l.startsWith('#'))).toEqual(['# Oby app', '# Tetrix']);
    await click(button('Cancel', dialog()!)!);
    await click(homeChip()!);
    await until(() => expect(shownChoices().map((c) => c[0])).toEqual(['Oby app', 'Tetrix', 'No project']));
  });
});

describe('PROJECTS-22', () => {
  it('PROJECTS-22: projects_22_the_title_bar_chip', async () => {
    const talks = [convo('c1', 'Two fixes for today', { project: OBY.id, folders: OBY.folders }), convo('c2', 'Loose talk'), convo('conv-h1', 'Researcher', { helper: 'h1' })];
    const o = obid([OBY, TETRIX], talks);
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    await openRow('Two fixes for today');
    await until(() => expect(chipName(barChip())).toBe('Oby app'));

    const patches = () => o.calls.filter((c) => c.method === 'PATCH');
    await click(barChip()!);
    await click(choice('Tetrix'));
    await until(() => expect(chipName(barChip())).toBe('Tetrix'));
    expect(patches()).toEqual([{ method: 'PATCH', path: '/v1/conversations/c1', body: { project: TETRIX.id } }]);
    expect(rowsOf('Tetrix')).toEqual(['Two fixes for today']);
    await click(barChip()!);
    await click(choice('No project'));
    await until(() => expect(chipName(barChip())).toBe('No project'));
    expect(patches()[1].body).toEqual({ project: null });
    // refused: obid's words, and the chip as it was
    o.said.patch = { status: 422, text: '{"error":"No project has that id."}' };
    await click(barChip()!);
    await click(choice('Oby app'));
    await until(() => expect(host.querySelector('.connection')?.textContent).toBe('No project has that id.'));
    expect(chipName(barChip())).toBe('No project');
    o.said.patch = undefined;
    await click(barChip()!);
    await click(choice('Oby app'));
    await until(() => expect(chipName(barChip())).toBe('Oby app'));

    // in its project: no Select working folder… (its folders: PROJECTS-25)
    const box = () => layer()!.querySelector('form.chat-composer')!;
    await click(button('Add to the message', box())!);
    expect(addPanel(box())).not.toBeNull();
    expect(button('Select working folder…', box())).toBeUndefined();
    // (one in no project offers it)
    await openRow('Loose talk');
    await click(button('Add to the message', box())!);
    expect(button('Select working folder…', box())).toBeDefined();
    expect(o.calls.some((c) => c.method === 'PATCH' && (c.body as object) && 'folders' in (c.body as object))).toBe(false);

    // a helper's conversation: no chip (under Agents, hidden for now: panelFlags.ts)
    if (!AGENTS_SHOWN) return;
    await click([...host.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((t) => t.textContent === 'Agents')!);
    await until(() => expect(host.querySelector('.agent-row')).not.toBeNull());
    await click(host.querySelector('.agent-row')!);
    await until(() => expect(host.querySelector('.crumbs')?.textContent).toContain('Researcher'));
    expect(barChip()).toBeNull();
  });
});

describe('PROJECTS-25', () => {
  it('PROJECTS-25: projects_25_no_folder_chip_in_a_projects_box', async () => {
    const talks = [convo('c1', 'Two fixes for today', { project: OBY.id, folders: OBY.folders }), convo('c2', 'Loose talk', { folders: ['/Users/me/scratch'] })];
    const o = obid([OBY, TETRIX], talks);
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    const box = () => layer()!.querySelector('form.chat-composer')!;
    // in a project: its title bar's chip, and no folder tag (nor any tag row) in its box
    await openRow('Two fixes for today');
    await until(() => expect(chipName(barChip())).toBe('Oby app'));
    expect(box().querySelector('.composer-tag')).toBeNull();
    expect(box().querySelector('.composer-tags')).toBeNull();
    expect(box().querySelector('.project-chip')).toBeNull();
    // words to it go with no folders
    await send(box(), 'Next step');
    await until(() => expect(posted(o.calls, '/v1/plans')).toHaveLength(1));
    expect(posted(o.calls, '/v1/plans')[0].body).not.toHaveProperty('folders');
    // in no project: its own folders, each with its ×, and Select working folder… to pick more
    await openRow('Loose talk');
    await until(() => expect([...box().querySelectorAll('.composer-tag')].map((t) => t.textContent)).toEqual(['scratch']));
    expect(button('Remove scratch', box())).toBeDefined();
    await click(button('Add to the message', box())!);
    expect(button('Select working folder…', box())).toBeDefined();
    // Home's box keeps its project chip (Home has no title bar to show it)
    expect(homeChip()).not.toBeNull();
  });
});

// a project's heading in the side panel, its ⋮, and the menu it opens
const headMore = (name: string) => button(`More for ${name}`, group(name))!;
async function editFromPanel(name: string) {
  await click(headMore(name));
  await click(button('Edit project…')!);
  await until(() => expect(dialog()!.querySelector('h2')!.textContent).toBe('Edit project'));
}
const patched = (calls: Call[]) => calls.filter((c) => c.method === 'PATCH' && c.path.startsWith('/v1/projects/'));

describe('PROJECTS-26', () => {
  it('PROJECTS-26: projects_26_edit_opens_filled_in', async () => {
    const talks = [convo('c1', 'Two fixes for today', { project: OBY.id, folders: OBY.folders }), convo('c2', 'Loose talk')];
    const o = obid([OBY, TETRIX], talks);
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    // the ⋮ of a project's heading (none on No project), and its menu
    expect(headMore('Oby app').getAttribute('aria-haspopup')).toBe('menu');
    expect(button('More for No project', group('No project'))).toBeUndefined();
    await click(headMore('Oby app'));
    expect([...menu()!.children].map((e) => e.textContent || e.tagName)).toEqual(['Edit project…', 'HR', 'Delete project…']);
    await click(button('Edit project…')!);
    expect(menu()).toBeNull();
    // filled in: its name, its folders and its repositories
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Edit project');
    expect(host.querySelector<HTMLInputElement>('#project-name')!.value).toBe('Oby app');
    expect(sheetRows()).toEqual([
      ['oby', 'On this Mac', '/Users/me/oby'], ['oby', 'GitHub', 'obi-ai-bot/oby'], ['oby-plan', 'GitHub', 'obi-ai-bot/oby-plan'],
    ]);
    expect(buttons(dialog()!).filter((b) => !b.classList.contains('project-row-remove')).map((b) => b.textContent))
      .toEqual(['Add a folder…', 'Delete project…', 'Cancel', 'Save']);
    // Save with nothing changed, and Cancel, send nothing
    await click(button('Save', dialog()!)!);
    await until(() => expect(dialog()).toBeNull());
    await editFromPanel('Oby app');
    await fill('#project-name', 'Oby');
    await click(button('Cancel', dialog()!)!);
    expect(dialog()).toBeNull();
    expect(patched(o.calls)).toHaveLength(0);

    // the title bar's menu: Edit project… for the conversation's project, before New project…
    await openRow('Two fixes for today');
    await until(() => expect(chipName(barChip())).toBe('Oby app'));
    await click(barChip()!);
    expect([...menu()!.children].map((e) => e.textContent || e.tagName).slice(-3)).toEqual(['HR', 'Edit project…', 'New project…']);
    await click(button('Edit project…')!);
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Edit project');
    expect(host.querySelector<HTMLInputElement>('#project-name')!.value).toBe('Oby app');
    await click(button('Cancel', dialog()!)!);
    // one in no project: no Edit project…
    await openRow('Loose talk');
    await until(() => expect(chipName(barChip())).toBe('No project'));
    await click(barChip()!);
    expect(button('Edit project…')).toBeUndefined();
  });
});

describe('PROJECTS-27', () => {
  it('PROJECTS-27: projects_27_no_add_from_github', async () => {
    const o = obid([OBY, TETRIX], [convo('c1', 'Two fixes for today', { project: OBY.id, folders: OBY.folders })]);
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    const noGithub = () => {
      expect(buttons(dialog()!).map((b) => b.textContent)).not.toContain('Add from GitHub…');
      expect(dialog()!.textContent).not.toContain('Add from GitHub');
      expect(dialog()!.querySelector('[aria-haspopup="menu"]')).toBeNull();
      expect(dialog()!.querySelector('[role="menuitemcheckbox"]')).toBeNull();
    };
    // New project: only a folder is added
    await click(button('New project')!);
    expect(dialog()!.querySelector('h2')!.textContent).toBe('New project');
    noGithub();
    await click(button('Cancel', dialog()!)!);
    // Edit project, from the title bar's chip: its repositories still show, and nothing adds one
    await openRow('Two fixes for today');
    await until(() => expect(chipName(barChip())).toBe('Oby app'));
    await click(barChip()!);
    await click(button('Edit project…')!);
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Edit project');
    noGithub();
    expect(sheetRows().map((r) => r[2])).toEqual(['/Users/me/oby', 'obi-ai-bot/oby', 'obi-ai-bot/oby-plan']);
    // a change to its name keeps them: only the name is sent
    await fill('#project-name', 'Oby');
    await click(button('Save', dialog()!)!);
    await until(() => expect(dialog()).toBeNull());
    expect(patched(o.calls)).toEqual([{ method: 'PATCH', path: `/v1/projects/${OBY.id}`, body: { name: 'Oby' } }]);
    await click(barChip()!);
    await until(() => expect(shownChoices()[0]).toEqual(['Oby', '2 repos']));
  });
});

describe('PROJECTS-28', () => {
  it('PROJECTS-28: projects_28_remove_a_repo', async () => {
    const o = obid([OBY, TETRIX]);
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    await editFromPanel('Tetrix');
    await click(button('Remove tetrix-front-end', dialog()!)!);
    expect(sheetRows().map((r) => r[2])).toEqual(['/Users/me/tetrix', 'deskree-inc/tetrix-ee', 'deskree-inc/tetrix-collectors']);
    await click(button('Save', dialog()!)!);
    await until(() => expect(dialog()).toBeNull());
    expect(patched(o.calls)).toEqual([{ method: 'PATCH', path: `/v1/projects/${TETRIX.id}`, body: { repos: ['deskree-inc/tetrix-ee', 'deskree-inc/tetrix-collectors'] } }]);
    await click(homeChip()!);
    await until(() => expect(shownChoices()[1]).toEqual(['Tetrix', '2 repos']));
  });
});

describe('PROJECTS-29', () => {
  it('PROJECTS-29: projects_29_add_and_remove_a_folder', async () => {
    const talks = [convo('c1', 'Two fixes for today', { project: OBY.id, folders: OBY.folders })];
    const o = obid([OBY, TETRIX], talks);
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    await editFromPanel('Oby app');
    // a folder from main's own dialog, the old one out, and a new name: each key that changed
    await click(button('Add a folder…', dialog()!)!);
    await until(() => expect(sheetRows()).toHaveLength(4));
    await click(button('Remove oby', dialog()!)!);
    expect(sheetRows().map((r) => r[2])).toEqual(['/Users/me/website', 'obi-ai-bot/oby', 'obi-ai-bot/oby-plan']);
    await fill('#project-name', ' Oby site ');
    await click(button('Save', dialog()!)!);
    await until(() => expect(dialog()).toBeNull());
    expect(patched(o.calls)).toEqual([{ method: 'PATCH', path: `/v1/projects/${OBY.id}`, body: { name: 'Oby site', folders: ['/Users/me/website'] } }]);
    // the heading and the title bar's chip take the new name
    await until(() => expect(listed()[0]).toBe('# Oby site'));
    await openRow('Two fixes for today');
    await until(() => expect(chipName(barChip())).toBe('Oby site'));
    // its conversation takes the new folders as obid's event says, and its box still shows none
    await act(async () => o.bridge.event('conversation', { ...talks[0], folders: ['/Users/me/website'], revision: 3 }));
    expect(layer()!.querySelector('form.chat-composer .composer-tag')).toBeNull();
  });
});

describe('PROJECTS-30', () => {
  it('PROJECTS-30: projects_30_a_refused_save', async () => {
    const o = obid([OBY, TETRIX]);
    o.said.edit = { status: 422, text: '{"error":"/Users/me/website is one of the places Oby never works in."}' };
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    await editFromPanel('Oby app');
    await click(button('Add a folder…', dialog()!)!);
    await until(() => expect(sheetRows()).toHaveLength(4));
    await click(button('Save', dialog()!)!);
    await until(() => expect(dialog()!.querySelector('[role="alert"]')?.textContent).toBe('/Users/me/website is one of the places Oby never works in.'));
    // the sheet stays, as it was; obid's list unchanged
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Edit project');
    expect(host.querySelector<HTMLInputElement>('#project-name')!.value).toBe('Oby app');
    expect(sheetRows().map((r) => r[2])).toEqual(['/Users/me/oby', '/Users/me/website', 'obi-ai-bot/oby', 'obi-ai-bot/oby-plan']);
    expect(button('Save', dialog()!)!.disabled).toBe(false);
    await click(button('Cancel', dialog()!)!);
    await click(homeChip()!);
    await until(() => expect(shownChoices()[0]).toEqual(['Oby app', '2 repos']));
    expect(o.said.projects).toEqual([OBY, TETRIX]);
  });
});

describe('PROJECTS-31', () => {
  it('PROJECTS-31: projects_31_delete_asks_first', async () => {
    const talks = [convo('c1', 'Two fixes for today', { project: OBY.id, folders: OBY.folders }), convo('c2', 'Loose talk')];
    const o = obid([OBY, TETRIX], talks);
    await showHome(o);
    await until(() => expect(listed()).toContain('# Tetrix'));
    const deletes = () => o.calls.filter((c) => c.method === 'DELETE');
    // from the sheet: it asks; Cancel goes back to the sheet
    await editFromPanel('Oby app');
    await click(button('Delete project…', dialog()!)!);
    expect(dialog()!.getAttribute('role')).toBe('alertdialog');
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Delete Oby app?');
    expect(dialog()!.querySelector('.modal-lede')!.textContent).toBe('Its conversations stay, working in its folders as their own.');
    await click(button('Cancel', dialog()!)!);
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Edit project');
    await click(button('Cancel', dialog()!)!);
    expect(deletes()).toHaveLength(0);
    // from the heading's menu: straight to the ask, and Delete project sends DELETE
    await click(headMore('Oby app'));
    await click(button('Delete project…')!);
    expect(dialog()!.querySelector('h2')!.textContent).toBe('Delete Oby app?');
    // refused: obid's words in the ask, which stays
    o.said.del = { status: 500, text: '{"error":"A conversation could not be written."}' };
    await click(button('Delete project', dialog()!)!);
    await until(() => expect(dialog()!.querySelector('[role="alert"]')?.textContent).toBe('A conversation could not be written.'));
    expect(listed()[0]).toBe('# Oby app');
    o.said.del = undefined;
    await click(button('Delete project', dialog()!)!);
    await until(() => expect(dialog()).toBeNull());
    expect(deletes()).toHaveLength(2);
    expect(deletes()[1]).toEqual({ method: 'DELETE', path: `/v1/projects/${OBY.id}`, body: undefined });
    // its conversation goes under No project as obid's event says, with the folders it had
    await act(async () => o.bridge.event('conversation', { ...talks[0], project: null, revision: 3 }));
    await until(() => expect(listed()).toEqual(['# Tetrix', '# No project', 'Loose talk', 'Two fixes for today', '+ New project']));
  });
});

describe('PROJECTS-23', () => {
  it('PROJECTS-23: projects_23_an_obid_without_projects', async () => {
    const o = obid([OBY], [convo('c1', 'Talk one'), convo('c2', 'Talk two')]);
    await showHome(o, ['folders-v1']);
    await until(() => expect(listed()).toEqual(['Talk one', 'Talk two']));
    expect(panel().querySelector('.project-group, .sidebar-group-head')).toBeNull();
    expect(button('New project')).toBeUndefined();
    expect(host.querySelector('.project-chip')).toBeNull();
    await openRow('Talk one');
    expect(host.querySelector('.project-chip')).toBeNull();
    await send(homeBox(), 'Fix the login');
    await until(() => expect(posted(o.calls, '/v1/plans')).toHaveLength(1));
    expect(posted(o.calls, '/v1/plans')[0].body).not.toHaveProperty('project');
    expect(o.calls.some((c) => c.path === '/v1/projects')).toBe(false);
    expect(JSON.stringify(o.calls.map((c) => c.body ?? null))).not.toContain('project');
  });
});

describe('PROJECTS-24', () => {
  it('PROJECTS-24: projects_24_the_mock', async () => {
    await showMock();
    const heads = listed().filter((l) => l.startsWith('#'));
    expect(heads).toEqual(['# Oby app', '# Tetrix', '# No project']);
    for (const name of ['Oby app', 'Tetrix', 'No project']) expect(rowsOf(name).length, name).toBeGreaterThan(0);
    expect(listed().at(-1)).toBe('+ New project');
    await click(homeChip()!);
    expect(shownChoices()).toEqual([['Oby app', '2 repos'], ['Tetrix', '3 repos'], ['No project', '']]);
    await click(homeChip()!);
    // a conversation in Oby app: the chip beside its title (screen 03); New project: the sheet (screen 02)
    await openRow(rowsOf('Oby app')[0]!);
    expect(chipName(barChip())).toBe('Oby app');
    await click(button('New project')!);
    expect(dialog()!.querySelector('h2')!.textContent).toBe('New project');
  });
});
