import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { liveBridge } from './liveBridge';

// A conversation's state at a glance (docs/contracts/conversation-states.md, CHATSTATE-1 to 9): from
// obid's own answers, through live.ts, to the real side panel, menu, line and refusals rendered; a
// click is the handler React got, and what reaches obid is what the test's obid was asked.
vi.mock('react', async (original) => ({
  ...await original<typeof import('react')>(),
  useSyncExternalStore: (_: unknown, get: () => unknown) => get(),
}));
vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
// clicks by a button's label, else its words, else its class; a server render keeps no state
const clicks = vi.hoisted(() => ({}) as Record<string, () => void>);
vi.mock('react/jsx-dev-runtime', async (original) => {
  const real = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...real, jsxDEV: (type: never, props: { onClick?: () => void; 'aria-label'?: string; className?: string; children?: unknown }, ...rest: never[]) => {
    if (props?.onClick) clicks[props['aria-label'] ?? (typeof props.children === 'string' ? props.children : props.className ?? '')] = props.onClick;
    return (real.jsxDEV as (...a: unknown[]) => unknown)(type, props, ...rest);
  } };
});
vi.mock('../ChatView', () => ({ default: () => null, ThreadTask: () => null, clock: () => '' }));
vi.mock('../Composer', () => ({ default: () => null, typingIn: () => false }));

const at = '2026-11-17T10:00:00Z';
const WHY = 'settings.json sets CLAUDE_CODE_X in its env';
const account = (id: string, agent: string, name: string, more: Record<string, unknown> = {}) =>
  ({ id, agent, name, signedIn: true, fiveHour: 0.1, sevenDay: 0.1, sevenDayResets: null, running: 0, ...more });
const job = (id: string, conversation: string | null, more: Record<string, unknown> = {}) => ({
  id, request: 'Fix it', agent: 'claude', account: 'a1', repo: 'deskree-inc/oby', branch: 'oby/fix', base: 'dev', issue: null, kind: 'work',
  status: 'running', step: 'Working', summary: '', created: at, finished: null, commits: 0, published: null, rounds: 1, round: 1, check: null,
  model: null, dismissed: null, permission: null, pipeline: null, title: `Task ${id}`, archived: null, conversation, turn: null, ...more,
});
const talk = (id: string, more: Record<string, unknown> = {}) => ({ id, title: `Talk ${id}`, pinned: false, created: at, lastActivity: at, cursor: '', seen: at, ...more });

let asked: { method: string; path: string; body: unknown }[] = [];
let answers: Record<string, unknown> = {};
const flush = async () => { for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0)); };
const dispatched = vi.fn();

// The app connected to an obid that has this
async function on({ accounts = [] as unknown[], jobs = [] as unknown[], conversations = [] as unknown[], inbox = [] as unknown[], capabilities = ['conversation-marks-v1'] }) {
  vi.resetModules();
  for (const k of Object.keys(clicks)) delete clicks[k];
  asked = []; answers = {}; dispatched.mockClear();
  vi.stubGlobal('location', { search: '' });
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  vi.stubGlobal('document', { createElement: () => ({}), body: {} });
  vi.stubGlobal('innerWidth', 1000);
  vi.stubGlobal('innerHeight', 800);
  vi.stubGlobal('addEventListener', () => {});
  vi.stubGlobal('removeEventListener', () => {});
  vi.stubGlobal('dispatchEvent', dispatched);
  vi.stubGlobal('CustomEvent', class { constructor(public type: string, public init: { detail: unknown }) {} });
  const bridge = liveBridge();
  vi.stubGlobal('window', { app: { obid: {
    call: async (method: string, path: string, body?: unknown) => {
      asked.push({ method, path, body });
      const a = answers[`${method} ${path}`];
      if (a instanceof Error) return { status: 409, text: JSON.stringify({ error: a.message }) };
      return a === undefined ? { status: 404, text: '{"error":"Not found."}' } : { status: 200, text: JSON.stringify(a) };
    },
    ...bridge.obid,
  } } });
  const status = { runner: { version: '0.2.4', host: 'mac' }, accounts, models: {}, maxRunning: 6, running: 0, talkAccount: null, capabilities };
  answers['GET /v1/status'] = status;
  const live = await import('../live');
  live.connect();
  bridge.sync({ status, jobs, conversations, inbox });
  await flush();
  const chats = await import('../chats');
  const tasks = await import('../tasks');
  const none = () => {};
  const { default: Sidebar } = await import('../Sidebar');
  return {
    live, chats, tasks,
    chat: (id: string) => chats.useChats().find((c) => c.id === id)!,
    task: (id: string) => tasks.useTasks().find((t) => t.id === id)!,
    render: (node: React.ReactNode) => renderToStaticMarkup(<>{node}</>).replaceAll('<!-- -->', ''),
    side: () => renderToStaticMarkup(
      <Sidebar open atHome width={362} onResize={none} onClose={none} onNewChat={none} onHome={none} onOpenGroup={none} onOpenTask={none} onOpenChat={none}
        onOpenOby={none} onOpenMember={none} onOpenTalk={none} onOpenArchive={none} onOpenSetting={none} onOpenSphereSetting={none} onOpenKnowledge={none} onOpenRoom={none} />,
    ).replaceAll('<!-- -->', ''),
  };
}
const rowOf = (html: string, title: string) => { const i = html.indexOf(`${title}</span>`); return i < 0 ? '' : html.slice(html.lastIndexOf('<div role="button"', i), html.indexOf('</div>', i)); };
const sent = (method: string, path: string) => asked.filter((a) => a.method === method && a.path === path).map((a) => a.body);

afterEach(() => { vi.unstubAllGlobals(); });
// the first import of the app's modules is slow on a loaded machine: once, before the clock starts
beforeAll(async () => { await on({}); vi.unstubAllGlobals(); }, 120_000);
const AT = { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 } as DOMRect;

describe('CHATSTATE-1 to 3: the line over the box when its account can’t run', () => {
  it('chatstate_1_a_conversation_whose_account_cant_run_says_so_over_the_box', async () => {
    const app = await on({ accounts: [account('a1', 'claude', 'Personal', { cantRun: WHY }), account('cx', 'codex', 'Work')], conversations: [talk('c1', { talkAccount: 'a1' })] });
    const { default: CantRunLine } = await import('../CantRunLine');
    const html = app.render(<CantRunLine chat={app.chat('c1')} />);
    expect(html).toContain(`Claude Code · Personal can’t start: ${WHY}`);
    expect(html).toContain('>Sign in</button>');
    expect(html).toContain('>Use another account</button>');
  });

  it('chatstate_1_a_failed_task_on_a_signed_out_account_says_so', async () => {
    const app = await on({ accounts: [account('a1', 'claude', 'Personal', { signedIn: false })], jobs: [job('j1', null, { status: 'failed', finished: at, reason: 'Not logged in' })] });
    const { default: CantRunLine } = await import('../CantRunLine');
    const html = app.render(<CantRunLine task={app.task('j1')} />);
    expect(html).toContain('Claude Code · Personal is signed out');
    expect(html).not.toContain('Use another account');  // none other can run
  });

  it('chatstate_1_no_line_while_the_account_can_run', async () => {
    const app = await on({ accounts: [account('a1', 'claude', 'Personal')], conversations: [talk('c1', { talkAccount: 'a1' })], jobs: [job('j1', null, { status: 'failed', finished: at })] });
    const { default: CantRunLine } = await import('../CantRunLine');
    expect(app.render(<><CantRunLine chat={app.chat('c1')} /><CantRunLine task={app.task('j1')} /></>)).toBe('');
  });

  it('chatstate_2_sign_in_starts_the_accounts_own_sign_in_in_settings', async () => {
    const app = await on({ accounts: [account('a1', 'claude', 'Personal', { cantRun: WHY }), account('cx', 'codex', 'Work')], conversations: [talk('c1', { talkAccount: 'a1' })] });
    const { default: CantRunLine } = await import('../CantRunLine');
    app.render(<CantRunLine chat={app.chat('c1')} />);
    clicks['Sign in']();
    expect(dispatched).toHaveBeenCalledWith(expect.objectContaining({ type: 'oby:open-setting', init: { detail: 'agents' } }));
    const { signInAction, accountRow } = await import('../SettingsView');
    const row = (a: Record<string, unknown>) => accountRow(a as never, Date.now());
    // an account obid signs in: Sign In, as its row's own button
    expect(signInAction(row(account('a1', 'claude', 'Personal', { cantRun: WHY })), false)).toBe('signIn');
    expect(signInAction(row(account('a2', 'cursor', 'Side', { signedIn: false })), false)).toBe('signIn');
    // a Codex login that ended, where ChatGPT is offered: Continue with ChatGPT
    expect(signInAction(row(account('cx', 'codex', 'Work', { signedIn: false, login: 'chatgpt' })), true)).toBe('chatgpt');
    // the tool's own login on this Mac, which obid never signs in: its row says how; one that can run: nothing
    expect(signInAction(row(account('default', 'claude', 'Mine', { cantRun: WHY })), false)).toBeNull();
    expect(signInAction(row(account('a1', 'claude', 'Personal')), false)).toBeNull();
  });

  it('chatstate_3_another_account_is_one_that_can_run_the_same_tools_first', async () => {
    const { otherAccount } = await import('../chatState');
    const a = (id: string, agent: string, more: Record<string, unknown> = {}) => ({ id, agent, name: id, payer: 'You', used: 0, resets: '', signedIn: true, ...more });
    const from = a('a1', 'claude', { cantRun: WHY });
    expect(otherAccount([from, a('cx', 'codex'), a('a2', 'claude')], from)?.id).toBe('a2');
    expect(otherAccount([from, a('cx', 'codex'), a('a2', 'claude', { full: true })], from)?.id).toBe('cx');
    expect(otherAccount([from, a('cx', 'codex', { signedIn: false }), a('a2', 'claude', { cantRun: 'x' })], from)).toBeUndefined();
  });

  it('chatstate_3_a_conversation_talks_through_it_and_a_task_moves', async () => {
    const app = await on({
      accounts: [account('a1', 'claude', 'Personal', { cantRun: WHY }), account('cx', 'codex', 'Work')],
      conversations: [talk('c1', { talkAccount: 'a1' })], jobs: [job('j1', null, { status: 'queued' })],
    });
    answers['PATCH /v1/conversations/c1'] = talk('c1', { talkAccount: 'cx' });
    answers['POST /v1/jobs/j1/move'] = job('j1', null, { status: 'queued', account: 'cx', agent: 'codex' });
    const { default: CantRunLine } = await import('../CantRunLine');
    app.render(<CantRunLine chat={app.chat('c1')} />);
    clicks['Use another account']();
    app.render(<CantRunLine task={app.task('j1')} />);
    clicks['Use another account']();
    await flush();
    expect(sent('PATCH', '/v1/conversations/c1')).toEqual([{ talkAccount: 'cx' }]);
    expect(sent('POST', '/v1/jobs/j1/move')).toEqual([{ account: 'cx' }]);
  });
});

describe('CHATSTATE-4: a refused click stays in its thread', () => {
  it('chatstate_4_a_refused_stop_stays_in_the_thread_until_dismissed', async () => {
    const app = await on({ jobs: [job('j1', 'c1')], conversations: [talk('c1')] });
    answers['POST /v1/jobs/j1/stop'] = new Error('It already ended.');
    answers['POST /v1/conversation/stop'] = new Error('Nothing is under way.');
    await app.live.stopJob('j1');
    await app.live.stopConversation('c1');
    const { default: Refusals } = await import('../Refusals');
    const html = app.render(<Refusals where={['c1', 'j1']} />);
    expect(html).toContain('It already ended.');
    expect(html).toContain('Nothing is under way.');
    // not Home's line, which comes and goes
    expect(app.live.liveState().problem).toBeNull();
    // another thread doesn't show it
    expect(app.render(<Refusals where={['c2']} />)).toBe('');
    await new Promise((r) => setTimeout(r, 50));
    expect(app.render(<Refusals where={['c1', 'j1']} />)).toContain('It already ended.');
    clicks.Dismiss();
    const left = app.render(<Refusals where={['c1', 'j1']} />);
    expect(left).toContain('It already ended.');
    expect(left).not.toContain('Nothing is under way.');
  });
});

describe('CHATSTATE-5 to 9: the rows', () => {
  it('chatstate_5_the_rows_mark_is_its_latest_tasks_end', async () => {
    const earlier = '2026-11-17T09:00:00Z';
    const app = await on({
      conversations: [talk('c1'), talk('c2'), talk('c3'), talk('c4')],
      jobs: [
        job('j1', 'c1', { status: 'done', finished: at }),
        job('j2', 'c2', { status: 'failed', finished: at }),
        // the latest decides: failed before, done since
        job('j3', 'c3', { status: 'failed', created: earlier, finished: earlier }), job('j4', 'c3', { status: 'done', finished: at }),
        job('j5', 'c4', { status: 'stopped', finished: at }),
      ],
    });
    const html = app.side();
    expect(rowOf(html, 'Talk c1')).toContain('aria-label="Latest task: Done"');
    expect(rowOf(html, 'Talk c2')).toContain('aria-label="Latest task: Failed"');
    expect(rowOf(html, 'Talk c3')).toContain('aria-label="Latest task: Done"');
    expect(rowOf(html, 'Talk c4')).toContain('aria-label="Latest task: Stopped"');
    const { rowMark } = await import('../chatState');
    const done = app.task('j1');
    expect([rowMark({ needs: true, working: true, unread: true, latest: done }), rowMark({ needs: false, working: true, unread: true, latest: done }),
      rowMark({ needs: false, working: false, unread: true, latest: done }), rowMark({ needs: false, working: false, unread: false })]).toEqual(['needs', 'working', 'unread', null]);
  });

  it('chatstate_6_opening_an_unread_conversation_sees_it', async () => {
    const app = await on({ conversations: [talk('c1', { unread: true }), talk('c2'), talk('c3', { seen: undefined })] });
    const html = app.side();
    expect(rowOf(html, 'Talk c1')).toContain('aria-label="Unread"');
    expect(rowOf(html, 'Talk c1')).toContain('is-unread');
    expect(rowOf(html, 'Talk c2')).not.toContain('Unread');
    answers['PATCH /v1/conversations/c1'] = talk('c1');
    answers['PATCH /v1/conversations/c3'] = talk('c3');
    app.chats.seeChat('c1');
    app.chats.seeChat('c2');  // seen already: nothing to tell obid
    app.chats.seeChat('c3');  // never looked at since obid kept marks: seen once
    await flush();
    expect(asked.filter((a) => a.method === 'PATCH').map((a) => [a.path, a.body])).toEqual([
      ['/v1/conversations/c1', { seen: true }], ['/v1/conversations/c3', { seen: true }],
    ]);
    expect(rowOf(app.side(), 'Talk c1')).not.toContain('Unread');
  });

  it('chatstate_6_an_obid_without_marks_is_sent_none', async () => {
    const app = await on({ conversations: [talk('c1', { seen: undefined })], capabilities: [] });
    app.chats.seeChat('c1');
    await flush();
    expect(asked.filter((a) => a.method === 'PATCH')).toEqual([]);
    const { default: ConvoMenu } = await import('../ConvoMenu');
    const menu = app.render(<ConvoMenu chat={app.chat('c1')} at={AT} onClose={() => {}} onRename={() => {}} />);
    expect(menu).not.toContain('Mark as unread');
    expect(menu).not.toContain('Archive');
    expect(menu).toContain('Delete conversation…');
  });

  it('chatstate_7_the_menu_marks_it_unread', async () => {
    const app = await on({ conversations: [talk('c1'), talk('c2', { unread: true })] });
    const { default: ConvoMenu } = await import('../ConvoMenu');
    const at0 = AT;
    expect(app.render(<ConvoMenu chat={app.chat('c1')} at={at0} onClose={() => {}} onRename={() => {}} />)).toContain('Mark as unread');
    answers['PATCH /v1/conversations/c1'] = talk('c1', { unread: true });
    clicks['Mark as unread']();
    expect(app.render(<ConvoMenu chat={app.chat('c2')} at={at0} onClose={() => {}} onRename={() => {}} />)).toContain('Mark as read');
    await flush();
    expect(sent('PATCH', '/v1/conversations/c1')).toEqual([{ seen: false }]);
    expect(rowOf(app.side(), 'Talk c1')).toContain('aria-label="Unread"');
  });

  it('chatstate_8_delete_asks_first', async () => {
    const app = await on({ conversations: [talk('c1')] });
    const menu = await import('../ConvoMenu');
    app.render(<menu.default chat={app.chat('c1')} at={AT} onClose={() => {}} onRename={() => {}} />);
    clicks['Delete conversation…']();
    await flush();
    expect(asked.filter((a) => a.method === 'DELETE')).toEqual([]);
    const ask = app.render(<menu.DeleteChatWindow />);
    expect(ask).toContain('Delete “Talk c1”?');
    expect(ask).toContain('Tasks it started keep going');
    answers['DELETE /v1/conversations/c1'] = null;
    clicks.Delete();
    await flush();
    expect(sent('DELETE', '/v1/conversations/c1')).toHaveLength(1);
  });

  it('chatstate_9_archive_moves_it_to_archived_and_restore_brings_it_back', async () => {
    const app = await on({
      conversations: [talk('c1'), talk('c2'), talk('c3', { archived: true })],
      jobs: [job('j3', 'c3', { permission: { id: 'p1', tool: 'Bash', input: {} } })],
      inbox: [{ id: 'i3', kind: 'permission', job: 'j3', title: 'Run this?', text: 'npm test', options: ['Allow', 'Deny'], details: null, created: at, state: 'open', later: null, headline: null, recommended: null }],
    });
    // archived, but something in it needs you: it keeps its row while it does
    expect(rowOf(app.side(), 'Talk c3')).toContain('aria-label="Needs you"');
    const { default: ConvoMenu } = await import('../ConvoMenu');
    app.render(<ConvoMenu chat={app.chat('c1')} at={AT} onClose={() => {}} onRename={() => {}} />);
    answers['PATCH /v1/conversations/c1'] = talk('c1', { archived: true });
    clicks.Archive();
    await flush();
    expect(sent('PATCH', '/v1/conversations/c1')).toEqual([{ archived: true }]);
    const side = app.side();
    expect(rowOf(side, 'Talk c1')).toBe('');
    expect(rowOf(side, 'Talk c2')).not.toBe('');
    expect(side).toMatch(/Archived<span class="sidebar-group-count">2<\/span>/);
    const { default: ArchiveView } = await import('../ArchiveView');
    const onOpenTalk = vi.fn();
    const page = app.render(<ArchiveView leaving={false} onClose={() => {}} onGone={() => {}} onOpenTask={() => {}} onOpenTalk={onOpenTalk} />);
    expect(page).toContain('Talk c1');
    expect(page).not.toContain('Talk c2');
    clicks['archive-row is-link']();
    expect(onOpenTalk).toHaveBeenCalledWith('c1');
    answers['PATCH /v1/conversations/c1'] = talk('c1');
    clicks['Restore Talk c1']();
    await flush();
    expect(sent('PATCH', '/v1/conversations/c1')).toEqual([{ archived: true }, { archived: false }]);
    expect(rowOf(app.side(), 'Talk c1')).not.toBe('');
  });
});
