import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { PeerPost } from '../obid';

// The thread's screen and its row as they really render and wire up, on an obid that answers from
// here (docs/contracts/peer-thread.md, PEER-7 to PEER-15, PEER-24). Composer is a marker holding what it's given;
// clicks are the handlers React got; effects are run by hand, since a server render runs none.
const page = vi.hoisted(() => {
  const g = globalThis as Record<string, unknown>;
  const kept = new Map<string, string>();
  const the = {
    calls: [] as { method: string; path: string; body?: unknown }[],
    routes: {} as Record<string, () => { status: number; text: string } | Promise<{ status: number; text: string }>>,
    box: null as null | { placeholder: string; label: string; draft: string; say: (t: string) => Promise<boolean> },
    clicks: {} as Record<string, () => void>,
    effects: [] as (() => (() => void) | void)[],
    // how often a screen asked to show again: a server render can't, so the next out() is that
    asked: 0,
    onSphere: (() => {}) as (s: { sphere: string; name: string }) => void,
    chunk: (() => {}) as (text: string) => void,
    kept,
  };
  g.location = { search: '' };
  g.localStorage = { getItem: (k: string) => kept.get(k) ?? null, setItem: (k: string, v: string) => { kept.set(k, v); }, removeItem: (k: string) => { kept.delete(k); } };
  g.document = { createElement: () => ({}) };
  g.addEventListener = () => {};
  g.removeEventListener = () => {};
  g.innerWidth = 1000;
  g.window = { app: {
    onSphere: (f: typeof the.onSphere) => { the.onSphere = f; },
    obid: {
      call: async (method: string, path: string, body?: unknown) => {
        the.calls.push({ method, path, body });
        return the.routes[`${method} ${path}`]?.() ?? { status: 404, text: '{}' };
      },
      follow: (chunk: typeof the.chunk) => { the.chunk = chunk; return () => {}; },
    },
  } };
  return the;
});
vi.mock('react', async (original) => {
  const real = await original<typeof import('react')>();
  return {
    ...real,
    useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
    useEffect: (f: () => (() => void) | void) => { page.effects.push(f); },
    useState: (first: unknown) => { const [now] = real.useState(first); return [now, () => { page.asked += 1; }]; },
  };
});
vi.mock('react/jsx-dev-runtime', async (original) => {
  const real = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...real, jsxDEV: (type: never, props: { onClick?: () => void; 'aria-label'?: string }, ...rest: never[]) => {
    if (props?.onClick && props['aria-label']) page.clicks[props['aria-label']] = props.onClick;
    return (real.jsxDEV as (...a: unknown[]) => unknown)(type, props, ...rest);
  } };
});
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));
vi.mock('../Composer', () => ({
  default: (props: NonNullable<typeof page.box>) => { page.box = props; return <div data-box={props.label} />; },
  typingIn: () => false,
}));
const ok = (value: unknown, status = 200) => () => ({ status, text: JSON.stringify(value) });
const joined = { state: 'joined', url: 'http://sphere.test', sphere: 'rig', owner: 'alex', obiId: 'obi-me' };
const none = () => {};
const exchange = { id: 'x1', type: 'question', direction: 'outgoing', state: 'sent', waitingOnThem: true, quietAt: null };
const post = (id: string, more: Partial<PeerPost> = {}): PeerPost => ({
  id, seq: 1, recordedAt: '2026-11-03T12:00:09Z', from: 'obi-dana', owner: 'dana', kind: 'status', by: 'obi', createdAt: '2026-11-03T12:00:08Z', replyTo: null,
  text: null, about: [], state: 'sent', error: null, direction: 'received', status: 'waiting_on_owner', reason: null, exchange, ...more,
});
const thread = (more: object = {}) => ({ owner: 'dana', name: 'Dima', member: true, takesQuestions: true, posts: [], ...more });

type Mods = {
  Room: typeof import('../RoomView').RoomThread; View: typeof import('../PeerThread').default; Sidebar: typeof import('../Sidebar').default;
  peers: typeof import('../peers'); spheres: typeof import('../spheres'); live: typeof import('../live');
};
let m: Mods;
const out = () => renderToStaticMarkup(<m.View owner="dana" leaving={false} onClose={none} onGone={none} />).replaceAll('<!-- -->', '');
// the screen as it first shows, with what it does on opening done
async function opened(answer: () => { status: number; text: string }) {
  page.routes['GET /v1/obid/sphere/peers/dana'] = answer;
  page.box = null;
  page.effects = [];
  out();
  page.effects.forEach((f) => f());
  await vi.waitFor(() => expect(page.calls.some((c) => c.path === '/v1/obid/sphere/peers/dana')).toBe(true));
  await new Promise((r) => setTimeout(r, 0));
  page.box = null;
  return out();
}
const handlers = { onOpenMember: vi.fn(), onOpenOby: vi.fn() };
const row = () => renderToStaticMarkup(
  <m.Sidebar open atHome={false} width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenOwn={none} onOpenGroup={none}
    onOpenTask={none} onOpenChat={none} {...handlers} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
).replaceAll('<!-- -->', '');

beforeAll(async () => {
  page.routes['GET /v1/obid/sphere'] = ok(joined);
  // two Obys of one owner, one with the mock's name, and one whose id borrows a Cyrillic letter
  page.routes['GET /v1/obid/sphere/members'] = ok([{ owner: 'dana' }, { owner: 'dana' }, { owner: 'Dima' }, { owner: 'dіma' }]);
  const spheres = await import('../spheres');
  m = { spheres, peers: await import('../peers'), Room: (await import('../RoomView')).RoomThread, live: await import('../live'), View: (await import('../PeerThread')).default, Sidebar: (await import('../Sidebar')).default };
  await vi.waitFor(() => expect(m.spheres.liveSphere(joined, [], {})).toBeTruthy());
  await vi.waitFor(() => expect(page.calls.some((c) => c.path === '/v1/obid/sphere/members')).toBe(true));
  await new Promise((r) => setTimeout(r, 0));
});
beforeEach(() => { page.calls = []; handlers.onOpenMember.mockClear(); handlers.onOpenOby.mockClear(); });

describe('the row', () => {
  it('peer_7_the_row_is_a_button_that_opens_the_thread_once_per_owner', () => {
    const html = row();
    expect(html).toContain('aria-label="Chat with dana’s Oby"');
    expect(html.match(/aria-label="Chat with dana’s Oby"/g)).toHaveLength(1);
    page.clicks['Chat with dana’s Oby']();
    expect(handlers.onOpenMember).toHaveBeenCalledWith('dana');
    expect(handlers.onOpenOby).not.toHaveBeenCalled();
  });

  it('peer_8_a_live_row_shows_none_of_the_mock_s', () => {
    const html = row();
    expect(html).toContain('aria-label="Chat with Dima’s Oby"');
    expect(html).not.toContain('connector-count');
    expect(html).not.toContain('New messages');
  });

  it('peer_10_the_row_shows_a_look_alike_id_s_codes', () => {
    const html = row();
    expect(html).toContain('d\\u{456}ma’s Oby');
    expect(html).not.toContain('dіma');
  });
});

describe('the screen', () => {
  it('peer_7_opening_reads_it_and_the_stream_reads_it_again', async () => {
    await opened(ok(thread()));
    expect(page.calls.filter((c) => c.path === '/v1/obid/sphere/peers/dana')).toHaveLength(1);
    // obid's event, through the stream as it comes in, and the stream coming back
    for (const [path, value] of Object.entries({ '/v1/status': { accounts: [] }, '/v1/jobs?limit=100': [], '/v1/conversations?limit=200': [] })) page.routes[`GET ${path}`] = ok(value);
    page.calls = [];
    m.live.connect();
    page.chunk(': hello\n\n');
    await vi.waitFor(() => expect(page.calls.some((c) => c.path === '/v1/obid/sphere/peers/dana')).toBe(true));
    await vi.waitFor(() => expect(m.live.liveState().connection).toBe('live'));
    page.calls = [];
    page.chunk('event: peerThread\ndata: {"owner":"dana","posts":[]}\n\n');
    await vi.waitFor(() => expect(page.calls.map((c) => c.path)).toContain('/v1/obid/sphere/peers/dana'));
  });

  it('peer_10_title_and_box_name_who_with_the_owner_id', async () => {
    const html = await opened(ok(thread()));
    expect(html).toContain('aria-label="Dima’s Oby (dana)"');
    expect(page.box).toMatchObject({ label: 'Ask Dima’s Oby (dana)', placeholder: 'Ask Dima’s Oby (dana)…', draft: 'member:rig:dana' });
  });

  it('peer_11_no_words_no_bubble_and_the_line_says_where_it_stands', async () => {
    const asked = post('q', { from: 'obi-me', owner: 'alex', kind: 'question', by: 'owner', text: 'Your editor?', direction: 'sent', status: null });
    const html = await opened(ok(thread({ posts: [asked, post('s')] })));
    expect(html).toContain('msg is-you');
    expect(html.match(/msg-bubble/g)).toHaveLength(1);
    expect(html).toContain('It’s waiting on Dima (dana).');
    expect(html).not.toContain('obi-dana');
  });

  it('peer_12_only_false_shuts_the_box', async () => {
    expect(await opened(ok(thread({ member: null, takesQuestions: null, name: 'dana' })))).not.toContain('left this');
    expect(page.box).not.toBeNull();
    expect(await opened(ok(thread({ takesQuestions: false })))).toContain('Dima’s Oby (dana) doesn’t take questions.');
    expect(page.box).toBeNull();
    expect(await opened(ok(thread({ member: false })))).toContain('dana left this Obysphere.');
    expect(page.box).toBeNull();
  });

  it('peer_7_a_thread_obid_no_longer_has_says_why_and_opens_afresh', async () => {
    const html = await opened(ok({ error: 'No such member.' }, 404));
    expect(html).toContain('No such member.');
    expect(page.box).toBeNull();
    await opened(ok(thread()));
    expect(page.box).not.toBeNull();
  });

  it('peer_9_a_refusal_keeps_the_words_and_shows_obid_s', async () => {
    await opened(ok(thread()));
    page.routes['POST /v1/obid/sphere/peers/dana/ask'] = ok({ error: 'The secret scan found something: nothing was sent.' }, 422);
    expect(await page.box!.say('token')).toBe(false);
    expect(out()).toContain('The secret scan found something: nothing was sent.');
    expect(out()).toContain('role="alert"');
    page.routes['POST /v1/obid/sphere/peers/dana/ask'] = ok({}, 202);
    expect(await page.box!.say('fine')).toBe(true);
    expect(out()).not.toContain('role="alert"');
  });

  it('peer_13_words_on_their_way_hold_a_box_opened_meanwhile_and_a_refusal_waits_for_it', async () => {
    await opened(ok(thread()));
    let answer!: (a: { status: number; text: string }) => void;
    page.routes['POST /v1/obid/sphere/peers/dana/ask'] = () => new Promise((r) => { answer = r; });
    const say = page.box!.say;
    const first = say('Are you there?');
    await vi.waitFor(() => expect(answer).toBeTypeOf('function'));
    // clicked away and back: a new view of the same thread, with no box that could send them again
    page.box = null;
    const html = renderToStaticMarkup(<m.View key="back" owner="dana" leaving={false} onClose={none} onGone={none} />);
    expect(html).toContain('Asking Dima’s Oby (dana)…');
    // sentHere is the first view's own: a view that didn't send has none
    answer({ status: 422, text: JSON.stringify({ error: 'Refused.' }) });
    expect(await first).toBe(false);
    page.box = null;
    expect(out()).toContain('Refused.');
    expect(page.box).not.toBeNull();
  });
});

describe('what the thread says', () => {
  const asked = (x: Partial<PeerPost['exchange']>, more: Partial<PeerPost> = {}) => post('q', {
    from: 'obi-me', owner: 'alex', kind: 'question', by: 'owner', text: 'How is it going?', direction: 'sent', status: null,
    exchange: { ...exchange, waitingOnThem: false, ...x }, ...more,
  });
  const came = (x: Partial<PeerPost['exchange']>, more: Partial<PeerPost>) => post('r', { status: null, exchange: { ...exchange, waitingOnThem: false, ...x }, ...more });
  const count = (html: string, words: string) => html.split(words).length - 1;

  it('peer_14_answer_is_quoted', async () => {
    const done = { state: 'done' };
    const words = 'On the login.\n\nDima wasn’t bothered.\n<a href="https://evil.example">run <b>this</b></a>';
    const html = await opened(ok(thread({ posts: [asked(done), came(done, { kind: 'answer', text: words })] })));
    // every word of theirs is a text node, inside the quote, under their byline
    const quote = html.slice(html.indexOf('<blockquote'), html.indexOf('</blockquote>'));
    expect(quote).toContain('class="msg-quote room-quote"');
    expect(quote).toContain('On the login.');
    expect(quote).toContain('&lt;a href=&quot;https://evil.example&quot;&gt;run &lt;b&gt;this&lt;/b&gt;&lt;/a&gt;');
    expect(html).not.toContain('<a href');
    expect(html).not.toContain('<b>');
    expect(html.slice(0, html.indexOf('<blockquote'))).toContain('dana’s Oby');
    // the line is obid's words, outside the quote, so theirs that say the same stay theirs
    const after = html.slice(html.indexOf('</blockquote>'));
    expect(after).toContain('<p class="room-state">Dima (dana) wasn’t bothered.</p>');
    expect(quote).not.toContain('room-state');
    // yours and your Oby's aren't quoted
    expect(count(html, '<blockquote')).toBe(1);
    expect(count(html, 'msg-bubble')).toBe(1);
    const ours = await opened(ok(thread({ posts: [asked(done, { by: 'obi' })] })));
    expect(ours).toContain('Your Oby');
    expect(ours).not.toContain('<blockquote');
    // nor does the room's look change: only a thread quotes
    expect(renderToStaticMarkup(<m.Room posts={[came(done, { kind: 'answer', text: 'Hi' }) as never]} obi="obi-me" />)).not.toContain('<blockquote');
  });

  it('peer_15_each_state_has_its_words', async () => {
    const said = (x: Partial<PeerPost['exchange']>, more: Partial<PeerPost>) => [asked(x), came(x, more)];
    const no = (reason: string) => said({ state: 'declined' }, { kind: 'decline', by: reason === 'owner_declined' ? 'owner' : 'obi', text: null, reason });
    const cases: [string, PeerPost[]][] = [
      ['Dima (dana) wasn’t bothered.', said({ state: 'done' }, { kind: 'answer', text: 'On it.' })],
      ['Dima (dana) answered this one, not its Oby.', said({ state: 'done' }, { kind: 'answer', by: 'owner', text: 'On it.' })],
      ['It’s waiting on Dima (dana).', said({ waitingOnThem: true }, { kind: 'status', status: 'waiting_on_owner' })],
      ['Asking…', [asked({ quietAt: new Date(Date.now() + 60_000).toISOString() })]],
      ['Dima’s Oby (dana) hasn’t answered yet.', [asked({ quietAt: '2020-01-01T00:00:00Z' })]],
      ['Dima (dana) said no to this one.', no('owner_declined')],
      ['Dima’s Oby (dana) is busy with other questions and didn’t take this one. Ask again later.', no('busy')],
      ['Dima’s Oby (dana) doesn’t take questions like this one.', no('out_of_scope')],
      ['This Obysphere’s rules stopped Dima’s Oby (dana) answering. An admin can change them.', no('policy')],
      // a reason is what they say: it picks words and never shows, whatever it is
      ['Dima’s Oby (dana) declined without saying why.', no('constructor')],
      ['Dima’s Oby (dana) never answered: the question expired.', [asked({ state: 'expired' })]],
    ];
    for (const [words, posts] of cases) {
      const html = await opened(ok(thread({ posts })));
      expect(count(html, `<p class="room-state">${words}</p>`), words).toBe(1);
      expect(count(html, 'class="room-state"'), words).toBe(html.includes('msg-quote') ? 2 : 1);
    }
    expect(await opened(ok(thread({ posts: no('constructor') })))).not.toContain('>constructor');
    // a decline's note is theirs, quoted, with obid's words under it
    const noted = await opened(ok(thread({ posts: said({ state: 'declined' }, { kind: 'decline', by: 'owner', reason: 'owner_declined', text: 'Heads down.' }) })));
    expect(noted.indexOf('Dima (dana) said no to this one.')).toBeGreaterThan(noted.indexOf('</blockquote>'));
    // someone else's question isn't said to be about them
    const theirs = { direction: 'incoming', state: 'done' };
    const incoming = await opened(ok(thread({ posts: [post('t', { kind: 'question', text: 'Your editor?', status: null, exchange: { ...exchange, waitingOnThem: false, ...theirs } }), asked(theirs, { kind: 'answer', by: 'obi', text: 'Zed.', id: 'mine' })] })));
    expect(incoming).not.toContain('bothered');
    expect(incoming).not.toContain('not its Oby');
    expect((await opened(ok(thread({ posts: [asked({ direction: 'incoming', state: 'declined' })] }))))).toContain('>Declined<');
    // the Sphere's rules keep no post: it's the refusal, in obid's words, with the box's words kept
    await opened(ok(thread()));
    const rules = 'This Obysphere\'s rules don\'t allow questions yet. An admin can change that.';
    page.routes['POST /v1/obid/sphere/peers/dana/ask'] = ok({ error: rules }, 502);
    expect(await page.box!.say('Are you there?')).toBe(false);
    expect(out()).toContain('This Obysphere&#x27;s rules don&#x27;t allow questions yet. An admin can change that.');
    page.routes['POST /v1/obid/sphere/peers/dana/ask'] = ok({}, 202);
    await page.box!.say('Now?');
  });

  afterEach(() => { vi.useRealTimers(); });
  it('peer_15_hasn_t_answered_comes_by_the_clock_once_and_goes_with_the_answer', async () => {
    const start = Date.now(), quietAt = new Date(start + 60_000).toISOString().replace(/\.\d+Z/, 'Z'), at = Date.parse(quietAt);
    const open = thread({ posts: [asked({ quietAt })] });
    await opened(ok(open));
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'], now: at - 1_000 });
    const quiet = 'Dima’s Oby (dana) hasn’t answered yet.';
    const show = () => { page.effects = []; const html = out(); return { html, stops: page.effects.map((f) => f()) }; };
    const first = show();
    expect(first.html).toContain('Asking…');
    expect(first.html).not.toContain(quiet);
    expect(vi.getTimerCount()).toBe(1);
    // left before it passes: no timer stays behind, and nothing shows again
    page.asked = 0;
    first.stops.forEach((stop) => stop?.());
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(2_000);
    expect(page.asked).toBe(0);
    vi.setSystemTime(at - 1_000);
    // stayed: at that moment, with no event, and not a moment before
    show();
    vi.advanceTimersByTime(999);
    expect(page.asked).toBe(0);
    vi.advanceTimersByTime(1);
    expect(page.asked).toBe(1);
    expect(vi.getTimerCount()).toBe(0);
    const said = show();
    expect(said.html.split(quiet)).toHaveLength(2);
    expect(said.html).not.toContain('Asking…');
    // told once: nothing more to wait for
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(600_000);
    expect(page.asked).toBe(1);
    // and the answer takes its place
    const done = { state: 'done', quietAt: null };
    page.routes['GET /v1/obid/sphere/peers/dana'] = ok(thread({ posts: [asked(done), came(done, { kind: 'answer', text: 'On it.' })] }));
    await m.peers.lookAtThread('dana');
    const answered = show().html;
    expect(answered).not.toContain(quiet);
    expect(answered).toContain('Dima (dana) wasn’t bothered.');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('peer_24_a_name_look_alike_shows_the_owner_id', async () => {
    const done = { state: 'done' };
    const answered = [asked(done), came(done, { kind: 'answer', text: 'On it.', owner: 'aaron' })];
    const line = async (name: string, owner: string, posts: PeerPost[]) => {
      const html = await opened(ok(thread({ name, owner, posts })));
      return html.match(/<p class="room-state">([^<]*)<\/p>/g)?.at(-1)?.replace(/<[^>]+>/g, '');
    };
    // a card saying it's Dima, from aaron: every line says who it really is
    expect(await line('Dima', 'aaron', answered)).toBe('Dima (aaron) wasn’t bothered.');
    expect(await line('Dima', 'aaron', [asked({ waitingOnThem: true })])).toBe('It’s waiting on Dima (aaron).');
    expect(await line('Dima', 'aaron', [asked({ quietAt: '2020-01-01T00:00:00Z' })])).toBe('Dima’s Oby (aaron) hasn’t answered yet.');
    // a letter from another alphabet shows as what it is, in the id and in a name that's only the id
    expect(await line('dima', 'dіma', answered)).toBe('dima (d\\u{456}ma) wasn’t bothered.');
    expect(await line('dіma', 'dіma', answered)).toBe('d\\u{456}ma wasn’t bothered.');
    // only the id's capitals: said once
    expect(await line('Dana', 'dana', answered)).toBe('Dana wasn’t bothered.');
    expect(await line('dana', 'dana', [asked({ quietAt: '2020-01-01T00:00:00Z' })])).toBe('dana’s Oby hasn’t answered yet.');
  });
});

describe('leaving', () => {
  it('peer_7_leaving_the_sphere_closes_the_thread_and_forgets_it', async () => {
    await opened(ok(thread()));
    const live = m.spheres.liveSphere(joined, [{ owner: 'dana' }], {})!;
    expect(m.spheres.outOfSphere({ kind: 'member', id: 'dana' }, [live])).toBe(false);
    expect(m.spheres.outOfSphere({ kind: 'member', id: 'dana' }, [])).toBe(true);
    expect(m.spheres.outOfSphere({ kind: 'oby', id: 'dana' }, [])).toBe(false);
    page.routes['GET /v1/obid/sphere'] = ok({ state: 'none', url: null, sphere: null });
    page.onSphere({ sphere: 'rig', name: 'rig' });
    await vi.waitFor(() => expect(m.peers.threadNow('dana')).toEqual({}));
  });
});
