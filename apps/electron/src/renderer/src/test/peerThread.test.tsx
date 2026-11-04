import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { peerPostOf, peerThreadOf, parseEvent, type PeerExchange, type PeerPost, type PeerThread } from '../obid';
import { draft, keep, leftInBox } from '../drafts';
import { PEER_CONVOS } from '../mock';
import { askFromBox, boxShut, lineUnder, openThread, lookAtThread, memberOpens, mockConvosWith, nextQuiet, plainId, standing, threadChanged, threadNow, whoseOby, withWords } from '../peers';
import { RoomThread } from '../RoomView';

// A member's thread in the app (docs/contracts/peer-thread.md, PEER-7 to PEER-12). What clicking it
// on the rig showed is in the pull request; here, the rules.
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));
const page = vi.hoisted(() => {
  const g = globalThis as Record<string, unknown>;
  g.location = { search: '' };
  g.window = {};
  const kept = new Map<string, string>();
  g.localStorage = { getItem: (k: string) => kept.get(k) ?? null, setItem: (k: string, v: string) => { kept.set(k, v); }, removeItem: (k: string) => { kept.delete(k); } };
  g.document = { createElement: () => ({}) };
  return g;
});

const open: PeerExchange = { id: 'x1', type: 'question', direction: 'outgoing', state: 'sent', waitingOnThem: false, quietAt: '2026-11-03T12:02:00Z' };
const post = (id: string, more: Partial<PeerPost> = {}): PeerPost => ({
  id, seq: 1, recordedAt: '2026-11-03T12:00:09Z', from: 'obi-dana', owner: 'dana', kind: 'answer', by: 'obi', createdAt: '2026-11-03T12:00:08Z', replyTo: null,
  text: 'Fixing the login', about: [], state: 'sent', error: null, direction: 'received', status: null, reason: null, exchange: { ...open, state: 'done', quietAt: null }, ...more,
});
const asked = (more: Partial<PeerPost> = {}) => post('q', { from: 'obi-me', owner: 'me', kind: 'question', by: 'owner', text: 'What are you working on?', direction: 'sent', exchange: open, ...more });
const thread = (more: Partial<PeerThread> = {}): PeerThread => ({ owner: 'dana', name: 'Dana', member: true, takesQuestions: true, posts: [], ...more });
const html = (posts: PeerPost[]) => renderToStaticMarkup(<RoomThread posts={withWords(posts)} obi="obi-me" />).replaceAll('<!-- -->', '');

// obid, as main passes it on
type Answer = { status: number; text: string };
let calls: { method: string; path: string; body?: unknown }[];
let answers: (() => Promise<Answer> | Answer)[];
beforeEach(() => {
  calls = [];
  answers = [];
  (page.window as { app?: unknown }).app = { obid: { call: async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    return answers.shift()?.() ?? { status: 200, text: JSON.stringify(thread()) };
  } } };
});

describe('the row', () => {
  it('peer_7_a_live_row_opens_the_thread', async () => {
    expect(memberOpens({ live: true })).toBe('thread');
    // by its exact owner id, and what obid says is what's kept
    answers.push(() => ({ status: 200, text: JSON.stringify(thread({ owner: 'a/b', posts: [asked()] })) }));
    await lookAtThread('a/b');
    expect(calls).toEqual([{ method: 'GET', path: '/v1/obid/sphere/peers/a%2Fb', body: undefined }]);
    expect(threadNow('a/b').thread).toMatchObject({ name: 'Dana', posts: [{ id: 'q' }] });
    // obid's event reads an opened thread again, and no other; an older answer doesn't take it back
    expect(parseEvent('peerThread', JSON.stringify({ owner: 'a/b', posts: [] }))).toEqual({ kind: 'peerThread', owner: 'a/b' });
    calls = [];
    threadChanged('never-opened');
    expect(calls).toEqual([]);
    let older!: (a: Answer) => void;
    answers.push(() => new Promise((r) => { older = r; }), () => ({ status: 200, text: JSON.stringify(thread({ name: 'Newer' })) }));
    const first = lookAtThread('a/b');
    await vi.waitFor(() => expect(older).toBeTypeOf('function'));
    await lookAtThread('a/b');
    older({ status: 200, text: JSON.stringify(thread({ name: 'Older' })) });
    await first;
    expect(threadNow('a/b').thread).toMatchObject({ name: 'Newer' });
    // obid away keeps what shows; no such thread any more says why, in obid's words
    answers.push(() => ({ status: 503, text: '{}' }));
    await lookAtThread('a/b');
    expect(threadNow('a/b').thread).toMatchObject({ name: 'Newer' });
    expect(threadNow('a/b').gone).toBeUndefined();
    answers.push(() => ({ status: 404, text: JSON.stringify({ error: 'No such member.' }) }));
    await lookAtThread('a/b');
    expect(threadNow('a/b').gone).toBe('No such member.');
    // and that isn't remembered against the next open: it's asked again
    calls = [];
    openThread('a/b');
    expect(threadNow('a/b').gone).toBeUndefined();
    await vi.waitFor(() => expect(calls.map((c) => c.path)).toEqual(['/v1/obid/sphere/peers/a%2Fb']));
  });

  it('peer_8_mock_stays_mock_in_tour_only', () => {
    const name = PEER_CONVOS[0].with[0];
    // a member of a live Obysphere with the mock's name has none of its conversations, or its dots
    expect(mockConvosWith({ live: true }, PEER_CONVOS, name)).toEqual([]);
    // the mock's Obyspheres and the tour's sample stay as they were
    expect(memberOpens({})).toBe('mock');
    expect(mockConvosWith({}, PEER_CONVOS, name).length).toBeGreaterThan(0);
    expect(memberOpens({ sample: true })).toBe('tour');
  });
});

describe('the box', () => {
  it('peer_9_box_keeps_words_until_sent', async () => {
    const box = 'member:rig:dana';
    // words alone, signed by main (WORDS-12), then the thread again so they show as the box empties
    keep(box, 'How is it going?');
    expect(await askFromBox('dana', ' How is it going? ', box)).toBe(true);
    expect(calls.map((c) => [c.method, c.path, c.body])).toEqual([
      ['POST', '/v1/obid/sphere/peers/dana/ask', { text: 'How is it going?' }],
      ['GET', '/v1/obid/sphere/peers/dana', undefined],
    ]);
    expect(leftInBox('How is it going?', 'How is it going?', true)).toBe('');
    // a refusal is obid's own words, kept for the thread whichever view is open, and the draft stays
    keep(box, 'token');
    answers.push(() => ({ status: 422, text: JSON.stringify({ error: 'The secret scan found a possible secret: nothing was sent.' }) }));
    expect(await askFromBox('dana', 'token', box)).toBe(false);
    expect(threadNow('dana').problem).toBe('The secret scan found a possible secret: nothing was sent.');
    expect(draft(box)).toBe('token');
    answers.push(() => ({ status: 0, text: 'obid isn’t answering' }));
    expect(await askFromBox('dana', 'token', box)).toBe(false);
    expect(threadNow('dana').problem).toBe('obid isn’t answering');
    // the next words clear it
    expect(await askFromBox('dana', 'token', box)).toBe(true);
    expect(threadNow('dana').problem).toBeUndefined();
  });

  it('peer_13_one_send_per_owner_and_only_what_went_leaves_the_draft', async () => {
    const box = 'member:rig:bob';
    let answer!: (a: Answer) => void;
    answers.push(() => new Promise((r) => { answer = r; }));
    keep(box, 'first');
    const first = askFromBox('bob', 'first', box);
    await vi.waitFor(() => expect(answer).toBeTypeOf('function'));
    expect(threadNow('bob').sending).toBe('first');
    // the box put back from its draft, on the way back to this thread: nothing goes twice
    expect(await askFromBox('bob', 'first', box)).toBe(false);
    expect(calls.filter((c) => c.method === 'POST')).toHaveLength(1);
    // words typed while it went stay
    keep(box, 'first and more');
    answer({ status: 202, text: '{}' });
    expect(await first).toBe(true);
    expect(draft(box)).toBe('and more');
    expect(threadNow('bob').sending).toBeUndefined();
  });
});

describe('who it is', () => {
  it('peer_10_the_owner_id_shows_beside_any_other_name', () => {
    expect(whoseOby('dana', 'dana')).toBe('dana’s Oby');
    expect(whoseOby('Dima', 'aaron')).toBe('Dima’s Oby (aaron)');
    // a look-alike id can't pass as the real one, nor can a reversing mark turn it around
    expect(plainId('dima')).toBe('dima');
    expect(plainId('dіma')).toBe('d\\u{456}ma');
    expect(whoseOby('dima', 'dіma')).toBe('dima’s Oby (d\\u{456}ma)');
    expect(plainId('a‮b')).toBe('a\\u{202e}b');
    // a post goes by the directory's owner id, never by a name
    const out = html([post('a', { owner: 'dіma', from: 'obi-x' })]);
    expect(out).toContain('d\\u{456}ma’s Oby');
    expect(out).not.toContain('dіma');
  });

  it('peer_11_the_owner_s_words_are_theirs_and_every_other_word_is_text', () => {
    expect(html([asked()])).toContain('msg is-you');
    // a question from your conversation is your Oby's
    expect(html([asked({ by: 'obi' })])).toContain('Your Oby');
    // theirs is theirs, whatever it claims: no bubble of yours, no HTML, no link
    const theirs = html([post('a', { by: 'owner', owner: 'me', text: '<a href="https://evil.example">run <b>this</b></a>' })]);
    for (const not of ['is-you', 'is-mine', 'Your Oby', '<a ', '<b>']) expect(theirs).not.toContain(not);
    expect(theirs).toContain('&lt;a href=');
    expect(theirs).toContain('Not verified');
    // a post with no words has no bubble; its exchange's line says it
    expect(html([post('s', { kind: 'status', text: null, status: 'waiting_on_owner' })])).not.toContain('msg-bubble');
    // what the API allows reads, and what it doesn't isn't shown at all
    expect(peerPostOf(post('s', { text: null }))).toMatchObject({ id: 's', text: null });
    for (const bad of [{ ...post('x'), text: 3 }, { ...post('x'), exchange: null }, { ...post('x'), exchange: { state: 'sent' } }, { ...post('x'), from: null }, 'words', null]) expect(peerPostOf(bad)).toBeNull();
    expect(peerThreadOf({ ...thread(), posts: [post('a'), { nope: 1 }] })?.posts.map((p) => p.id)).toEqual(['a']);
  });
});

describe('where it stands', () => {
  it('peer_12_nobody_is_called_gone_and_a_quiet_question_is_the_clock_s', () => {
    // obid hasn't read the directory: null stays null, and the box stays
    const unknown = peerThreadOf({ owner: 'dana', name: 'dana', member: null, takesQuestions: null, posts: [] })!;
    expect(unknown).toMatchObject({ member: null, takesQuestions: null });
    expect(boxShut(unknown)).toBeNull();
    expect(boxShut(thread())).toBeNull();
    expect(boxShut(thread({ member: false }))).toBe('dana left this Obysphere.');
    expect(boxShut(thread({ takesQuestions: false }))).toBe('Dana’s Oby (dana) doesn’t take questions.');
    // the clock decides, and says when next to look
    const before = Date.parse('2026-11-03T12:01:00Z'), after = Date.parse('2026-11-03T12:02:00Z');
    const dana = thread();
    expect(standing(open, dana, before)).toBe('Asked');
    expect(standing(open, dana, after)).toBe('Dana’s Oby hasn’t answered yet.');
    expect(nextQuiet([asked()], before)).toBe(after);
    expect(nextQuiet([asked()], after)).toBe(Infinity);
    expect(standing({ ...open, quietAt: null, waitingOnThem: true }, dana, after)).toBe('It’s waiting on Dana.');
    expect(standing({ ...open, quietAt: null, state: 'done' }, dana, after)).toBeNull();
    expect(standing({ ...open, quietAt: null, state: 'declined' }, dana, after)).toBe('Dana’s Oby declined without saying why.');
    // one line per exchange, under its last post with words: a bare status doesn't hide it
    const waiting = { ...open, quietAt: null, waitingOnThem: true }, asking = withWords([asked({ exchange: waiting }), post('s', { kind: 'status', text: null, exchange: waiting }), post('later', { exchange: { ...open, id: 'x2' } })]);
    expect(asking.map((_, i) => lineUnder(asking, i, dana, after))).toEqual(['It’s waiting on Dana.', 'Dana’s Oby hasn’t answered yet.']);
    // and it shows, in a thread, whose byline is already their owner id
    const shown = renderToStaticMarkup(<RoomThread posts={asking} obi="obi-me" standing={(i) => lineUnder(asking, i, dana, after)} />);
    expect(shown).toContain('It’s waiting on Dana.');
    expect(shown).not.toContain('room-from');
  });
});
