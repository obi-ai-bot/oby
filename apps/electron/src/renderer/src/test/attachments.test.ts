import { afterEach, describe, expect, it, vi } from 'vitest';

// ATTACH-10, ATTACH-15, ATTACH-16 (docs/contracts/attachments.md): the message box's files reach obid. Each file is
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
    const files = await live.uploadFiles([{ name: 'shot 1.png', preview: 'blob:shot', file }]);
    const upload = calls[0];
    expect(upload.method).toBe('POST');
    expect(new TextDecoder().decode(upload.body as Uint8Array)).toBe('pixels');
    expect(files).toEqual([{ name: 'shot 1.png', preview: 'blob:shot', file, path: '/home/attachments/app/u1/shot_1.png' }]);
    live.ask('Look', undefined, undefined, undefined, { files: files!, jobs: ['j1'] });
    await vi.waitFor(() => expect(calls.some((c) => c.path.startsWith('/v1/conversation?'))).toBe(true), { timeout: 5000 });
    await flush();
    expect(plans()).toHaveLength(1);
    expect(plans()[0].body).toEqual({ said: 'Look', new: true, id, attachments: ['/home/attachments/app/u1/shot_1.png'], jobs: ['j1'] });
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    // the tag shows the name you picked, and the photo's preview
    const said = chats.dev!.get().find((c) => c.said.some((m) => m.text === 'Look'))!.said[0];
    expect(said.files).toEqual([{ name: 'shot 1.png', preview: 'blob:shot' }]);
  });

  it('attach_15_one_refused_upload_keeps_the_box_and_takes_the_others_away', async () => {
    const { live } = await load((c) => (c.path.startsWith('/v1/attachments?name=a.txt')
      ? { status: 201, body: { path: '/p/u1/a.txt', name: 'a.txt' } }
      : c.method === 'DELETE' ? { status: 204 } : { status: 422, body: { error: 'b.txt is empty.' } }));
    const done = await live.uploadFiles([{ name: 'a.txt', file: new File(['a'], 'a.txt') }, { name: 'b.txt', file: new File([], 'b.txt') }]);
    expect(done).toBeNull();
    await flush();
    expect(calls.at(-1)).toMatchObject({ method: 'DELETE', path: '/v1/attachments?path=%2Fp%2Fu1%2Fa.txt' });
    expect(live.liveState().problem).toBe('b.txt is empty.');
    // one over 25 MB isn't read or sent at all, and says so in obid's words
    calls.length = 0;
    const big = { name: 'big.mov', file: { size: 25_000_001, name: 'big.mov' } as File };
    expect(await live.uploadFiles([big])).toBeNull();
    expect(calls).toEqual([]);
    expect(live.liveState().problem).toBe("big.mov is larger than 25 MB, so Oby can't take it.");
  });

  it('attach_10_files_alone_are_sent', async () => {
    const { live } = await load((c) => ({ status: 200, body: c.path === '/v1/plans' ? { kind: 'none', jobs: [] } : [] }));
    live.ask('', 'c9', undefined, undefined, { files: [{ name: 'a.txt', path: '/p/a.txt' }] });
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    expect(plans()[0].body).toMatchObject({ said: '', conversation: 'c9', attachments: ['/p/a.txt'] });
  });

  it('attach_15_words_obid_refuses_take_their_files_away', async () => {
    const { live } = await load((c) => (c.method === 'DELETE' ? { status: 204 } : { status: 422, body: { error: 'Files go with a follow-up of a finished task or a new task: tell this one in words.' } }));
    await live.tellJob('j1', 'and this', [{ path: '/p/u2/shot.png' }]);
    expect(calls[0]).toMatchObject({ method: 'POST', path: '/v1/jobs/j1/messages', body: { text: 'and this', attachments: ['/p/u2/shot.png'] } });
    await flush();
    expect(calls[1]).toMatchObject({ method: 'DELETE', path: '/v1/attachments?path=%2Fp%2Fu2%2Fshot.png' });
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

  it('attach_16_a_second_pasted_screenshot_is_numbered_not_dropped', async () => {
    const { numbered } = await import('../Composer');
    expect(numbered('image.png', new Set())).toBe('image.png');
    expect(numbered('image.png', new Set(['image.png', 'image 2.png']))).toBe('image 3.png');
    expect(numbered('Pasted image', new Set(['Pasted image']))).toBe('Pasted image 2');
  });
});
