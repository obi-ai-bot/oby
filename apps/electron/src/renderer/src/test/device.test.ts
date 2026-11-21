import { createPublicKey, verify } from 'node:crypto';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { confirming, deviceKey, fillSecret, notificationWords, ownersWords, typeInPages } from '../../../main/device';
import { input, picked, takeInput } from '../../../main/words';

// Main's Confirm as the window reaches it (docs/contracts/peer-thread.md, PEER-17): what it signs,
// through its real handler, with obid and Electron's own parts stood in for.
const the = vi.hoisted(() => ({
  handlers: new Map<string, (...args: unknown[]) => Promise<{ status: number; text: string }>>(),
  calls: [] as { method: string; path: string; body?: { approval?: unknown; clientRequestId?: unknown }; words?: string }[],
  item: { id: 'confirm-1', kind: 'confirm', title: 'Can I?', state: 'open', confirm: { action: 'share_status' } },
  // main's own read of whether obid takes request keys, and how many of the next messages obid
  // doesn't answer at all (status 0)
  receipts: false,
  lost: 0,
  // obid's answer to main's own read of its projects (WORDS-18)
  projects: { status: 200, text: '[]' },
  // Undo this round (CHANGES-14): obid's sentence, and the button main's dialog answers (0: Undo)
  undo: 'oby/fix-1 goes back to abc1234, as it was before this round. Nothing is pushed.',
  dialog: 1,
  asked: [] as { message: string; detail: string }[],
  // a finishing action (WORKSPACE-4): obid's sentence for it, read by main itself
  finish: 'Commit 1 change in /Users/me/a/api on main: api/todo.md (new).',
}));
vi.mock('electron', () => ({
  app: { getPath: () => mkdtempSync(join(tmpdir(), 'oby-device-')) },
  BrowserWindow: { fromWebContents: () => null },
  dialog: { showMessageBox: async (asked: { message: string; detail: string }) => { the.asked.push(asked); return { response: the.dialog }; } },
  ipcMain: { handle: (name: string, f: (...args: unknown[]) => Promise<{ status: number; text: string }>) => the.handlers.set(name, f) },
  safeStorage: { isEncryptionAvailable: () => true, encryptString: (s: string) => Buffer.from(s), decryptString: (b: Buffer) => b.toString() },
}));
vi.mock('../../../main/obid', () => ({
  HOME: '/nowhere',
  obid: async (method: string, path: string, body?: { approval?: unknown }, words?: string) => {
    the.calls.push({ method, path, body, words });
    if (path === '/v1/inbox') return { status: 200, text: JSON.stringify([the.item]) };
    if (path === '/v1/devices') return { status: 200, text: JSON.stringify({ id: 'd1' }) };
    if (path === '/v1/projects') return the.projects;
    if (path === '/v1/browser') return { status: 200, text: (the as { browserList?: string }).browserList ?? '{"tabs":[]}' };
    if (path === '/v1/jobs/j1/undo' && method === 'GET') return { status: 200, text: JSON.stringify({ action: the.undo }) };
    if (path.startsWith('/v1/conversations/c1/workspace?') && method === 'GET') return { status: 200, text: JSON.stringify({ action: the.finish }) };
    if (path.endsWith('/messages') && the.lost > 0) { the.lost--; return { status: 0, text: 'obid closed the connection.' }; }
    return { status: 200, text: '{}' };
  },
  takesKeys: async () => the.receipts,
  chooseProfile: () => false, openedComputer: () => null, onComputer: () => {}, heardComputers: () => {},
  openedProfile: () => null,
}));

describe('obid-confirm', () => {
  it('words_15_notification_signs_without_window_input_and_preserves_its_request', async () => {
    for (const decision of ['allow', 'deny']) {
      const path = '/v1/jobs/j1/permission', body = { id: 'p1', decision };
      await notificationWords('POST', path, body);
      expect(the.calls.at(-1)).toMatchObject({ method: 'POST', path, body, words: expect.stringMatching(/^d1 /) });
    }
    for (const answer of ['Allow', 'Deny']) {
      const path = '/v1/inbox/i1/answer', body = { answer };
      await notificationWords('POST', path, body);
      expect(the.calls.at(-1)).toMatchObject({ method: 'POST', path, body, words: expect.stringMatching(/^d1 /) });
    }
    expect(takeInput()).toBe(false);
  });
  confirming(() => true);
  const confirm = (clicked: unknown) => the.handlers.get('obid-confirm')!({}, the.item.id, clicked);
  const answered = () => the.calls.filter((c) => c.path.endsWith('/answer'));
  beforeEach(() => { the.calls.length = 0; the.item.confirm.action = 'share_status'; while (takeInput()) { /* other tests' */ } });

  it('words_11_main_signs_sharing_only_on_a_press_of_confirm_and_an_input_it_saw', async () => {
    expect((await confirm(true)).status).toBe(403);
    input();
    expect((await confirm(false)).status).toBe(403);
    expect(answered()).toEqual([]);
    expect((await confirm(true)).status).toBe(200);
    expect(answered()[0].body?.approval).toMatchObject({ device: 'd1', action: 'Can I?' });
  });
  it('words_11: the room\'s card takes the same press', async () => {
    the.item.confirm.action = 'share_room';
    expect((await confirm(false)).status).toBe(403);
    input();
    expect((await confirm(true)).status).toBe(200);
  });
  it('peer_33_a_confirm_that_answers_for_the_owner_takes_their_press', async () => {
    for (const action of ['answer_question', 'decline_remember']) {
      while (takeInput()) { /* inputs from other tests */ }
      the.calls.length = 0;
      the.item.confirm.action = action;
      // a script's call, with no input of the owner's: nothing reaches obid
      expect((await confirm(undefined)).status, action).toBe(403);
      expect((await confirm(true)).status, action).toBe(403);
      input();
      expect((await confirm(false)).status, action).toBe(403);
      expect(answered(), action).toEqual([]);
      input();
      expect((await confirm(true)).status, action).toBe(200);
      expect(answered(), action).toHaveLength(1);
    }
  });
  it('another Confirm goes as it did', async () => {
    the.item.confirm.action = 'delete_job';
    expect((await confirm(undefined)).status).toBe(200);
    expect(answered()).toHaveLength(1);
    expect(answered()[0].body).not.toHaveProperty('clientRequestId');
  });
  it('receipts_16_a_confirm_carries_mains_own_key_once_obid_takes_keys', async () => {
    the.item.confirm.action = 'delete_job';
    the.receipts = true;
    try {
      await confirm(undefined);
      await confirm(undefined);
    } finally { the.receipts = false; }
    const [a, b] = answered().map((c) => c.body?.clientRequestId);
    expect(a).toMatch(/^[0-9a-f-]{36}$/);
    expect(b).toMatch(/^[0-9a-f-]{36}$/);
    expect(a).not.toBe(b);
  });
});

// A keyed send obid didn't answer, sent again (docs/contracts/signed-words.md, WORDS-16)
describe('main signs again only the request obid didn’t answer', () => {
  const path = '/v1/jobs/j1/messages';
  const body = (key: string, text = 'and this') => ({ text, clientRequestId: key });
  const last = () => the.calls.at(-1)!;
  const signedAt = (words?: string) => words?.split(' ')[1];
  const later = (ms: number) => vi.setSystemTime(Date.now() + ms);
  // the page's call: owners, the preload held its words
  const send = async (b: unknown, owners = false, p = path) => { await ownersWords('POST', p, b, owners); return last().words; };
  // signed on an input of the owner's, and obid didn't answer
  const lost = async (b: unknown) => {
    input();
    the.lost = 1;
    const words = await send(b, true);
    expect(words).toMatch(/^d1 /);
    return words;
  };
  let clock = Date.parse('2026-11-14T10:00:00Z');
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(clock += 60_000);
    while (takeInput()) { /* other tests' */ }
    the.calls.length = 0;
    the.lost = 0;
    the.receipts = true;
  });
  afterEach(() => { vi.useRealTimers(); the.receipts = false; the.lost = 0; });

  it('words_16_main_re_signs_only_the_request_obid_did_not_answer', async () => {
    // the same request again, no input: signed again with a fresh time, and once obid answers, no more
    const first = await lost(body('k1'));
    later(1000);
    the.lost = 1;
    const again = await send(body('k1'));
    expect(again).toMatch(/^d1 /);
    expect(signedAt(again)).not.toBe(signedAt(first));
    later(1000);
    expect(await send(body('k1'))).toMatch(/^d1 /);
    later(1000);
    expect(await send(body('k1'))).toBeUndefined();
    expect(takeInput()).toBe(false);
  });
  it('words_16: anything but the very same request goes unsigned', async () => {
    await lost(body('k2'));
    later(500);
    for (const [other, p] of [
      [body('k3'), path], [body('k2', 'and that'), path], [{ ...body('k2'), conversation: true }, path], [body('k2'), '/v1/jobs/j2/messages'],
    ] as const) expect(await send(other, false, p), JSON.stringify(other)).toBeUndefined();
    // still the very same one
    expect(await send(body('k2'))).toMatch(/^d1 /);
  });
  it('words_16: not past 10 s, nor a third time', async () => {
    await lost(body('k4'));
    later(10_000);
    expect(await send(body('k4'))).toBeUndefined();
    await lost(body('k5'));
    the.lost = 2;
    later(1000);
    expect(await send(body('k5'))).toMatch(/^d1 /);
    later(1000);
    expect(await send(body('k5'))).toMatch(/^d1 /);
    later(1000);
    expect(await send(body('k5'))).toBeUndefined();
  });
  it('words_16: not one main never signed, one with no key, nor while obid doesn’t take keys', async () => {
    the.lost = 1;
    expect(await send(body('k6'))).toBeUndefined();
    expect(await send(body('k6'))).toBeUndefined();
    await lost({ text: 'and this' });
    expect(await send({ text: 'and this' })).toBeUndefined();
    await lost(body('k7'));
    the.receipts = false;
    expect(await send(body('k7'))).toBeUndefined();
  });
  it('words_16: at most twice, however many identical resends arrive at once', async () => {
    await lost(body('k8'));
    the.calls.length = 0;
    later(1000);
    await Promise.all([1, 2, 3, 4, 5].map(() => ownersWords('POST', path, body('k8'), false)));
    const sent = the.calls.filter((c) => c.path === path);
    expect(sent).toHaveLength(5);
    expect(sent.filter((c) => c.words?.startsWith('d1 '))).toHaveLength(2);
  });
});

// Words that start a conversation in a project (docs/contracts/signed-words.md, WORDS-18): main reads
// the project's folders from obid itself, and signs only when each was picked in its dialog this run
describe('main reads a project’s folders itself', () => {
  const plan = { said: 'Fix the login', new: true, project: 'pr1' };
  const listing = (projects: unknown) => { the.projects = { status: 200, text: JSON.stringify(projects) }; };
  const signedOnInput = async (body: unknown) => {
    while (takeInput()) { /* other tests' */ }
    the.calls.length = 0;
    input();
    await ownersWords('POST', '/v1/plans', body);
    return the.calls.find((c) => c.path === '/v1/plans')?.words;
  };
  afterEach(() => { the.projects = { status: 200, text: '[]' }; });

  it('words_18_a_project_signs_only_with_folders_picked_this_run', async () => {
    picked(['/Users/me/picked']);
    // no folders, or only folders main's dialog returned: signed, after main's own read
    listing([{ id: 'pr1', name: 'App', folders: [], repos: [] }]);
    expect(await signedOnInput(plan)).toMatch(/^d1 /);
    expect(the.calls.map((c) => `${c.method} ${c.path}`)).toContain('GET /v1/projects');
    listing([{ id: 'pr0', name: 'Other', folders: ['/Users/me/never-picked'], repos: [] }, { id: 'pr1', name: 'App', folders: ['/Users/me/picked'], repos: [] }]);
    expect(await signedOnInput(plan)).toMatch(/^d1 /);
    // one folder not picked, an id obid doesn't list, a read that fails or doesn't parse: unsigned
    listing([{ id: 'pr1', name: 'App', folders: ['/Users/me/picked', '/Users/me/never-picked'], repos: [] }]);
    expect(await signedOnInput(plan)).toBeUndefined();
    listing([{ id: 'pr2', name: 'App', folders: [], repos: [] }]);
    expect(await signedOnInput(plan)).toBeUndefined();
    the.projects = { status: 500, text: '{"error":"projects.json doesn’t read."}' };
    expect(await signedOnInput(plan)).toBeUndefined();
    the.projects = { status: 200, text: 'not json' };
    expect(await signedOnInput(plan)).toBeUndefined();
    the.projects = { status: 200, text: '{"projects":[]}' };
    expect(await signedOnInput(plan)).toBeUndefined();
    // the page's folders never stand for the project's
    listing([{ id: 'pr1', name: 'App', folders: ['/Users/me/never-picked'], repos: [] }]);
    expect(await signedOnInput({ ...plan, folders: ['/Users/me/picked'] })).toBeUndefined();
    // words with no project: no read of main's
    expect(await signedOnInput({ said: 'Fix the login', new: true })).toMatch(/^d1 /);
    expect(the.calls.some((c) => c.path === '/v1/projects')).toBe(false);
  });
});

// Undo this round (docs/contracts/task-changes.md, CHANGES-14): main signs `oby-undo` only on Undo in
// its own dialog, after an input it saw, for the sentence it read from obid itself.
describe('obid-undo', () => {
  confirming(() => true);
  const undo = (action: string) => the.handlers.get('obid-undo')!({ sender: {} }, 'j1', action);
  const posted = () => the.calls.filter((c) => c.method === 'POST' && c.path === '/v1/jobs/j1/undo');
  beforeEach(() => { the.calls.length = 0; the.asked.length = 0; the.dialog = 1; while (takeInput()) { /* other tests' */ } });

  it('changes_14_main_signs_undo_only_on_undo_in_its_own_dialog', async () => {
    // a script's call, with no input of the owner's: no dialog, nothing reaches obid
    expect((await undo(the.undo)).status).toBe(403);
    expect(the.asked).toEqual([]);
    // the page showed other words than obid's: refused before any dialog
    input();
    expect((await undo('Undo it all.')).status).toBe(409);
    expect(the.asked).toEqual([]);
    // Not now in main's dialog: nothing signed
    input();
    expect((await undo(the.undo)).status).toBe(409);
    expect(the.asked.at(-1)).toMatchObject({ message: 'Undo this round?', detail: the.undo });
    expect(posted()).toEqual([]);
    // Undo there: signed obid's sentence
    the.dialog = 0;
    input();
    expect((await undo(the.undo)).status).toBe(200);
    expect(posted()).toHaveLength(1);
    expect(posted()[0].body).toMatchObject({ device: 'd1', action: the.undo, signedAt: expect.any(String), signature: expect.any(String) });
  });
});

// A finishing action in a conversation's folder (docs/contracts/folders.md, WORKSPACE-4): main signs
// `oby-finish` only on its own dialog's button, after an input it saw, for the sentence it read from
// obid itself, which names the folder by its full path.
describe('obid-finish', () => {
  confirming(() => true);
  const folder = '/Users/me/a/api';
  const finish = (action: string) => the.handlers.get('obid-finish')!({ sender: {} }, 'c1', folder, action);
  const posted = () => the.calls.filter((c) => c.method === 'POST' && c.path === '/v1/conversations/c1/workspace');
  beforeEach(() => { the.calls.length = 0; the.asked.length = 0; the.dialog = 1; while (takeInput()) { /* other tests' */ } });

  it('workspace_4_main_signs_a_finishing_action_only_on_its_own_dialogs_button', async () => {
    // a script's call, with no input of the owner's: no read, no dialog, nothing reaches obid
    expect((await finish('commit')).status).toBe(403);
    expect(the.calls).toEqual([]);
    expect(the.asked).toEqual([]);
    // Not now in main's dialog: nothing signed; the dialog shows obid's own sentence
    input();
    expect((await finish('commit')).status).toBe(409);
    const read = the.calls.find((c) => c.method === 'GET');
    expect(read?.path).toBe(`/v1/conversations/c1/workspace?${new URLSearchParams({ folder, finish: 'commit' }).toString()}`);
    expect(the.asked.at(-1)).toMatchObject({ message: 'Commit?', detail: the.finish });
    expect(posted()).toEqual([]);
    // Commit there: signed obid's sentence, with this device's key, once
    the.dialog = 0;
    input();
    expect((await finish('commit')).status).toBe(200);
    expect(posted()).toHaveLength(1);
    const body = posted()[0].body as unknown as { folder: string; finish: string; approval: { device: string; action: string; signedAt: string; signature: string } };
    expect(body).toMatchObject({ folder, finish: 'commit', approval: { device: 'd1', action: the.finish } });
    const message = `oby-finish\nc1\n${the.finish}\n${body.approval.signedAt}`;
    expect(verify('sha256', Buffer.from(message), createPublicKey(deviceKey()), Buffer.from(body.approval.signature, 'base64'))).toBe(true);
    // the click is used: a second call with no new input signs nothing
    expect((await finish('commit')).status).toBe(403);
    expect(posted()).toHaveLength(1);
  });
});

// A password typed in Oby's card, typed by main into the tab's own webview on the owner's press of
// Fill in (BROWSERAPP-12): obid hears only that the ask is answered, never the value
describe('BROWSERAPP-12: main types the password into the tab\'s own page on the owner\'s own Fill in, never through obid', () => {
  const SECRET = 'device-test-secret-4417';
  it('browserapp_12_fill_takes_the_owners_click_types_into_the_page_and_tells_obid_only_the_ask_answered', async () => {
    const errors = vi.spyOn(console, 'error');
    const typed: [string, string, string][] = [];
    typeInPages(async (tab, field, value) => { typed.push([tab, field, value]); return null; });
    // no press of Fill in, or no input main saw: nothing is typed, nothing reaches obid
    let before = the.calls.length;
    expect(await fillSecret('t1', '#pw', SECRET, 'a1', false)).toBe('Filling in takes your own click.');
    takeInput(); takeInput();
    expect(await fillSecret('t1', '#pw', SECRET, 'a1', true)).toBe('Filling in takes your own click.');
    expect([typed, the.calls.length - before]).toEqual([[], 0]);
    // with both: typed into the page, then only the ask's answer to obid, signed
    input();
    before = the.calls.length;
    expect(await fillSecret('t1', '#pw', SECRET, 'a1', true)).toBeNull();
    expect(typed).toEqual([['t1', '#pw', SECRET]]);
    const sent = the.calls.slice(before);
    expect(sent.filter((c) => c.method === 'POST').map((c) => [c.path, c.body])).toEqual([['/v1/browser/asks/a1/answer', {}]]);
    expect(sent.every((c) => c.method !== 'POST' || /^d1 \S+ \S+$/.test(c.words ?? ''))).toBe(true);
    // the value goes to obid in no request at all
    expect(JSON.stringify(the.calls)).not.toContain(SECRET);
    expect(errors.mock.calls.flat().join(' ')).not.toContain(SECRET);
  });

  it('browserapp_12_a_page_that_cant_be_typed_into_answers_no_ask', async () => {
    typeInPages(async () => 'That field isn’t on the page.');
    input();
    const before = the.calls.length;
    expect(await fillSecret('t1', '#nope', SECRET, 'a1', true)).toBe('That field isn’t on the page.');
    expect(the.calls.slice(before).filter((c) => c.method === 'POST')).toEqual([]);
  });
});
