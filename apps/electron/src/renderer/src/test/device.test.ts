import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { confirming } from '../../../main/device';
import { input, takeInput } from '../../../main/words';

// Main's Confirm as the window reaches it (docs/contracts/peer-thread.md, PEER-17): what it signs,
// through its real handler, with obid and Electron's own parts stood in for.
const the = vi.hoisted(() => ({
  handlers: new Map<string, (...args: unknown[]) => Promise<{ status: number; text: string }>>(),
  calls: [] as { method: string; path: string; body?: { approval?: unknown } }[],
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
  obid: async (method: string, path: string, body?: { approval?: unknown }) => {
    the.calls.push({ method, path, body });
    if (path === '/v1/inbox') return { status: 200, text: JSON.stringify([the.item]) };
    if (path === '/v1/devices') return { status: 200, text: JSON.stringify({ id: 'd1' }) };
    return { status: 200, text: '{}' };
  },
}));

describe('obid-confirm', () => {
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
