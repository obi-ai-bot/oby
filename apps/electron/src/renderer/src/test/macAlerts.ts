import { closeSync, constants, mkdtempSync, openSync, writeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, vi } from 'vitest';

// Oby's alerts on this Mac as they're drawn (main/notify.ts, docs/contracts/alerts.md): one launch
// of it over an app folder, obid answering from here, its event stream ours to write to, and
// Notification Center a list of banners whose buttons a test presses.

export type Banner = {
  title: string; subtitle?: string; body: string; groupId?: string; actions: string[]; closed: boolean;
  click: () => void; press: (text: string) => Promise<void>;
};

export const mac = {
  userData: '',
  // GET /v1/alerts, /v1/inbox, /v1/limit-decisions, /v1/jobs
  served: [] as unknown[],
  inbox: [] as unknown[],
  decisions: [] as unknown[],
  jobs: [] as unknown[],
  // obid can't serve GET /v1/alerts (503, its vault unreadable)
  down: false,
  // whether the window is in front
  front: false,
  posted: [] as Banner[],
  badge: undefined as string | undefined,
  // what Oby sent obid that isn't a read, and whether this Mac signed it
  sent: [] as { method: string; path: string; body?: unknown; signed: boolean }[],
  // where a banner's click opened the window
  opened: [] as unknown[],
  // how many times GET /v1/alerts was read since launch
  alertReads: 0,
  event: (_name: string, _data: unknown) => {},
};

// The launch whose copy of notify.ts still reaches mac; every earlier one is retired
let launches = 0;

// A new app folder, nothing served
export function fresh() {
  launches++;
  Object.assign(mac, { userData: mkdtempSync(join(tmpdir(), 'oby-alerts-')), served: [], inbox: [], decisions: [], jobs: [], down: false, front: false });
}

// One alert of obid's list (docs/runner-api.md, GET /v1/alerts)
export const served = (key: string, more: Record<string, unknown> = {}) => ({
  key, what: 'open', title: 'Needs your OK: Fix it', body: 'npm test', category: 'inbox', thread: 'inbox', urgent: true, answers: [], ...more,
});

// One launch of Oby: notify.ts fresh, following obid until its first look. The next launch or
// fresh() retires it: notify.ts has no stop, and its timers would otherwise reach the shared mac.
// Retired, its mocks post, badge, send and write nothing, and its reads never answer
export async function launch() {
  const me = ++launches;
  const live = () => me === launches;
  const never = new Promise<never>(() => {});
  const userData = mac.userData;
  vi.resetModules();
  mac.posted = [];
  mac.badge = undefined;
  mac.sent = [];
  mac.opened = [];
  const handlers: Record<string, (e: unknown) => unknown> = {};
  const window = { isVisible: () => true, isMinimized: () => false, isFocused: () => mac.front, webContents: { send: () => { if (live()) mac.opened.push(handlers.opened({})); } } };
  vi.doMock('electron', () => ({
    app: { getPath: () => userData, isPackaged: false, dock: { setBadge: (b: string) => { if (live()) mac.badge = b; } } },
    Notification: class {
      static isSupported = () => true;
      private on_: Record<string, (...a: never[]) => unknown> = {};
      private banner: Banner;
      constructor(o: { title: string; subtitle?: string; body: string; groupId?: string; actions: { text: string }[] }) {
        const actions = o.actions.map((a) => a.text);
        this.banner = {
          title: o.title, subtitle: o.subtitle, body: o.body, groupId: o.groupId, actions, closed: false,
          click: () => { this.on_.click?.(); },
          press: async (text) => { await (this.on_.action as (e: { actionIndex: number }) => Promise<void>)({ actionIndex: actions.indexOf(text) }); },
        };
      }
      on(name: string, f: (...a: never[]) => unknown) { this.on_[name] = f; }
      close() { this.banner.closed = true; }
      show() { if (live()) mac.posted.push(this.banner); }
    },
    ipcMain: { on() {}, handle: (name: string, f: (e: unknown) => unknown) => { handlers[name] = f; } },
    powerMonitor: { on() {}, getSystemIdleState: () => 'active' },
    shell: {},
  }));
  let lists = 0;
  mac.alertReads = 0;
  const answer = (method: string, path: string, body: unknown, signed: boolean) => {
    if (method !== 'GET') {
      mac.sent.push({ method, path, body, signed });
      return { status: 200, text: '{}' };
    }
    lists++;
    if (path === '/v1/alerts') mac.alertReads++;
    if (path === '/v1/alerts' && mac.down) return { status: 503, text: '{"error": "The vault can’t be read."}' };
    const got = ({ '/v1/alerts': mac.served, '/v1/inbox': mac.inbox, '/v1/limit-decisions': mac.decisions, '/v1/jobs': mac.jobs } as Record<string, unknown[]>)[path] ?? [];
    return { status: 200, text: JSON.stringify(got) };
  };
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  mac.event = (name, data) => stream.enqueue(new TextEncoder().encode(`event: ${name}\ndata: ${JSON.stringify(data)}\n\n`));
  vi.doMock('../../../main/obid', () => ({
    obid: async (method: string, path: string, body?: unknown) => (live() ? answer(method, path, body, false) : never),
    send: async () => (live() ? new Response(new ReadableStream<Uint8Array>({ start: (c) => { stream = c; } })) : never),
  }));
  vi.doMock('../../../main/device', () => ({
    answerLimit: async (...a: unknown[]) => { if (!live()) return never; mac.sent.push({ method: 'POST', path: 'answerLimit', body: a, signed: true }); return { status: 200, text: '{}' }; },
    notificationWords: async (method: string, path: string, body: unknown) => (live() ? answer(method, path, body, true) : never),
    // as device.ts's: never through a link
    writeOwn: (path: string, data: string, flags: number) => {
      if (!live()) return;
      const fd = openSync(path, constants.O_WRONLY | constants.O_CREAT | constants.O_NOFOLLOW | flags, 0o600);
      try { writeSync(fd, data); } finally { closeSync(fd); }
    },
  }));
  const notify = await import('../../../main/notify');
  notify.notifying(() => true, () => window as never, () => window as never);
  // obid's lists answered, then the look that follows them
  await vi.waitFor(() => expect(lists).toBeGreaterThanOrEqual(4));
  await new Promise((done) => setTimeout(done, 0));
  return notify;
}

export const titles = () => mac.posted.map((b) => b.title);
