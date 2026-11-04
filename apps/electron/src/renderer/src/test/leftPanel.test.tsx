import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';

// No request waiting on the owner goes unseen once the Tasks list and Home's icon are gone
// (docs/contracts/chat-first.md, CHATFIRST-25 to 31). From obid's own answers, through live.ts, to the
// real Sidebar and AskStack rendered: what obid says on one side, the row, the icon and the card on the other.
vi.mock('react', async (original) => ({
  ...await original<typeof import('react')>(),
  useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
}));
// clicks are the handlers React got, by a button's label or else its class; a server render keeps no state
const clicks = vi.hoisted(() => ({}) as Record<string, () => void>);
vi.mock('react/jsx-dev-runtime', async (original) => {
  const real = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...real, jsxDEV: (type: never, props: { onClick?: () => void; 'aria-label'?: string; className?: string }, ...rest: never[]) => {
    if (props?.onClick) clicks[props['aria-label'] ?? props.className ?? ''] = props.onClick;
    return (real.jsxDEV as (...a: unknown[]) => unknown)(type, props, ...rest);
  } };
});
// Home's heavy pieces as markers; the stack in it is the real one
vi.mock('../ChatView', () => ({ default: ({ id }: { id: string }) => <div data-chat={id} />, ThreadTask: () => null, clock: () => '' }));
vi.mock('../Composer', () => ({ default: () => <div data-box="" />, typingIn: () => false }));
vi.mock('../Oby', () => ({ default: () => <div data-oby="" /> }));

let answers: Record<string, unknown> = {};
let feed: (text: string) => void = () => {};
const flush = async () => { for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0)); };
const at = '2026-10-30T10:00:00Z';
const job = (id: string, conversation: string | null, more: Record<string, unknown> = {}) => ({
  id, request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix', base: 'dev', issue: null, kind: 'work',
  status: 'running', step: 'Working', summary: '', created: at, finished: null, commits: 0, published: null, rounds: 1, round: 1, check: null,
  model: null, dismissed: null, permission: null, pipeline: null, title: `Task ${id}`, archived: null, conversation, ...more,
});
const talk = (id: string, more: Record<string, unknown> = {}) => ({ id, title: `Talk ${id}`, pinned: false, created: at, lastActivity: at, cursor: '', ...more });
const item = (id: string, kind: string, more: Record<string, unknown> = {}) => ({
  id, kind, job: '', title: `Item ${id}`, text: 'Its words', options: ['Allow', 'Deny'], details: null, created: at, state: 'open', later: null, headline: null, recommended: null, ...more,
});

// The app connected to an obid that has this, with the two things on screen rendered as they stand
async function on({ jobs = [], inbox = [], conversations = [], limits = [] }: { jobs?: unknown[]; inbox?: unknown[]; conversations?: unknown[]; limits?: unknown[] }) {
  vi.resetModules();
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  vi.stubGlobal('innerWidth', 1000);
  vi.stubGlobal('addEventListener', () => {});
  vi.stubGlobal('removeEventListener', () => {});
  vi.stubGlobal('window', { app: { obid: {
    call: async (method: string, path: string) => {
      const a = answers[`${method} ${path}`];
      return a === undefined ? { status: 404, text: '{"error":"Not found."}' } : { status: 200, text: JSON.stringify(a) };
    },
    confirm: async () => ({ status: 200, text: '{}' }),
    follow: (onChunk: (text: string) => void) => { feed = onChunk; return () => {}; },
  } } });
  answers['GET /v1/status'] = { runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0 };
  answers['GET /v1/jobs?limit=100'] = jobs;
  answers['GET /v1/inbox'] = inbox;
  answers['GET /v1/conversations?limit=200'] = conversations;
  answers['GET /v1/limit-decisions'] = limits;
  const live = await import('../live');
  live.connect();
  feed('');
  await flush();
  expect(live.liveState().connection).toBe('live');
  const { default: Sidebar } = await import('../Sidebar');
  const { default: AskStack } = await import('../AskStack');
  const none = () => {};
  const spy = { onNewChat: vi.fn(), onHome: vi.fn(), onOpenArchive: vi.fn(), onOpenRoom: vi.fn(), onOpenTalk: vi.fn() };
  return {
    live,
    tasks: await import('../tasks'),
    spy,
    side: (more: { atHome?: boolean; activeTalk?: string } = {}) => renderToStaticMarkup(
      <Sidebar open atHome={more.atHome ?? true} activeTalk={more.activeTalk} width={362} onResize={none} onClose={none} onNewChat={spy.onNewChat} onHome={spy.onHome} onOpenGroup={none}
        onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none} onOpenTalk={none} onOpenArchive={spy.onOpenArchive} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={spy.onOpenRoom} />,
    ).replaceAll('<!-- -->', ''),
    stack: () => renderToStaticMarkup(<AskStack active onOpen={none} onOpenTalk={spy.onOpenTalk} />),
    home: async () => { const { default: Home } = await import('../Home'); return renderToStaticMarkup(<Home />).replaceAll('<!-- -->', ''); },
  };
}
// The panel on the demo, where a room is the mock's: a tile's click, as ROOMAPP-1 says
async function tile(search: string) {
  vi.resetModules();
  vi.stubGlobal('location', { search });
  vi.stubGlobal('window', {});
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}) });
  const { default: Sidebar } = await import('../Sidebar');
  const none = () => {}, onOpenRoom = vi.fn();
  renderToStaticMarkup(
    <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none} onOpenOby={none} onOpenMember={none}
      onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={onOpenRoom} />,
  );
  clicks.Deskree();
  return onOpenRoom;
}
const row = (html: string, title: string) => { const i = html.indexOf(`${title}</span>`); return i < 0 ? '' : html.slice(i, html.indexOf('</div>', i)); };
const lit = (html: string, title: string) => row(html, title).includes('aria-label="Needs you"');
const icon = (html: string) => /<button[^>]*class="[^"]*has-dot is-needs[^"]*"[^>]*aria-label="Chats"/.test(html);
const own = (html: string) => html.slice(html.indexOf('chat-own'), html.indexOf('New conversation')).includes('Needs you');

describe('a Sphere’s tile', () => {
  afterEach(() => { vi.unstubAllGlobals(); });

  it('ROOMAPP-1: chat first, it opens the Sphere’s room when there is one', async () => {
    expect(await tile('?everything&chat')).toHaveBeenCalledWith('deskree');
  });

  it('ROOMAPP-1: with no room it opens nothing over what you’re at', async () => {
    expect(await tile('?everything&chat&room=unavailable')).not.toHaveBeenCalled();
  });

  it('CHATFIRST-5: ?everything shows Sangam’s design as it was: its tile opens the column alone', async () => {
    expect(await tile('?everything')).not.toHaveBeenCalled();
  });
});

describe('what waits on the owner', () => {
  beforeEach(() => { answers = {}; });
  afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

  for (const [kind, words] of [['permission', 'Run this?'], ['agentQuestion', 'Which store?'], ['plan', 'Approve the plan?'], ['merge', 'Ready to merge'], ['question', 'Should I go on?']] as const) {
    it(`CHATFIRST-25: a job's ${kind} lights its conversation's row and the Chats icon, and is on the stack`, async () => {
      const s = await on({
        jobs: [job('j', 'c-old')], conversations: [talk('c-new', { lastActivity: '2026-11-01T10:00:00Z' }), talk('c-old')],
        inbox: [item('i', kind, { job: 'j', title: words, text: words, headline: words })],
      });
      expect(lit(s.side(), 'Talk c-old')).toBe(true);
      expect(lit(s.side(), 'Talk c-new')).toBe(false);
      expect(icon(s.side())).toBe(true);
      expect(s.stack()).toContain(words);
    });
  }

  it('CHATFIRST-26: a job past the latest 100 still shows: it’s fetched, its row lights, and it’s on the stack', async () => {
    answers['GET /v1/jobs/old'] = job('old', 'c-old');
    const s = await on({
      jobs: Array.from({ length: 100 }, (_, i) => job(`n${i}`, null, { status: 'done', created: '2026-11-01T10:00:00Z' })),
      conversations: [talk('c-old')],
      inbox: [item('i', 'permission', { job: 'old', title: 'Run this?', headline: 'Run this?' })],
    });
    expect(s.tasks.tasksWith('needs').map((t) => t.id)).toEqual(['old']);
    expect(lit(s.side(), 'Talk c-old')).toBe(true);
    expect(icon(s.side())).toBe(true);
    expect(s.stack()).toContain('Run this?');
  });

  it('CHATFIRST-27: the sharing card (a Confirm with no job or conversation) is on the stack, and lights the icon and Oby’s row', async () => {
    const s = await on({
      conversations: [talk('c1')],
      inbox: [item('share', 'confirm', { title: 'Can I tell people in your Obysphere how your tasks are going, in every repository?', options: ['Confirm', 'Cancel'], confirm: { action: 'share_status', target: '', payload: null } })],
    });
    expect(s.stack()).toContain('Can I tell people in your Obysphere');
    expect(s.stack()).toContain('Confirm');
    expect(icon(s.side())).toBe(true);
    expect(own(s.side())).toBe(true);
    expect(lit(s.side(), 'Talk c1')).toBe(false);
  });

  it('CHATFIRST-28: a conversation’s own Confirm and an offer light its row, and the card gets the way to it', async () => {
    const offer = { id: 'o1', what: 'Fix it', task: 'Fix it', untrusted: false };
    const s = await on({
      conversations: [talk('c1'), talk('c2'), talk('c3')],
      inbox: [
        item('conf', 'confirm', { conversation: 'c1', options: ['Confirm', 'Cancel'] }),
        item('offer', 'start', { conversation: 'c2', offer, options: ['Start', 'Cancel'] }),
      ],
    });
    expect(lit(s.side(), 'Talk c1')).toBe(true);
    expect(lit(s.side(), 'Talk c2')).toBe(true);
    expect(lit(s.side(), 'Talk c3')).toBe(false);
    expect(own(s.side())).toBe(false);
    expect(s.stack()).toContain('Open conversation');
    clicks['tc-link ask-open']();
    expect(s.spy.onOpenTalk).toHaveBeenCalledWith('c1');
  });

  it('CHATFIRST-28: put off, its row goes quiet until its time, and it’s back', async () => {
    vi.useFakeTimers({ toFake: ['Date'], now: new Date('2026-11-03T12:00:00Z') });
    const s = await on({ conversations: [talk('c1')], inbox: [item('conf', 'confirm', { conversation: 'c1', options: ['Confirm', 'Cancel'], later: '2026-11-03T13:00:00Z' })] });
    expect(lit(s.side(), 'Talk c1')).toBe(false);
    expect(icon(s.side())).toBe(false);
    feed(`event: inbox\ndata: ${JSON.stringify(item('conf', 'confirm', { conversation: 'c1', options: ['Confirm', 'Cancel'], later: '2026-11-03T11:00:00Z' }))}\n\n`);
    await flush();
    expect(lit(s.side(), 'Talk c1')).toBe(true);
    expect(icon(s.side())).toBe(true);
  });

  it('CHATFIRST-29: a question only you can answer lights "Questions from other Obys"; an ignored one doesn’t', async () => {
    const question = item('obi-1', 'obiRequest', { title: 'Question from dana', obi: { type: 'question' } });
    const talks = [talk('asked', { title: 'Questions from other Obys', fromObys: true }), talk('c1')];
    const s = await on({ conversations: talks, inbox: [question] });
    expect(lit(s.side(), 'Questions from other Obys')).toBe(true);
    expect(icon(s.side())).toBe(true);
    expect(lit(s.side(), 'Talk c1')).toBe(false);
    const ignored = await on({ conversations: talks, inbox: [{ ...question, triage: 'ignore' }] });
    expect(lit(ignored.side(), 'Questions from other Obys')).toBe(false);
  });

  it('CHATFIRST-29: a limit lights the conversation of each task it stopped', async () => {
    const s = await on({
      jobs: [job('j', 'c1')], conversations: [talk('c1'), talk('c2')],
      limits: [{ id: 'l1', revision: 1, account: 'a1', agent: 'claude', resetAt: null, jobs: [{ id: 'j', title: 'Task j' }], options: [{ kind: 'stop' }] }],
    });
    expect(lit(s.side(), 'Talk c1')).toBe(true);
    expect(lit(s.side(), 'Talk c2')).toBe(false);
    expect(icon(s.side())).toBe(true);
    expect(s.stack()).toContain('Task j');
  });

  it('CHATFIRST-30: a task with no conversation that needs you is on the stack and lights Oby’s row', async () => {
    const s = await on({ jobs: [job('j', null)], inbox: [item('i', 'permission', { job: 'j', title: 'Run this?', headline: 'Run this?' })] });
    expect(s.stack()).toContain('Run this?');
    expect(own(s.side())).toBe(true);
    expect(icon(s.side())).toBe(true);
  });

  it('CHATFIRST-30: a conversation deleted: its task needing you still shows, and a done one is in Archived', async () => {
    const s = await on({
      jobs: [job('waits', 'gone'), job('ended', 'gone', { status: 'done', finished: new Date().toISOString() })],
      inbox: [item('i', 'permission', { job: 'waits', title: 'Run this?', headline: 'Run this?' })],
    });
    expect(s.stack()).toContain('Run this?');
    expect(own(s.side())).toBe(true);
    expect(s.tasks.archivedTasks().map((a) => a.id)).toEqual(['ended']);
    expect(s.side()).toContain('sidebar-archived');
  });

  it('CHATFIRST-31: Later on a task: off the stack, its row still lit, and back on the stack when its time comes', async () => {
    vi.useFakeTimers({ toFake: ['Date'], now: new Date('2026-11-03T12:00:00Z') });
    const asked = item('i', 'permission', { job: 'j', title: 'Run this?', headline: 'Run this?' });
    const s = await on({ jobs: [job('j', 'c1')], conversations: [talk('c1')], inbox: [asked] });
    answers['POST /v1/inbox/i/answer'] = { ...asked, later: '2026-11-03T13:00:00Z', state: 'later' };
    await s.live.answerJob('j', 'Later');
    await flush();
    expect(s.stack()).not.toContain('Run this?');
    expect(lit(s.side(), 'Talk c1')).toBe(true);
    vi.setSystemTime(new Date('2026-11-03T13:00:01Z'));
    feed(`event: inbox\ndata: ${JSON.stringify({ ...asked, later: '2026-11-03T13:00:00Z', state: 'later' })}\n\n`);
    await flush();
    expect(s.stack()).toContain('Run this?');
    expect(lit(s.side(), 'Talk c1')).toBe(true);
  });

  it('CHATFIRST-31: settled elsewhere after Later, its row and the icon go quiet', async () => {
    const asked = item('i', 'permission', { job: 'j', title: 'Run this?', headline: 'Run this?' });
    const s = await on({ jobs: [job('j', 'c1')], conversations: [talk('c1')], inbox: [asked] });
    answers['POST /v1/inbox/i/answer'] = { ...asked, later: new Date(Date.now() + 3600_000).toISOString(), state: 'later' };
    await s.live.answerJob('j', 'Later');
    await flush();
    expect(lit(s.side(), 'Talk c1')).toBe(true);
    feed(`event: inbox\ndata: ${JSON.stringify({ ...asked, state: 'done' })}\n\n`);
    await flush();
    expect(lit(s.side(), 'Talk c1')).toBe(false);
    expect(icon(s.side())).toBe(false);
  });

  it('CHATFIRST-31: put off, across a restart: still off the stack, its row lit', async () => {
    const later = new Date(Date.now() + 3600_000).toISOString();
    const s = await on({ jobs: [job('j', 'c1')], conversations: [talk('c1')], inbox: [item('i', 'permission', { job: 'j', title: 'Run this?', headline: 'Run this?', later, state: 'later' })] });
    expect(s.stack()).not.toContain('Run this?');
    expect(lit(s.side(), 'Talk c1')).toBe(true);
    expect(icon(s.side())).toBe(true);
  });

  it('CHATFIRST-31: put off with no row: nothing lit, then back by itself when its time comes, with no event', async () => {
    const later = new Date(Date.now() + 150).toISOString();
    const s = await on({ jobs: [job('j', null)], inbox: [item('i', 'permission', { job: 'j', title: 'Run this?', headline: 'Run this?', later, state: 'later' })] });
    expect(s.stack()).not.toContain('Run this?');
    expect(icon(s.side())).toBe(false);
    expect(own(s.side())).toBe(false);
    await new Promise((r) => setTimeout(r, 900));
    expect(s.stack()).toContain('Run this?');
    expect(own(s.side())).toBe(true);
    expect(icon(s.side())).toBe(true);
  });

  it('CHATFIRST-28: a Confirm put off comes back by itself when its time comes, with no event', async () => {
    const later = new Date(Date.now() + 150).toISOString();
    const s = await on({ conversations: [talk('c1')], inbox: [item('conf', 'confirm', { conversation: 'c1', options: ['Confirm', 'Cancel'], later, state: 'later' })] });
    expect(lit(s.side(), 'Talk c1')).toBe(false);
    await new Promise((r) => setTimeout(r, 900));
    expect(lit(s.side(), 'Talk c1')).toBe(true);
  });

  it('CHATFIRST-26: an item that comes as an event for a job past the latest 100 fetches it', async () => {
    answers['GET /v1/jobs/old'] = job('old', 'c-old');
    const s = await on({ conversations: [talk('c-old')] });
    expect(icon(s.side())).toBe(false);
    feed(`event: inbox\ndata: ${JSON.stringify(item('i', 'permission', { job: 'old', title: 'Run this?', headline: 'Run this?' }))}\n\n`);
    await flush();
    expect(lit(s.side(), 'Talk c-old')).toBe(true);
    expect(s.stack()).toContain('Run this?');
  });

  it('CHATFIRST-22: Oby’s question back lights its conversation’s row and the icon', async () => {
    const s = await on({ conversations: [talk('c1'), talk('c2')] });
    answers['POST /v1/plans'] = { kind: 'question', said: 'which one?', question: { text: 'Which one?', options: ['A', 'B'] }, jobs: [], conversation: 'c1' };
    s.live.ask('Fix it', 'c1');
    await flush();
    expect(lit(s.side(), 'Talk c1')).toBe(true);
    expect(lit(s.side(), 'Talk c2')).toBe(false);
    expect(icon(s.side())).toBe(true);
    expect(own(s.side())).toBe(false);
  });

  it('CHATFIRST-19: with no conversations and a Confirm waiting, Oby’s row says so and not All quiet', async () => {
    const s = await on({ inbox: [item('conf', 'confirm', { options: ['Confirm', 'Cancel'] })] });
    expect(s.side()).not.toContain('All quiet');
    expect(own(s.side())).toBe(true);
  });

  it('CHATFIRST-19: Oby’s row opens the empty chat', async () => {
    const s = await on({ conversations: [talk('c1')] });
    s.side();
    clicks['connector chat-own']();
    expect(s.spy.onNewChat).toHaveBeenCalledOnce();
  });

  it('CHATFIRST-5: Chats from a page brings your conversation back, and not from a conversation or the empty chat', async () => {
    const s = await on({ conversations: [talk('c1')] });
    s.side({ atHome: false });
    clicks.Chats();
    expect(s.spy.onHome).toHaveBeenCalledOnce();
    s.side({ atHome: false, activeTalk: 'c1' });
    clicks.Chats();
    s.side({ atHome: true });
    clicks.Chats();
    expect(s.spy.onHome).toHaveBeenCalledOnce();
  });

  it('CHATFIRST-20: Archived opens, and a task in it opens its page', async () => {
    const s = await on({ jobs: [job('ended', 'gone', { status: 'done', finished: new Date().toISOString() })] });
    s.side();
    clicks['sidebar-archived']();
    expect(s.spy.onOpenArchive).toHaveBeenCalledOnce();
    const { default: ArchiveView } = await import('../ArchiveView');
    const onOpenTask = vi.fn();
    renderToStaticMarkup(<ArchiveView leaving={false} onClose={() => {}} onGone={() => {}} onOpenTask={onOpenTask} />);
    clicks['archive-row is-link']();
    expect(onOpenTask).toHaveBeenCalledWith('ended');
  });

  it('CHATFIRST-23: chats_4_asks_still_show: a card is on the empty chat, under its line', async () => {
    const s = await on({ jobs: [job('j', null)], inbox: [item('i', 'permission', { job: 'j', title: 'Run this?', headline: 'Run this?' })] });
    expect(await s.home()).toMatch(/class="attention"[^]*What do you want me to work on\?[^]*ask-card is-up[^]*Run this\?/);
  });

  it('CHATFIRST-23: chats_4_asks_still_show: a card is over a conversation', async () => {
    const s = await on({ jobs: [job('j', 'c1')], conversations: [talk('c1')], inbox: [item('i', 'permission', { job: 'j', title: 'Run this?', headline: 'Run this?' })] });
    expect(await s.home()).toMatch(/class="asks-over"><section[^]*ask-card is-up[^]*Run this\?/);
  });
});
