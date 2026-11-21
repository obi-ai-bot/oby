import { closeSync, constants, mkdtempSync, openSync, writeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, vi } from 'vitest';
import type { LiveMessage } from '../../../shared/live-wire';

// Oby's alerts on this Mac as they're drawn (main/notify.ts, docs/contracts/alerts.md): one launch
// of it with main's one follower of obid's events (main/live.ts) over an app folder, obid answering
// from here, its event stream ours to write to, the window's page a list of what main sent it, and
// Notification Center a list of banners whose buttons a test presses.

export type Banner = {
  title: string; subtitle?: string; body: string; groupId?: string; actions: string[]; closed: boolean;
  click: () => void; press: (text: string) => Promise<void>;
};

export const mac = {
  userData: '',
  // GET /v1/alerts, /v1/inbox, /v1/limit-decisions, /v1/jobs?limit=100
  served: [] as unknown[],
  inbox: [] as unknown[],
  decisions: [] as unknown[],
  jobs: [] as unknown[],
  // obid can't serve GET /v1/alerts (503, its vault unreadable)
  down: false,
  // whether the window is open, and in front
  window: true,
  front: false,
  // whether main's own read of GET /v1/status says obid takes request keys
  receipts: false,
  // how many of the next answers Oby sends obid get no answer at all (status 0)
  lost: 0,
  // the status obid answers what Oby sends (200 unless a test says: 409 for an item settled elsewhere)
  refusing: undefined as number | undefined,
  posted: [] as Banner[],
  badge: undefined as string | undefined,
  // what Oby sent obid that isn't a read, and whether this Mac signed it
  sent: [] as { method: string; path: string; body?: unknown; signed: boolean }[],
  // where a banner's click opened the window
  opened: [] as unknown[],
  // how many times GET /v1/alerts was read since launch, and each read's path with its query
  alertReads: 0,
  alertPaths: [] as string[],
  // the paths of the event streams Oby asked obid for since launch
  streams: [] as string[],
  // what main sent the window's page on obid-live
  page: [] as LiveMessage[],
  // a channel's handler, as the window's own page reaches it
  ask: (_channel: string, ..._args: unknown[]): unknown => undefined,
  event: (_name: string, _data: unknown) => {},
};

// The launch whose copy of notify.ts still reaches mac; every earlier one is retired
let launches = 0;

// A new app folder, nothing served
export function fresh() {
  launches++;
  Object.assign(mac, { userData: mkdtempSync(join(tmpdir(), 'oby-alerts-')), served: [], inbox: [], decisions: [], jobs: [], down: false, window: true, front: false, receipts: false, lost: 0, refusing: undefined });
}

// One alert of obid's list (docs/runner-api.md, GET /v1/alerts)
export const served = (key: string, more: Record<string, unknown> = {}) => ({
  key, what: 'open', title: 'Needs your OK: Fix it', body: 'npm test', category: 'inbox', thread: 'inbox', urgent: true, answers: [], ...more,
});

// One launch of Oby: live.ts and notify.ts fresh, following obid until the first look. The next
// launch or fresh() retires it: neither has a stop, and their timers would otherwise reach the shared mac.
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
  mac.streams = [];
  mac.page = [];
  const handlers: Record<string, (e: unknown, ...args: unknown[]) => unknown> = {};
  mac.ask = (channel, ...args) => handlers[channel]({}, ...args);
  const window = {
    isVisible: () => true, isMinimized: () => false, isFocused: () => mac.front, isDestroyed: () => false,
    webContents: { send: (channel: string, m: LiveMessage) => {
      if (!live()) return;
      if (channel === 'open') mac.opened.push(handlers.opened({}));
      if (channel === 'obid-live') mac.page.push(m);
    } },
  };
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
    ipcMain: { on() {}, handle: (name: string, f: (e: unknown, ...args: unknown[]) => unknown) => { handlers[name] = f; } },
    powerMonitor: { on() {}, getSystemIdleState: () => 'active' },
    shell: {},
  }));
  mac.alertReads = 0;
  mac.alertPaths = [];
  const answer = (method: string, path: string, body: unknown, signed: boolean) => {
    if (method !== 'GET') {
      mac.sent.push({ method, path, body, signed });
      if (mac.lost > 0) { mac.lost--; return { status: 0, text: 'obid closed the connection.' }; }
      // as obid's `unsigned_accept` (SPHERE-17): an Accept of another Oby's review request or
      // handoff that no hand of the owner's signed is refused
      const item = /^\/v1\/inbox\/([^/]+)\/answer$/.exec(path)?.[1];
      const type = (mac.inbox.find((i) => (i as { id?: unknown }).id === item) as { obi?: { type?: unknown } } | undefined)?.obi?.type;
      const said = (body as { answer?: unknown } | undefined)?.answer;
      if (!signed && (type === 'review_request' || type === 'handoff') && typeof said === 'string' && said.startsWith('Accept')) {
        return { status: 403, text: '{"error": "Accepting another Oby\'s work takes your own click in Oby."}' };
      }
      if (mac.refusing) return { status: mac.refusing, text: '{"error": "That item is done already."}' };
      return { status: 200, text: '{}' };
    }
    // GET /v1/alerts with any query (`fyi`, ALERTS-15) serves mac.served
    if (path.split('?')[0] === '/v1/alerts') {
      mac.alertReads++;
      mac.alertPaths.push(path);
      if (mac.down) return { status: 503, text: '{"error": "The vault can’t be read."}' };
      path = '/v1/alerts';
    }
    if (path === '/v1/status') return { status: 200, text: JSON.stringify({ runner: { version: '0.2.4', host: 'mac' }, accounts: [], models: {}, maxRunning: 6, running: 0 }) };
    if (path === '/v1/github/work') return { status: 404, text: '{"error": "Not found."}' };
    const got = ({ '/v1/alerts': mac.served, '/v1/inbox': mac.inbox, '/v1/limit-decisions': mac.decisions, '/v1/jobs?limit=100': mac.jobs } as Record<string, unknown[]>)[path] ?? [];
    return { status: 200, text: JSON.stringify(got) };
  };
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  mac.event = (name, data) => stream.enqueue(new TextEncoder().encode(`event: ${name}\ndata: ${JSON.stringify(data)}\n\n`));
  vi.doMock('../../../main/obid', () => ({
    obid: async (method: string, path: string, body?: unknown) => (live() ? answer(method, path, body, false) : never),
    send: async (_method: string, path: string) => {
      if (!live()) return never;
      mac.streams.push(path);
      return new Response(new ReadableStream<Uint8Array>({ start: (c) => { stream = c; } }));
    },
    takesKeys: async () => mac.receipts,
    chooseProfile: () => false, openedComputer: () => null, onComputer: () => {}, heardComputers: () => {},
    openedProfile: () => null,
    streamEnded: () => {},
    reason: (e: unknown) => ({ status: 0, text: String(e) }),
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
  const main = await import('../../../main/live');
  const shown = () => (mac.window ? window as never : null);
  notify.notifying(() => true, shown, () => window as never);
  main.living(() => true, shown);
  // obid's lists answered, then the look that follows them
  await vi.waitFor(() => expect(mac.alertReads).toBeGreaterThanOrEqual(1));
  await new Promise((done) => setTimeout(done, 0));
  return Object.assign(notify, { main });
}

export const titles = () => mac.posted.map((b) => b.title);
