import { afterEach, describe, expect, it, vi } from 'vitest';

// ATTACH-10 (docs/contracts/attachments.md): the message box's files reach obid. Each file is
// uploaded first (POST /v1/attachments), then the words go with the paths obid kept, the tasks
// attached and an id of their own; a send obid didn't answer goes again with the same id, and the
// words show their files' tags.

type Call = { method: string; path: string; body?: unknown };
const calls: Call[] = [];
const flush = () => new Promise((r) => setTimeout(r, 0));
function memory(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; }, clear: () => map.clear(), getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null, removeItem: (k) => { map.delete(k); }, setItem: (k, v) => { map.set(k, v); },
  };
}

async function load(answer: (c: Call, n: number) => { status: number; body?: unknown }) {
  vi.resetModules();
  calls.length = 0;
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', memory());
  const call = async (method: string, path: string, body?: unknown) => {
    const c = { method, path, body };
    calls.push(c);
    const a = answer(c, calls.filter((x) => x.method === method && x.path === path).length);
    return { status: a.status, text: a.body === undefined ? '' : JSON.stringify(a.body) };
  };
  vi.stubGlobal('window', { app: { obid: { call, confirm: async () => ({ status: 200, text: '{}' }) } } });
  const live = await import('../live');
  const chats = await import('../chats');
  return { live, chats };
}
const plans = () => calls.filter((c) => c.path === '/v1/plans');

describe('files and photos reach obid', { timeout: 15_000 }, () => {
  afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

  it('attach_10_files_are_uploaded_then_sent_with_the_words_tasks_and_an_id', async () => {
    let id = '';
    const { live, chats } = await load((c) => {
      if (c.path === '/v1/attachments?name=shot+1.png') return { status: 201, body: { path: '/home/attachments/app/u1/shot_1.png', name: 'shot_1.png' } };
      if (c.path === '/v1/plans') { id = (c.body as { id: string }).id; return { status: 200, body: { kind: 'reply', text: 'Looking.', jobs: [], conversation: 'c1' } }; }
      if (c.path.startsWith('/v1/conversation?')) return { status: 200, body: [{ id, who: 'user', text: 'Look', at: new Date().toISOString(), jobs: ['j1'], conversation: 'c1', attachments: ['shot_1.png'] }] };
      return { status: 404, body: { error: 'Not found.' } };
    });
    const file = new File(['pixels'], 'shot 1.png', { type: 'image/png' });
    live.ask('Look', undefined, undefined, undefined, { files: [{ name: 'shot 1.png', preview: 'blob:shot', file }], jobs: ['j1'] });
    await vi.waitFor(() => expect(calls.some((c) => c.path.startsWith('/v1/conversation?'))).toBe(true), { timeout: 5000 });
    await flush();
    const upload = calls[0];
    expect(upload.method).toBe('POST');
    expect(upload.body).toBeInstanceOf(Uint8Array);
    expect(new TextDecoder().decode(upload.body as Uint8Array)).toBe('pixels');
    expect(plans()).toHaveLength(1);
    expect(plans()[0].body).toEqual({ said: 'Look', new: true, id, attachments: ['/home/attachments/app/u1/shot_1.png'], jobs: ['j1'] });
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    // the tag shows the name you picked, and the photo's preview
    const said = chats.dev!.get().find((c) => c.said.some((m) => m.text === 'Look'))!.said[0];
    expect(said.files).toEqual([{ name: 'shot 1.png', preview: 'blob:shot' }]);
  });

  it('attach_10_files_alone_are_sent', async () => {
    const { live } = await load((c) => (c.path.startsWith('/v1/attachments')
      ? { status: 201, body: { path: '/p/a.txt', name: 'a.txt' } }
      : { status: 200, body: c.path === '/v1/plans' ? { kind: 'none', jobs: [] } : [] }));
    live.ask('', 'c9', undefined, undefined, { files: [{ name: 'a.txt', file: new File(['a'], 'a.txt') }] });
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    expect(plans()[0].body).toMatchObject({ said: '', conversation: 'c9', attachments: ['/p/a.txt'] });
  });

  it('attach_10_a_send_obid_didnt_answer_goes_again_with_the_same_id', async () => {
    const { live } = await load((c, n) => (c.path === '/v1/plans' && n === 1
      ? { status: 0, body: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' }
      : { status: 200, body: c.path === '/v1/plans' ? { kind: 'none', jobs: [], conversation: 'c2' } : [] }));
    live.ask('Fix it', 'c2');
    await vi.waitFor(() => expect(plans()).toHaveLength(2), { timeout: 3000 });
    expect(plans()[1].body).toEqual(plans()[0].body);
    expect((plans()[0].body as { id?: string }).id).toBeTruthy();
  });

  it('attach_10_an_answer_of_obids_is_final', async () => {
    const { live } = await load(() => ({ status: 0 }));
    const { ApiError } = await import('../obid');
    let tries = 0;
    await expect(live.sent(async () => { tries++; throw new ApiError(422, 'No.'); }, 3, 0)).rejects.toThrow('No.');
    expect(tries).toBe(1);
  });
});
