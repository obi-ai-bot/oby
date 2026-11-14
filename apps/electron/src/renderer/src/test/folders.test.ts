import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { has } from '../../../shared/capabilities';

// FOLDERS-11 (docs/contracts/folders.md): with an obid whose status has no folders, the app keeps
// them itself, as #1564 does; once it says folders: true, the app sends them and shows obid's.
function storage(entries: Record<string, string> = {}): Storage {
  const map = new Map(Object.entries(entries));
  return {
    get length() { return map.size; },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null,
    removeItem: (k) => { map.delete(k); },
    setItem: (k, v) => { map.set(k, v); },
  };
}

describe('working folders', () => {
  let s: Storage;
  beforeEach(() => {
    vi.resetModules();
    s = storage({ 'oby.folders': JSON.stringify({ c1: ['/Users/me/api'] }) });
    vi.stubGlobal('localStorage', s);
  });

  it('folders_11_an_older_obid_leaves_them_to_the_app', async () => {
    const f = await import('../folders');
    f.setFolders('c2', ['/Users/me/web']);
    expect(f.foldersOf('c1')).toEqual(['/Users/me/api']);
    expect(JSON.parse(s.getItem('oby.folders')!)).toEqual({ c1: ['/Users/me/api'], c2: ['/Users/me/web'] });
  });

  it('folders_11_an_obid_that_takes_them_gets_them_and_shows_its_own', async () => {
    const f = await import('../folders');
    const sent: [string, string[]][] = [];
    f.sendFoldersBy((c, folders) => {
      if (c.startsWith('new:')) return undefined;
      sent.push([c, folders]);
      return Promise.resolve();
    }, () => {});
    f.setFolders('new:1', ['/Users/me/notes']);
    // obid's list: c1 has none there yet, so the app's are sent once
    const moving = f.takeFolders({ c1: [], c3: ['/Users/me/docs'] }, s);
    expect(sent).toEqual([['c1', ['/Users/me/api']]]);
    // the app's copy stays until obid has them all
    expect(s.getItem('oby.folders')).not.toBeNull();
    await moving;
    expect(s.getItem('oby.folders')).toBeNull();
    expect(f.foldersOf('c3')).toEqual(['/Users/me/docs']);
    expect(f.foldersOf('new:1')).toEqual(['/Users/me/notes']);
    // a change goes to obid, and shows once obid's answer comes back
    f.setFolders('c3', []);
    expect(sent.at(-1)).toEqual(['c3', []]);
    expect(f.foldersOf('c3')).toEqual(['/Users/me/docs']);
    await f.takeFolders({ c1: ['/Users/me/api'], c3: [] }, s);
    expect(f.foldersOf('c3')).toEqual([]);
    expect(f.foldersOf('c1')).toEqual(['/Users/me/api']);
    expect(s.getItem('oby.folders')).toBeNull();
  });

  it('folders_11_a_folder_obid_refuses_stays_with_the_app', async () => {
    s.setItem('oby.folders', JSON.stringify({ c1: ['/Users/me/api', '/private/tmp/scratch', '/Users/me/web'] }));
    const f = await import('../folders');
    const taken: Record<string, string[]> = {};
    const reported: unknown[] = [];
    // obid refuses the whole change when one folder is refused
    f.sendFoldersBy((c, folders) => {
      if (folders.some((p) => p.startsWith('/private/'))) return Promise.reject(new Error('"scratch" is where your Mac keeps its own files'));
      taken[c] = folders;
      return Promise.resolve();
    }, (e) => reported.push(e));
    await f.takeFolders({ c1: [] }, s);
    expect(taken).toEqual({ c1: ['/Users/me/api', '/Users/me/web'] });
    expect(JSON.parse(s.getItem('oby.folders')!)).toEqual({ c1: ['/private/tmp/scratch'] });
    // moving them says nothing; a change the owner makes that obid refuses is said
    expect(reported).toEqual([]);
    f.setFolders('c1', ['/private/tmp/scratch']);
    await new Promise((r) => setTimeout(r, 0));
    expect(reported).toHaveLength(1);
  });
});

// CAPS-4 and CAPS-5 (docs/contracts/capabilities.md): the app reads the gate as `folders-v1` in
// obid's `capabilities`, and as `folders: true` from an obid that sends no list.
const calls: { method: string; path: string; body?: unknown }[] = [];
const flush = () => new Promise((r) => setTimeout(r, 0));
const conversation = (id: string, folders: string[] = []) => ({
  id, title: 'Notes', pinned: false, created: '2026-11-14T10:00:00Z', lastActivity: '2026-11-14T10:00:00Z', cursor: '', folders,
});
// The app on obid, following its events, with obid's status as given and c1 listed
async function onObid(status: Record<string, unknown>, s: Storage) {
  vi.resetModules();
  calls.length = 0;
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', s);
  const answers: Record<string, { status: number; body?: unknown }> = {
    'GET /v1/status': { status: 200, body: { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0, ...status } },
    'GET /v1/jobs?limit=100': { status: 200, body: [] },
    'GET /v1/inbox': { status: 200, body: [] },
    'GET /v1/conversations?limit=200': { status: 200, body: [conversation('c1')] },
    'POST /v1/plans': { status: 200, body: { kind: 'reply', text: 'Looking.', jobs: [], conversation: 'c2' } },
  };
  const call = async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    const a = method === 'PATCH' ? { status: 200, body: conversation(path.split('/').pop()!, (body as { folders: string[] }).folders) }
      : path.startsWith('/v1/conversation?') ? { status: 200, body: [] }
      : answers[`${method} ${path}`] ?? { status: 404, body: { error: 'Not found.' } };
    return { status: a.status, text: JSON.stringify(a.body) };
  };
  let feed: (text: string) => void = () => {};
  const follow = (onChunk: (text: string) => void) => { feed = onChunk; return () => {}; };
  vi.stubGlobal('window', { app: { obid: { call, confirm: async () => ({ status: 200, text: '{}' }), follow } } });
  const live = await import('../live');
  const f = await import('../folders');
  live.connect();
  feed('');
  for (let i = 0; i < 5; i++) await flush();
  expect(live.liveState().connection).toBe('live');
  calls.length = 0;
  return { live, f };
}
const plans = () => calls.filter((c) => c.method === 'POST' && c.path === '/v1/plans');
const patches = () => calls.filter((c) => c.method === 'PATCH');

describe('working folders, gated on capabilities', { timeout: 15_000 }, () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it('caps_4_an_obid_without_folders_v1_gets_no_folders', async () => {
    const s = storage();
    const { live, f } = await onObid({ capabilities: [], folders: true }, s);
    // a new conversation's first words go without its folders
    live.ask('Look at the api', undefined, undefined, ['/Users/me/api']);
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    expect(plans()[0].body).not.toHaveProperty('folders');
    // a listed conversation's folders stay with the app
    f.setFolders('c1', ['/Users/me/web']);
    await flush();
    expect(patches()).toEqual([]);
    expect(f.foldersOf('c1')).toEqual(['/Users/me/web']);
    expect(JSON.parse(s.getItem('oby.folders')!)).toEqual({ c1: ['/Users/me/web'] });
  });

  it('caps_5_an_obid_before_capabilities_falls_back_to_the_boolean', async () => {
    const { live, f } = await onObid({ folders: true }, storage());
    live.ask('Look at the api', undefined, undefined, ['/Users/me/api']);
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    expect(plans()[0].body).toMatchObject({ folders: ['/Users/me/api'] });
    f.setFolders('c1', ['/Users/me/web']);
    await vi.waitFor(() => expect(patches()).toEqual([{ method: 'PATCH', path: '/v1/conversations/c1', body: { folders: ['/Users/me/web'] } }]));
    await vi.waitFor(() => expect(f.foldersOf('c1')).toEqual(['/Users/me/web']));
  });

  it('caps_4_caps_5_has_reads_the_list_then_the_older_key', () => {
    expect(has({ capabilities: ['folders-v1'] }, 'folders-v1')).toBe(true);
    expect(has({ capabilities: [], folders: true }, 'folders-v1')).toBe(false);
    expect(has({ folders: true }, 'folders-v1')).toBe(true);
    expect(has({}, 'folders-v1')).toBe(false);
    expect(has({ folders: true }, 'receipts-v1')).toBe(false);
    expect(has(null, 'folders-v1')).toBe(false);
  });
});
