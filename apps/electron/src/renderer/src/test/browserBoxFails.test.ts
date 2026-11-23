import { EventEmitter } from 'node:events';
import { describe, expect, it, vi } from 'vitest';
import type { BrowserWindow } from 'electron';
import { relayCommand, type BoxCommand, type RelayHost } from '../../../main/browserRelay';

// A box session whose clipboard guard can't be set up (its preload file not written: a full disk, a
// folder in its place) fails closed (docs/contracts/browser-app.md, BROWSERAPP-36): its requests are
// cancelled, never let through, and its door is still set. Apart from browserBox.test.ts, as the
// guard's file is written once a run.

type Handler = (details: unknown, callback: (r: { cancel?: boolean }) => void) => void;
const the = vi.hoisted(() => ({ sessions: new Map<string, ReturnType<typeof fakeSession>>() }));
function fakeSession() {
  const s = {
    requests: [] as Handler[], proxies: [] as unknown[], preloads: [] as unknown[],
    setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, on() {},
    registerPreloadScript: (p: unknown) => { s.preloads.push(p); return ''; },
    webRequest: { onBeforeRequest: (h: Handler) => { s.requests.push(h); } },
    setProxy: async (config: unknown) => { s.proxies.push(config); },
  };
  return s;
}
vi.mock('node:fs', async (real) => ({ ...(await real<typeof import('node:fs')>()), writeFileSync: () => { throw new Error('ENOSPC: no space left on device'); } }));
vi.mock('electron', () => ({
  app: { getPath: () => '/nowhere' },
  session: { fromPartition: (p: string) => { if (!the.sessions.has(p)) the.sessions.set(p, fakeSession()); return the.sessions.get(p)!; } },
  webContents: { fromId: () => null },
}));
const { boxDoor, hosting } = await import('../../../main/browserGuest');

function request(s: ReturnType<typeof fakeSession>) {
  const answer: { got: { cancel?: boolean } | 'held' } = { got: 'held' };
  for (const h of s.requests) h({ url: 'https://example.com/' }, (r) => { answer.got = r; });
  return answer;
}

describe('BROWSERAPP-36: a box whose clipboard guard can\'t be set up reaches nothing', () => {
  it('browserapp_36: its guard\'s file not written, every request of the box is cancelled, before and after its door is set; the door is still set', async () => {
    const win = { webContents: new EventEmitter() };
    const host = { attach: vi.fn(), detach: vi.fn(), forget: vi.fn(), event: vi.fn(), shown: vi.fn(), loadFailed: vi.fn() };
    hosting(win as unknown as BrowserWindow, host as unknown as RelayHost, () => undefined, () => {}, { computer: () => null, send: vi.fn() });
    const e = { preventDefault: vi.fn() };
    win.webContents.emit('will-attach-webview', e, {}, { partition: 'oby-box-f1', src: 'https://example.com/' });
    const s = the.sessions.get('oby-box-f1')!;
    expect(s.preloads).toEqual([]);
    expect(s.requests).toHaveLength(1);
    expect(request(s).got).toEqual({ cancel: true });
    await boxDoor(relayCommand('box', JSON.stringify({ threadId: 'f1', partition: 'oby-box-f1', proxy: `http://oby:${'a'.repeat(64)}@127.0.0.1:50150` })) as BoxCommand, null);
    expect(s.proxies).toEqual([{ mode: 'fixed_servers', proxyRules: '127.0.0.1:50150', proxyBypassRules: '<-loopback>' }]);
    expect(request(s).got).toEqual({ cancel: true });
    // a box whose door came first (obid's `box` at launch) fails the same way
    await boxDoor(relayCommand('box', JSON.stringify({ threadId: 'f2', partition: 'oby-box-f2', proxy: `http://oby:${'b'.repeat(64)}@127.0.0.1:50151` })) as BoxCommand, null);
    const first = the.sessions.get('oby-box-f2')!;
    expect(first.proxies).toHaveLength(1);
    expect(request(first).got).toEqual({ cancel: true });
  });
});
