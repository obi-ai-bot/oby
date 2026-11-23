import { request } from 'node:http';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RELAY_CSP, designHost, designPath, startRelay, type Designs } from '../../../main/sandpackRelay';
import { allowed, canvasStatus } from '../../../main/previewRules';

// docs/contracts/artifacts-app.md, ARTIFACTSAPP-29 and 30: a Claude Design design drawn in the pane by
// obid's canvas host and Oby's runtime (ARTIFACTS-13 to 15), on its version's own origin on main's
// relay. Main's previewing() runs for real with Electron's views and obid stood in for; the relay runs
// for real on loopback with obid's answers stood in for.

type Handler = (e: unknown, ...args: unknown[]) => unknown;
const the = vi.hoisted(() => ({
  handlers: new Map<string, Handler>(),
  views: [] as { loads: string[]; files: number; visible: boolean[]; reloads: number; webContents: { id: number } }[],
  designs: null as Designs | null,
  reads: [] as string[],
  // what the canvas host's `window.__OBY_CANVAS` holds; hung: its page never answers
  canvas: { ready: true, errors: [] } as unknown,
  hung: false,
  // a design's load held until let go
  held: null as Promise<void> | null,
}));
vi.mock('electron', () => {
  class WebContentsView {
    loads: string[] = [];
    files = 0;
    visible: boolean[] = [];
    reloads = 0;
    webContents = {
      id: the.views.length + 1,
      setWindowOpenHandler() {}, on() {},
      loadURL: async (u: string) => { this.loads.push(u); await the.held; },
      loadFile: async () => { this.files++; },
      reload: () => { this.reloads++; },
      executeJavaScript: (js: string): Promise<unknown> => {
        if (js.includes('__OBY_CANVAS')) return the.hung ? new Promise(() => {}) : Promise.resolve(the.canvas);
        if (js === 'window.obyStatus') return Promise.resolve({ drawn: true });
        return Promise.resolve();
      },
      close() {}, isDestroyed: () => false,
    };
    constructor() { the.views.push(this); }
    setBackgroundColor() {}
    setVisible(v: boolean) { this.visible.push(v); }
    setBounds() {}
  }
  return {
    app: { isPackaged: false },
    ipcMain: { handle: (name: string, f: Handler) => { the.handlers.set(name, f); } },
    WebContentsView,
    session: { fromPartition: () => ({ webRequest: { onBeforeRequest() {} }, setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, on() {} }) },
  };
});
// obid: an id ending in d is a design, any other a page; versions 1 and 2 of one file each
const ok = (body: object) => Object.setPrototypeOf({ ok: true, status: 200, ...body }, Response.prototype) as Response;
vi.mock('../../../main/obid', () => ({
  send: async (_method: string, path: string) => {
    the.reads.push(path);
    const id = path.split('/')[3];
    if (path === `/v1/artifacts/${id}`) {
      const design = id.endsWith('d');
      return ok({ json: async () => ({ kind: design ? 'design' : 'html', title: design ? 'Boards' : 'Page', versions: [1, 2].map((version) => ({ version, files: [design ? 'canvas.json' : 'index.html'] })) }) });
    }
    // an id ending in e: a page whose files can't be read
    if (id.endsWith('e')) return Object.setPrototypeOf({ ok: false, status: 500 }, Response.prototype) as Response;
    return ok({ arrayBuffer: async () => new TextEncoder().encode('<h1>Hello</h1>').buffer });
  },
}));
vi.mock('../../../main/sandpackRelay', async (original) => ({
  ...await original<typeof import('../../../main/sandpackRelay')>(),
  startRelay: async (designs: Designs) => { the.designs = designs; return { port: 4123, origin: 'http://preview.localhost:4123', designOrigin: (id: string, v: number) => `http://${id}-v${v}-0123456789abcdef.localhost:4123`, close: async () => {} }; },
}));

const DESIGN = '00000000-0000-4000-8000-00000000000d';
const PAGE = '00000000-0000-4000-8000-00000000000a';
const win = { isDestroyed: () => false, contentView: { addChildView() {}, removeChildView() {} }, webContents: { on() {} } };
const bounds = { x: 0, y: 0, width: 600, height: 400 };
async function main() {
  vi.resetModules();
  const { previewing } = await import('../../../main/artifactPreview');
  const stop = previewing(() => true, () => win as never);
  const preview = (id: string, version = 1) => the.handlers.get('artifact-preview')!({}, { id, version, bounds }) as Promise<{ shown?: true; pageError?: string; error?: string }>;
  const thumb = (id: string, version = 1) => the.handlers.get('artifact-thumb')!({}, { id, version }) as Promise<{ src?: string; none?: true }>;
  const reload = () => the.handlers.get('artifact-reload')!({});
  return { stop, preview, thumb, reload };
}
const shownLast = (v: { visible: boolean[] }) => v.visible.at(-1) === true;

beforeEach(() => {
  the.handlers.clear();
  Object.assign(the, { views: [], designs: null, reads: [], canvas: { ready: true, errors: [] }, hung: false, held: null });
});
afterEach(() => { vi.useRealTimers(); });

describe('ARTIFACTSAPP-29: a design drawn in the pane', () => {
  it('artifactsapp_29_a_design_opens_obids_canvas_host_on_its_versions_own_origin', async () => {
    const m = await main();
    expect(await m.preview(DESIGN, 2)).toEqual({ shown: true });
    // the preview's view (preview.html) and the design's view
    expect(the.views).toHaveLength(2);
    const [page, design] = the.views;
    expect(design.loads).toEqual([`http://${DESIGN}-v2-0123456789abcdef.localhost:4123/__canvas.html`]);
    expect(design.files).toBe(0);
    expect(shownLast(design)).toBe(true);
    expect(page.visible).not.toContain(true);
    // main reads only what the artifact is: its boards are the relay's to read, for this version alone
    expect(the.reads).toEqual([`/v1/artifacts/${DESIGN}`]);
    expect(the.designs!.shown(DESIGN, 2)).toBe(true);
    expect(the.designs!.shown(DESIGN, 1)).toBe(false);
    expect(the.designs!.shown(PAGE, 2)).toBe(false);
    // Reload loads the canvas host again, in the design's view
    await m.reload();
    expect(design.reloads).toBe(1);
    m.stop();
  });

  it('artifactsapp_29_the_relay_reads_a_designs_file_from_obids_file_route', async () => {
    const m = await main();
    await m.preview(DESIGN, 2);
    await the.designs!.read(DESIGN, 2, '/__dc/logic/Main.dc.html.js');
    expect(the.reads.at(-1)).toBe(`/v1/artifacts/${DESIGN}/2/__dc/logic/Main.dc.html.js`);
    m.stop();
  });

  it('artifactsapp_29_a_boards_mistake_is_the_panes_page_error', async () => {
    the.canvas = { ready: true, errors: [{ board: 'Viewer.dc.html', message: 'boom' }, { board: 'Main.dc.html', message: 'x is not defined' }] };
    const m = await main();
    expect(await m.preview(DESIGN)).toEqual({ shown: true, pageError: 'Viewer.dc.html: boom\nMain.dc.html: x is not defined' });
    m.stop();
  });

  it('artifactsapp_29_a_design_still_drawing_shows_at_4_s', async () => {
    vi.useFakeTimers();
    the.canvas = { ready: false, errors: [] };
    const m = await main();
    let answered: unknown;
    void m.preview(DESIGN).then((a) => { answered = a; });
    await vi.advanceTimersByTimeAsync(3_000);
    expect(answered).toBeUndefined();
    await vi.advanceTimersByTimeAsync(1_500);
    expect(answered).toEqual({ shown: true });
    expect(shownLast(the.views[1])).toBe(true);
    m.stop();
  });

  it('artifactsapp_29_a_page_after_a_design_shows_in_the_preview_and_the_design_goes', async () => {
    const m = await main();
    await m.preview(DESIGN);
    expect(await m.preview(PAGE)).toEqual({ shown: true });
    const [page, design] = the.views;
    expect(shownLast(page)).toBe(true);
    expect(shownLast(design)).toBe(false);
    // with a page on view, the relay serves no design
    expect(the.designs!.shown(DESIGN, 1)).toBe(false);
    await m.preview(DESIGN, 2);
    expect(shownLast(design)).toBe(true);
    expect(shownLast(page)).toBe(false);
    expect(the.designs!.shown(DESIGN, 1)).toBe(false);
    m.stop();
  });

  it('artifactsapp_29_a_page_that_stops_answering_still_shows_at_4_s', async () => {
    vi.useFakeTimers();
    the.hung = true;
    const m = await main();
    let answered: unknown;
    void m.preview(DESIGN).then((a) => { answered = a; });
    await vi.advanceTimersByTimeAsync(4_500);
    expect(answered).toEqual({ shown: true });
    m.stop();
  });

  it('artifactsapp_29_another_design_opens_hidden_and_the_relay_serves_it_alone', async () => {
    const m = await main();
    await m.preview(DESIGN, 1);
    const design = the.views[1];
    let go!: () => void;
    the.held = new Promise((done) => { go = done; });
    const next = m.preview(DESIGN, 2);
    await vi.waitFor(() => expect(design.loads).toHaveLength(2));
    // while v2 opens: its view hidden (never a half-drawn board), and only v2 served
    expect(shownLast(design)).toBe(false);
    expect(the.designs!.shown(DESIGN, 1)).toBe(false);
    expect(the.designs!.shown(DESIGN, 2)).toBe(true);
    go();
    expect(await next).toEqual({ shown: true });
    expect(shownLast(design)).toBe(true);
    m.stop();
  });

  it('artifactsapp_29_a_page_that_cant_be_read_after_a_design_shows_neither', async () => {
    const m = await main();
    await m.preview(DESIGN, 1);
    const answer = await m.preview('00000000-0000-4000-8000-00000000000e', 1);
    expect(answer).toEqual({ error: 'index.html couldn’t be read.' });
    const [page, design] = the.views;
    expect(shownLast(design)).toBe(false);
    expect(page.visible).not.toContain(true);
    expect(the.designs!.shown(DESIGN, 1)).toBe(false);
    m.stop();
  });

  it('artifactsapp_29_closed_the_relay_serves_no_design', async () => {
    const m = await main();
    await m.preview(DESIGN, 1);
    expect(the.designs!.shown(DESIGN, 1)).toBe(true);
    await the.handlers.get('artifact-preview')!({}, null);
    expect(the.designs!.shown(DESIGN, 1)).toBe(false);
    expect(shownLast(the.views[1])).toBe(false);
    m.stop();
  });

  it('artifactsapp_29_a_design_opening_as_the_window_closes_shows_nothing', async () => {
    const m = await main();
    let go!: () => void;
    the.held = new Promise((done) => { go = done; });
    const opening = m.preview(DESIGN, 1);
    await vi.waitFor(() => expect(the.views[1]?.loads).toHaveLength(1));
    m.stop();
    go();
    expect(await opening).toEqual({ shown: true });
    expect(the.views[1].visible).not.toContain(true);
    expect(the.designs!.shown(DESIGN, 1)).toBe(false);
  });

  it('artifactsapp_29_a_designs_card_keeps_the_boards_blocks', async () => {
    const m = await main();
    expect(await m.thumb(DESIGN)).toEqual({ none: true });
    // no picture's view was made, and no file of it read
    expect(the.views).toHaveLength(0);
    expect(the.reads).toEqual([`/v1/artifacts/${DESIGN}`]);
    m.stop();
  });

  it('artifactsapp_29_the_canvas_hosts_word_is_drawn_or_each_boards_mistake', () => {
    expect(canvasStatus({ ready: false, errors: [] })).toBeNull();
    expect(canvasStatus(null)).toBeNull();
    expect(canvasStatus({ ready: true, errors: [] })).toEqual({ drawn: true });
    expect(canvasStatus({ ready: false, errors: [] }, true)).toEqual({ timeout: true });
    expect(canvasStatus('drawn', true)).toEqual({ timeout: true });
    expect(canvasStatus({ ready: true, errors: [{ board: 'A.dc.html', message: 'boom' }, { board: 1, message: 'not a board' }] })).toEqual({ error: 'A.dc.html: boom', console: [] });
    // still drawing, a board's mistake waits for the rest; at the ceiling it is said
    expect(canvasStatus({ ready: false, errors: [{ board: 'A.dc.html', message: 'boom' }] })).toBeNull();
    expect(canvasStatus({ ready: false, errors: [{ board: 'A.dc.html', message: 'boom' }] }, true)).toEqual({ error: 'A.dc.html: boom', console: [] });
    // each line at most 4,000 characters, at most ten after the first
    const many = Array.from({ length: 20 }, (_, i) => ({ board: `B${i}.dc.html`, message: 'x'.repeat(5_000) }));
    const s = canvasStatus({ ready: true, errors: many }) as { error: string; console: string[] };
    expect(s.error).toHaveLength(4_000);
    expect(s.console).toHaveLength(10);
  });
});

// one request to a real relay, with the Host header a browser would send
function get(port: number, host: string, path: string, method = 'GET'): Promise<{ status: number; headers: Record<string, string | string[] | undefined>; body: string }> {
  return new Promise((done, failed) => {
    const r = request({ host: '127.0.0.1', port, path, method, headers: { Host: host } }, (res) => {
      let body = '';
      res.setEncoding('utf8');
      res.on('data', (c) => { body += c; });
      res.on('end', () => done({ status: res.statusCode ?? 0, headers: res.headers, body }));
    });
    r.on('error', failed);
    r.end();
  });
}

describe('ARTIFACTSAPP-30: a design’s own origin, loopback only', () => {
  const BOARD_POLICY = "default-src 'self' data: blob:; connect-src 'none'; frame-ancestors 'self'; sandbox allow-scripts";
  it('artifactsapp_30_the_relay_serves_the_version_on_view_on_its_own_origin_as_obid_sent_it', async () => {
    const { startRelay: real } = await vi.importActual<typeof import('../../../main/sandpackRelay')>('../../../main/sandpackRelay');
    const asked: string[] = [];
    let policy: string | null = BOARD_POLICY;
    const relay = await real({
      shown: (id, version) => id === DESIGN && version === 2,
      read: async (id, version, path) => {
        asked.push(`${id}/${version}${path}`);
        if (path === '/missing.dc.html') return new Response('Not found.', { status: 404 });
        const headers: Record<string, string> = { 'Content-Type': 'text/javascript; charset=utf-8', 'Access-Control-Allow-Origin': '*', 'Set-Cookie': 'a=b', 'X-Content-Type-Options': 'nosniff' };
        if (policy) headers['Content-Security-Policy'] = policy;
        return new Response(`/* ${path} */`, { status: 200, headers });
      },
    });
    try {
      const origin = relay.designOrigin(DESIGN, 2);
      expect(origin).toMatch(new RegExp(`^http://${DESIGN}-v2-[0-9a-f]{16}\\.localhost:${relay.port}$`));
      const host = new URL(origin).host;
      const secret = host.split('.')[0].slice(-16);
      const r = await get(relay.port, host, '/__dc/logic/Main.dc.html.js?x=1');
      expect(r.status).toBe(200);
      expect(r.body).toBe('/* /__dc/logic/Main.dc.html.js */');
      // obid's type, policy and CORS as obid sent them; nothing else of its
      expect(r.headers['content-security-policy']).toBe(BOARD_POLICY);
      expect(r.headers['access-control-allow-origin']).toBe('*');
      expect(r.headers['content-type']).toBe('text/javascript; charset=utf-8');
      expect(r.headers['set-cookie']).toBeUndefined();
      expect(r.headers['cache-control']).toBe('no-store');
      expect(asked).toEqual([`${DESIGN}/2/__dc/logic/Main.dc.html.js`]);
      // obid's 404 stays a 404
      expect((await get(relay.port, host, '/missing.dc.html')).status).toBe(404);
      asked.length = 0;
      // another version, another artifact, another port, no secret or another, or a host that only looks like one: nothing asked of obid
      const other = secret === '0'.repeat(16) ? '1'.repeat(16) : '0'.repeat(16);
      for (const h of [`${DESIGN}-v1-${secret}.localhost:${relay.port}`, `${PAGE}-v2-${secret}.localhost:${relay.port}`, `${DESIGN}-v2-${secret}.localhost:${relay.port + 1}`, `${DESIGN}-v2.localhost:${relay.port}`, `${DESIGN}-v2-${other}.localhost:${relay.port}`, `${DESIGN.toUpperCase()}-v2-${secret}.localhost:${relay.port}`, `${DESIGN}-v2-${secret}.localhost.evil.com:${relay.port}`, `x${DESIGN}-v2-${secret}.localhost:${relay.port}`, `${DESIGN}-v0-${secret}.localhost:${relay.port}`]) {
        expect((await get(relay.port, h, '/__canvas.html')).status, h).toBe(404);
      }
      // a path that climbs, encoded or not, and a method but GET and HEAD
      for (const p of ['/../x', '/a/%2e%2e/x', '/%2e%2e', '/a//b', '/a%2fb', '/a%5cb', '/a%00b', '/%zz']) expect((await get(relay.port, host, p)).status, p).toBe(404);
      expect((await get(relay.port, host, '/__canvas.html', 'POST')).status).toBe(404);
      expect(asked).toEqual([]);
      // an answer of obid's with no policy is never sent on
      policy = null;
      const bare = await get(relay.port, host, '/__canvas.html');
      expect(bare.status).toBe(502);
      expect(bare.headers['content-security-policy']).toBe(RELAY_CSP);
      // the preview's own hosts are served as before
      expect((await get(relay.port, `k3j9-preview.localhost:${relay.port}`, '/__csb_relay/')).status).toBe(200);
    } finally {
      await relay.close();
    }
  });

  it('artifactsapp_30_a_designs_host_and_path_are_read_strictly', () => {
    const S = '0123456789abcdef';
    expect(designHost(`${DESIGN}-v12-${S}.localhost:4123`, 4123, S)).toEqual({ id: DESIGN, version: 12 });
    for (const h of [undefined, '', `${DESIGN}-v12.localhost:4123`, `${DESIGN}-v12-fedcba9876543210.localhost:4123`, `${DESIGN}-v1-${S}.localhost:4124`, `${DESIGN}-v01-${S}.localhost:4123`, `${DESIGN}-v1-${S}.localhost`, `${DESIGN}-preview.localhost:4123`, `a.${DESIGN}-v1-${S}.localhost:4123`]) expect(designHost(h, 4123, S), String(h)).toBeNull();
    expect(designPath('/__canvas.html')).toBe('/__canvas.html');
    expect(designPath('/project/Board%20One.dc.html')).toBe('/project/Board%20One.dc.html');
    for (const p of ['', '/', 'x', '/a/', '/./a', '/a/..', '/%2E%2E/a']) expect(designPath(p), p).toBeNull();
  });

  it('artifactsapp_30_the_preview_session_lets_a_designs_origin_go_on_the_relays_port_only', () => {
    const page = 'file:///Applications/Oby.app/Contents/Resources/app.asar/out/renderer/';
    const S = '0123456789abcdef';
    for (const u of [`http://${DESIGN}-v1-${S}.localhost:4123/__canvas.html`, `http://${DESIGN}-v2-${S}.localhost:4123/_blob/abc`]) expect(allowed(u, 4123, page), u).toBe(true);
    for (const u of [`http://${DESIGN}-v1-${S}.localhost:4124/__canvas.html`, `https://${DESIGN}-v1-${S}.localhost:4123/`, `http://${DESIGN}-v0-${S}.localhost:4123/`, `http://x.${DESIGN}-v1-${S}.localhost:4123/`, `http://${DESIGN}-v1-${S}.localhost.evil.com:4123/`, `http://${DESIGN.slice(1)}-v1-${S}.localhost:4123/`, `ws://${DESIGN}-v1-${S}.localhost:4123/`]) expect(allowed(u, 4123, page), u).toBe(false);
  });
});
