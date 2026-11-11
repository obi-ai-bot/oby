import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { InboxItem } from '../../../shared/obid-wire';

// The Mac keeps what it said across launches (docs/contracts/alerts.md, ALERTS-2): main/notify.ts
// launched, quit and launched again over the same app folder, with obid's answers mocked.

let userData = '';
let open: InboxItem[] = [];
let posted: string[] = [];
let badge: string | undefined;

const permission: InboxItem = {
  id: 'p1', kind: 'permission', job: '', title: 'Run npm test?', text: 'npm test', options: ['Allow', 'Deny'], state: 'open',
} as InboxItem;

// One launch of Oby: notify.ts fresh, following obid until its first look
async function launch() {
  vi.resetModules();
  posted = [];
  badge = undefined;
  vi.doMock('electron', () => ({
    app: { getPath: () => userData, isPackaged: false, dock: { setBadge: (b: string) => { badge = b; } } },
    Notification: class {
      static isSupported = () => true;
      constructor(private o: { title: string }) {}
      on() {}
      close() {}
      show() { posted.push(this.o.title); }
    },
    ipcMain: { on() {}, handle() {} },
    powerMonitor: { on() {} },
    shell: {},
  }));
  let asked = 0;
  const answer = (path: string) => { asked++; return { status: 200, text: JSON.stringify(path === '/v1/inbox' ? open : []) }; };
  // the event stream stays open and quiet: the first look is the one after obid's lists
  vi.doMock('../../../main/obid', () => ({ obid: async (_: string, path: string) => answer(path), send: async () => new Response(new ReadableStream()) }));
  vi.doMock('../../../main/device', () => ({ answerLimit: vi.fn(), notificationWords: vi.fn(), writeOwn: (p: string, d: string) => writeFileSync(p, d) }));
  const { notifying } = await import('../../../main/notify');
  notifying(() => true, () => null, () => { throw new Error('no window'); });
  // obid's four lists answered, then the look that follows them
  await vi.waitFor(() => expect(asked).toBe(4));
  await new Promise((done) => setTimeout(done, 0));
}

beforeEach(() => {
  userData = mkdtempSync(join(tmpdir(), 'oby-alerts-'));
  open = [];
});

describe('ALERTS-2', () => {
  it('alerts_2_what_waited_while_closed_alerts_once_at_launch', async () => {
    await launch();
    expect(posted).toEqual([]);
    // quit; a permission comes while Oby is closed: the next launch alerts it, and counts it
    open = [permission];
    await launch();
    expect(posted).toEqual(['Run npm test?']);
    expect(badge).toBe('1');
    // what it said is kept for the next launch
    expect(JSON.parse(readFileSync(join(userData, 'alerts-said.json'), 'utf8'))).toEqual({ 'item:p1': { what: 'open', needs: true } });
    // a file that can't be parsed keeps nothing quiet: what waits alerts
    writeFileSync(join(userData, 'alerts-said.json'), '{not json');
    await launch();
    expect(posted).toEqual(['Run npm test?']);
  });

  it('alerts_2_a_restart_repeats_nothing', async () => {
    // nothing kept (the first launch with this): what's there isn't news, as before; the badge counts it
    open = [permission];
    await launch();
    expect(posted).toEqual([]);
    expect(badge).toBe('1');
    // quit and relaunch with nothing new: nothing said again
    await launch();
    expect(posted).toEqual([]);
    expect(badge).toBe('1');
  });
});
