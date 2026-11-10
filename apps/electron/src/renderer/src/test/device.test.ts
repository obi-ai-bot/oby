import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { confirming, notificationWords, ownersWords, deviceKey } from '../../../main/device';
import { input, keyInput, takeInput } from '../../../main/words';
import { createHash, createPublicKey, verify } from 'node:crypto';

// Main's Confirm as the window reaches it (docs/contracts/peer-thread.md, PEER-17): what it signs,
// through its real handler, with obid and Electron's own parts stood in for.
const the = vi.hoisted(() => ({
  handlers: new Map<string, (...args: unknown[]) => Promise<{ status: number; text: string }>>(),
  calls: [] as { method: string; path: string; body?: { approval?: unknown }; words?: string }[],
  item: { id: 'confirm-1', kind: 'confirm', title: 'Can I?', state: 'open', confirm: { action: 'share_status' } },
}));
vi.mock('electron', () => ({
  app: { getPath: () => mkdtempSync(join(tmpdir(), 'oby-device-')) },
  BrowserWindow: {}, dialog: {},
  ipcMain: { handle: (name: string, f: (...args: unknown[]) => Promise<{ status: number; text: string }>) => the.handlers.set(name, f) },
  safeStorage: { isEncryptionAvailable: () => true, encryptString: (s: string) => Buffer.from(s), decryptString: (b: Buffer) => b.toString() },
}));
vi.mock('../../../main/obid', () => ({
  HOME: '/nowhere',
  obid: async (method: string, path: string, body?: { approval?: unknown }, words?: string) => {
    the.calls.push({ method, path, body, words });
    if (path === '/v1/inbox') return { status: 200, text: JSON.stringify([the.item]) };
    if (path === '/v1/devices') return { status: 200, text: JSON.stringify({ id: 'd1' }) };
    return { status: 200, text: '{}' };
  },
}));

describe('obid-confirm', () => {
  it('words_14_default_mode_navigation_signs_in_main_once_and_only_within_two_seconds', async () => {
    vi.useFakeTimers();
    try {
      const path = '/v1/settings/permissions', body = { mode: 'bypass' };
      const key = (key = 'ArrowDown', type = 'keyDown', isAutoRepeat = false) => keyInput({ key, code: key, type, isAutoRepeat });
      // Focus and an earlier click are no longer fresh when the select changes.
      input();
      vi.advanceTimersByTime(2000);
      await ownersWords('PUT', path, body);
      expect(the.calls.at(-1)?.words).toBeUndefined();
      key();
      // A navigation token cannot sign ordinary words, a task's mode, extra keys or Confirm.
      expect(takeInput()).toBe(false);
      for (const [method, target, sent] of [
        ['POST', '/v1/jobs/j1/messages', { text: 'made up' }],
        ['POST', '/v1/jobs/j1/mode', body],
        ['PUT', path, { ...body, extra: true }],
      ] as const) {
        await ownersWords(method, target, sent);
        expect(the.calls.at(-1)?.words).toBeUndefined();
      }
      await ownersWords('PUT', path, body);
      const [, at, signature] = the.calls.at(-1)!.words!.split(' ');
      const digest = createHash('sha256').update(JSON.stringify(body)).digest('hex');
      expect(verify('sha256', Buffer.from(`oby-words\nPUT\n${path}\n${digest}\n${at}`), createPublicKey(deviceKey()), Buffer.from(signature, 'base64'))).toBe(true);
      await ownersWords('PUT', path, body);
      expect(the.calls.at(-1)?.words).toBeUndefined();
      key();
      vi.advanceTimersByTime(2000);
      await ownersWords('PUT', path, body);
      expect(the.calls.at(-1)?.words).toBeUndefined();
      key('ArrowDown', 'keyUp');
      key('ArrowDown', 'keyDown', true);
      key('Tab');
      await ownersWords('PUT', path, body);
      expect(the.calls.at(-1)?.words).toBeUndefined();
      // Enter grants just its existing token, with no second select token.
      key('Enter');
      await ownersWords('PUT', path, body);
      expect(the.calls.at(-1)?.words).toMatch(/^d1 /);
      await ownersWords('PUT', path, body);
      expect(the.calls.at(-1)?.words).toBeUndefined();
    } finally { vi.useRealTimers(); }
  });
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
  });
});
