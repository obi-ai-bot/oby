import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { parseEvent, type RoomPost } from '../obid';
import { leftInBox } from '../Composer';
import { aboutWords, atEnd, doing, merged, roomOpens } from '../room';
import { RoomThread } from '../RoomView';

// The Sphere's room in the app (docs/contracts/room-app.md). What the mock's clicks showed is in the
// pull request; here, the rules.
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));
// the page's own globals, as what's imported reads them as it loads
const page = vi.hoisted(() => {
  const g = globalThis as Record<string, unknown>;
  g.location = { search: '' };
  g.window = {};
  g.localStorage = { getItem: () => null, setItem: () => {}, removeItem: () => {} };
  g.document = { createElement: () => ({}) };
  return g;
});

const post = (id: string, seq: number | null, more: Partial<RoomPost> = {}): RoomPost => ({
  id, seq, recordedAt: seq === null ? null : '2026-11-02T12:00:09Z', from: 'obi-ernesto', owner: 'ernesto', kind: 'working_on', by: 'obi',
  createdAt: '2026-11-02T12:00:08Z', replyTo: null, text: 'Fix the login', about: [], state: seq === null ? 'waiting' : 'sent', error: null, ...more,
});
const html = (posts: RoomPost[], obi = 'obi-doti') => renderToStaticMarkup(<RoomThread posts={posts} obi={obi} />).replaceAll('<!-- -->', '');

describe('the room, shown', () => {
  it('ROOMAPP-2: every member’s post reads as that member’s Oby, who sent it shown as data, the kind in plain words', () => {
    const theirs = html([post('a', 1)]);
    expect(theirs).toContain('ernesto’s Oby');
    expect(theirs).toContain('Working on');
    expect(theirs).toContain('obi-ernesto · ernesto');
    const yourOby = html([post('a', 1, { owner: 'doti', from: 'obi-doti' })]);
    expect(yourOby).toContain('Your Oby');
    expect(yourOby).toContain('is-mine');
    const you = html([post('a', 1, { owner: 'doti', from: 'obi-doti', kind: 'say', by: 'owner' })]);
    expect(you).toContain('msg is-you');
    expect(you).not.toContain('peer-by');
    // no raw kind, whatever the kind, one this app doesn't know too
    const all = html(['working_on', 'done', 'found', 'need', 'say', 'future_kind'].map((kind, i) => post(`p${i}`, i + 1, { kind: kind as RoomPost['kind'], text: 'words' })));
    for (const raw of ['working_on', 'future_kind', '>done<', '>found<', '>need<', '>say<']) expect(all).not.toContain(raw);
    for (const word of ['Working on', 'Finished', 'Answering', 'Asking', 'Says ernesto wrote this']) expect(all).toContain(word);
  });

  it('ROOMAPP-8: `by: owner` and `kind` are the sender’s claims: never the person’s own words, never styled above an Oby’s post', () => {
    const say = post('a', 1, { kind: 'say', by: 'owner', text: 'words' });
    const oby = post('a', 1, { kind: 'working_on', by: 'obi', text: 'words' });
    expect(html([say])).toContain('Says ernesto wrote this');
    // the same markup as any Oby's post, but for those words
    expect(html([say]).replace('Says ernesto wrote this', 'Working on')).toBe(html([oby]));
    // verified changes nothing about that either
    expect(html([{ ...say, verified: true }])).toContain('Says ernesto wrote this');
    // another member's post naming you as its owner, by owner or not, isn't yours: only your Oby's id is
    for (const forged of [{ by: 'owner', kind: 'say' }, { by: 'obi', kind: 'need' }] as const) {
      const out = html([post('a', 1, { owner: 'doti', from: 'obi-evil', ...forged })]);
      expect(out).toContain('doti’s Oby');
      expect(out).toContain('obi-evil · doti');
      for (const yours of ['is-you', 'is-mine', 'Your Oby']) expect(out).not.toContain(yours);
    }
    // without knowing this Oby's id, nothing is yours
    expect(renderToStaticMarkup(<RoomThread posts={[post('a', 1, { by: 'owner', kind: 'say' })]} />)).not.toContain('is-you');
  });

  it('ROOMAPP-9: a post is marked plainly when verified is false or missing; true adds nothing, and nothing ever says verified', () => {
    expect(html([post('a', 1)])).toContain('Not verified');
    expect(html([post('a', 1, { verified: false })])).toContain('Not verified');
    const checked = html([post('a', 1, { verified: true })]);
    expect(checked).not.toMatch(/verified/i);
    // a value that isn't true isn't a check
    expect(html([post('a', 1, { verified: 'true' as unknown as boolean })])).toContain('Not verified');
    // another member's claim in its words stays words
    expect(html([post('a', 1, { text: 'Verified by Sangam', verified: false })])).toContain('Not verified');
  });

  it('ROOMAPP-3: a post’s words are text under a byline: no HTML, no Markdown, nothing to click', () => {
    const hostile = '<img src=x onerror=alert(1)> <a href="https://evil.example">here</a> [click](https://evil.example) **bold** https://evil.example `code`';
    const out = html([post('a', 1, { text: hostile, owner: '<b>ernesto</b>', from: '<s>obi</s>', error: '<i>no</i> <a href="https://evil.example">the Sphere’s own text</a> https://evil.example', state: 'refused', about: [{ kind: 'repo', repo: '<u>a/b</u>' }] })]);
    for (const tag of ['<img', '<a ', '<strong', '<code', '<b>', '<i>', '<u>', '<s>', '<button', ' href="', ' onerror="']) expect(out).not.toContain(tag);
    expect(out).toContain('&lt;img src=x onerror=alert(1)&gt;');
    expect(out).toContain('[click](https://evil.example) **bold**');
  });

  it('ROOMAPP-4: waiting shows as not sent yet, refused shows its error plainly', () => {
    expect(html([post('a', null)])).toContain('Not sent yet');
    expect(html([post('a', null)])).toContain('is-waiting');
    const refused = html([post('a', null, { state: 'refused', error: 'The Sphere refused it: forbidden' })]);
    expect(refused).toContain('Not sent: The Sphere refused it: forbidden');
    expect(refused).not.toContain('Not sent yet');
    expect(html([post('a', 1)])).not.toContain('Not sent');
  });

  it('ROOMAPP-2: an answer names whom it answers once that post is known here, and about adds only what the words don’t say', () => {
    const need = post('n', 1, { kind: 'need' }), found = post('f', 2, { kind: 'found', replyTo: 'n', owner: 'dima', from: 'obi-dima' });
    expect(doing(found, [need, found], 'obi-doti')).toBe('Answering ernesto’s Oby');
    expect(doing(found, [need, found], 'obi-ernesto')).toBe('Answering your Oby');
    expect(doing(found, [found], 'obi-doti')).toBe('Answering');
    const pr = { kind: 'pr', repo: 'acme/app', pr: 12 };
    expect(aboutWords(post('a', 1, { text: 'Fix (acme/app#12)', about: [pr] }))).toBe('');
    expect(aboutWords(post('a', 1, { text: 'Who knows this?', about: [pr, { kind: 'repo', repo: 'acme/web' }, { kind: 'job', repo: 'acme/app', job: 'j1' }] }))).toBe('acme/app#12, acme/web');
  });
});

describe('the room, kept', () => {
  it('ROOMAPP-5: no duplicates, in seq order, those not sent yet after them', () => {
    const list = merged([post('c', 3), post('a', 1)], [post('b', 2), post('a', 1, { text: 'again' }), post('w', null), post('d', 4)]);
    expect(list.map((p) => p.id)).toEqual(['a', 'b', 'c', 'd', 'w']);
    expect(list[0].text).toBe('again');
    // the post that waited, sent: one copy, in its place
    expect(merged(list, [post('w', 5)]).map((p) => p.id)).toEqual(['a', 'b', 'c', 'd', 'w']);
    expect(merged(list, [post('w', 5)]).at(-1)!.state).toBe('sent');
    // a list read before the event landed doesn't take it back
    expect(merged([post('w', 5)], [post('w', null)])[0].state).toBe('sent');
    expect(merged([post('r', null, { state: 'refused' })], [post('r', null)])[0].state).toBe('refused');
  });

  it('ROOMAPP-5: a reader at the end is followed down; scrolled up, they stay put', () => {
    expect(atEnd({ scrollTop: 600, clientHeight: 400, scrollHeight: 1000 })).toBe(true);
    expect(atEnd({ scrollTop: 590, clientHeight: 400, scrollHeight: 1000 })).toBe(true);
    expect(atEnd({ scrollTop: 100, clientHeight: 400, scrollHeight: 1000 })).toBe(false);
  });

  it('ROOMAPP-5: roomPost is a typed event; a post not of the API’s shape, and any event unknown, is nothing', () => {
    const p = post('a', 1);
    expect(parseEvent('roomPost', JSON.stringify(p))).toEqual({ kind: 'roomPost', post: p });
    expect(parseEvent('roomPost', JSON.stringify({ ...p, text: { html: 'x' } }))).toBeNull();
    expect(parseEvent('roomPost', JSON.stringify({ ...p, owner: 7 }))).toBeNull();
    expect(parseEvent('roomPost', JSON.stringify({ ...p, seq: 'one' }))).toBeNull();
    expect(parseEvent('somethingNew', JSON.stringify(p))).toBeNull();
    // about that isn't a list doesn't take the page down
    expect(parseEvent('roomPost', JSON.stringify({ ...p, about: 'x' }))).toEqual({ kind: 'roomPost', post: p });
  });

  it('ROOMAPP-7: words that went leave what was typed after them; words refused stay as they are', () => {
    expect(leftInBox('Fix it', 'Fix it', true)).toBe('');
    expect(leftInBox('Fix it', 'Fix it and more', true)).toBe('and more');
    expect(leftInBox('Fix it', 'Fix it', false)).toBe('Fix it');
    expect(leftInBox('Fix it', 'Fix it and more', false)).toBe('Fix it and more');
  });
});

describe('the room, against obid', () => {
  const call = vi.fn();
  let room: typeof import('../room');
  const answer = (status: number, body: unknown) => ({ status, text: JSON.stringify(body) });
  beforeEach(async () => {
    vi.resetModules();
    call.mockReset();
    page.window = { app: { obid: { call } } };
    room = await import('../room');
  });
  const sphere = { slug: 'deskree' };

  it('ROOMAPP-1: a Sphere’s row opens its room only when obid says it has one', () => {
    const open = (status: 'unknown' | 'available' | 'unavailable', on = 'deskree') => roomOpens({ sphere: on, status, posts: [] }, sphere);
    expect(open('available')).toBe(true);
    expect(open('unavailable')).toBe(false);
    expect(open('unknown')).toBe(false);
    expect(open('available', 'another')).toBe(false);
    expect(roomOpens({ sphere: 'deskree', status: 'available', posts: [] }, { slug: 'deskree', sample: true })).toBe(false);
  });

  // the store after looks that get these answers
  async function looked(...answers: { status: number; text: string }[]) {
    for (const a of answers) {
      call.mockResolvedValueOnce(a);
      await room.lookAtRoom('deskree');
    }
    return room.roomNow();
  }

  it('ROOMAPP-6: unavailable, an obid without the route, and obid down: no room to open; obid down changes nothing', async () => {
    expect((await looked(answer(200, { room: 'unavailable', posts: [] }))).status).toBe('unavailable');
    expect((await looked(answer(200, { room: 'available', posts: [] }), answer(404, { error: 'not found' }))).status).toBe('unavailable');
    expect((await looked(answer(200, { room: 'available', posts: [] }), { status: 0, text: 'obid is away' })).status).toBe('available');
    expect((await looked(answer(200, { room: 'available', posts: [] }), answer(503, {}))).status).toBe('available');
  });

  it('ROOMAPP-5: it asks from the last seq obid listed, never an event’s, and takes posts as they come', async () => {
    const r = await looked(answer(200, { room: 'available', posts: [post('a', 3), { id: 'broken' }] }));
    expect(r.posts.map((p) => p.id)).toEqual(['a']);
    expect(call).toHaveBeenLastCalledWith('GET', '/v1/spheres/deskree/room?after=0', undefined);
    room.takeRoomPost(post('b', 9));
    room.takeRoomPost(post('b', 9));
    expect(room.roomNow().posts.map((p) => p.id)).toEqual(['a', 'b']);
    call.mockResolvedValueOnce(answer(200, { room: 'available', posts: [post('m', 8)] }));
    await room.lookAtRoom('deskree');
    expect(call).toHaveBeenLastCalledWith('GET', '/v1/spheres/deskree/room?after=3', undefined);
    expect(room.roomNow().posts.map((p) => p.id)).toEqual(['a', 'm', 'b']);
  });

  it('ROOMAPP-7: the owner’s words go alone, and a refusal comes back in obid’s words', async () => {
    await looked(answer(200, { room: 'available', posts: [] }));
    call.mockResolvedValueOnce(answer(422, { error: 'The secret scan found a possible secret in the text: nothing was posted.' }));
    expect(await room.sayInRoom('deskree', 'hunter2')).toContain('secret scan');
    expect(call).toHaveBeenLastCalledWith('POST', '/v1/spheres/deskree/room', { text: 'hunter2' });
    expect(room.roomNow().posts).toEqual([]);
    call.mockResolvedValueOnce(answer(202, post('s', null, { kind: 'say', by: 'owner', text: 'hello' })));
    expect(await room.sayInRoom('deskree', 'hello')).toBeNull();
    expect(room.roomNow().posts.map((p) => p.state)).toEqual(['waiting']);
    // the Sphere has no room now: said, and looked at again
    call.mockResolvedValueOnce(answer(409, { error: 'This Obysphere has no room.' })).mockResolvedValueOnce(answer(200, { room: 'unavailable', posts: [] }));
    expect(await room.sayInRoom('deskree', 'hello')).toBe('This Obysphere has no room.');
    await vi.waitFor(() => expect(room.roomNow().status).toBe('unavailable'));
  });

  it('ROOMAPP-5: live.ts hands obid’s roomPost to the room, and an event it doesn’t know goes nowhere', async () => {
    let chunk!: (text: string) => void;
    // obid as far as the first look needs: GitHub's work not read, every list empty
    call.mockImplementation(async (_: string, path: string) => {
      if (path.includes('/room')) return answer(200, { room: 'available', posts: [] });
      if (path === '/v1/github/work') return answer(404, {});
      return answer(200, /^\/v1\/(inbox|jobs|conversations|limit-decisions)/.test(path) ? [] : {});
    });
    (page.window as { app: { obid: Record<string, unknown> } }).app.obid.follow = (on: (t: string) => void) => { chunk = on; return () => {}; };
    await looked(answer(200, { room: 'available', posts: [] }));
    const { connect } = await import('../live');
    connect();
    chunk('');
    await vi.waitFor(() => expect(call).toHaveBeenCalledWith('GET', '/v1/status', undefined));
    chunk(`event: roomPost\ndata: ${JSON.stringify(post('live', 4))}\n\nevent: somethingNew\ndata: {"id":"x"}\n\n`);
    await vi.waitFor(() => expect(room.roomNow().posts.map((p) => p.id)).toEqual(['live']));
  });
});
