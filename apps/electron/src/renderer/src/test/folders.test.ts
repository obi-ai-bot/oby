import { beforeEach, describe, expect, it, vi } from 'vitest';

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
    f.sendFoldersBy((c, folders) => { sent.push([c, folders]); return !c.startsWith('new:'); });
    f.setFolders('new:1', ['/Users/me/notes']);
    // obid's list: c1 has none there yet, so the app's are sent once, and its own copy goes
    f.takeFolders({ c1: [], c3: ['/Users/me/docs'] }, s);
    expect(sent).toEqual([['new:1', ['/Users/me/notes']], ['c1', ['/Users/me/api']]]);
    expect(s.getItem('oby.folders')).toBeNull();
    expect(f.foldersOf('c3')).toEqual(['/Users/me/docs']);
    expect(f.foldersOf('new:1')).toEqual(['/Users/me/notes']);
    // a change goes to obid, and shows once obid's answer comes back
    f.setFolders('c3', []);
    expect(sent.at(-1)).toEqual(['c3', []]);
    expect(f.foldersOf('c3')).toEqual(['/Users/me/docs']);
    f.takeFolders({ c1: ['/Users/me/api'], c3: [] }, s);
    expect(f.foldersOf('c3')).toEqual([]);
    expect(f.foldersOf('c1')).toEqual(['/Users/me/api']);
    expect(s.getItem('oby.folders')).toBeNull();
  });
});
