import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// docs/contracts/artifacts-app.md, ARTIFACTSAPP-18, 24 and 25: a card's picture and the pane's
// view, in main (main/artifactPreview.ts), run for real with Electron's views, obid and the relay
// stood in for. Each stand-in view says what it was told and what it drew: its picture's pixels are
// the title of the artifact last shown in it, its bitmap one colour or two as `captures` says.
type Handler = (e: unknown, ...args: unknown[]) => unknown;
type Image = { getSize(): { width: number; height: number }; resize(o: { width: number }): Image; toPNG(): Buffer; isEmpty(): boolean; toBitmap(): Buffer };
type Request = (d: { url: string; webContentsId?: number }, done: (r: { cancel: boolean }) => void) => void;
const the = vi.hoisted(() => ({
  handlers: new Map<string, Handler>(),
  views: [] as {
    bounds: unknown; emulation: unknown; zoom: number; scripts: string[]; shown: string; closed: boolean; captured: number;
    moves: { width: number }[]; visible: boolean[]; preferences: Record<string, unknown>;
    webContents: { id: number; executeJavaScript(js: string): Promise<unknown> };
  }[],
  relays: 0,
  locks: 0,
  reads: [] as string[],
  // an artifact whose read from obid waits, and one whose capture does, until let go
  heldReads: new Map<string, Promise<void>>(),
  heldCaptures: new Map<string, Promise<void>>(),
  drawn: true,
  // the host page's status (host.ts), once it settles; and what each next capture comes out as
  status: { drawn: true } as unknown,
  captures: [] as ('empty' | 'one' | 'two')[],
  size: { width: 1280, height: 370 },
  resized: [] as { width: number }[],
  // the preview session's onBeforeRequest, as main set it
  request: null as Request | null,
}));
vi.mock('electron', () => {
  const image = (pixels: string, size: { width: number; height: number }, kind = 'two'): Image => ({
    getSize: () => size,
    resize: (o) => { the.resized.push(o); return image(pixels, { width: o.width, height: Math.round(size.height * o.width / size.width) }, kind); },
    toPNG: () => Buffer.from(pixels),
    isEmpty: () => kind === 'empty',
    // a few pixels, all one colour or half and half
    toBitmap: () => Buffer.from(Array.from({ length: 64 }, (_, i) => (kind === 'two' && i >= 32 ? 255 : 0))),
  });
  class WebContentsView {
    bounds: unknown = null;
    emulation: unknown = null;
    zoom = 1;
    scripts: string[] = [];
    shown = '';
    closed = false;
    captured = 0;
    moves: { width: number }[] = [];
    visible: boolean[] = [];
    preferences: Record<string, unknown>;
    webContents = {
      id: the.views.length + 1,
      setWindowOpenHandler() {}, on() {},
      loadURL: async () => {}, loadFile: async () => {},
      executeJavaScript: (js: string): Promise<unknown> => {
        this.scripts.push(js);
        const shows = /^window\.obyShow\(JSON\.parse\((.*)\)\)$/.exec(js);
        if (shows) this.shown = (JSON.parse(JSON.parse(shows[1])) as { title: string }).title;
        if (js === 'window.obyStatus') return the.drawn ? Promise.resolve(the.status) : new Promise(() => {});
        return Promise.resolve();
      },
      capturePage: async () => {
        this.captured++;
        const pixels = this.shown;
        await the.heldCaptures.get(pixels);
        return image(pixels, the.size, the.captures.shift() ?? 'two');
      },
      enableDeviceEmulation: (p: unknown) => { this.emulation = p; },
      setZoomFactor: (f: number) => { this.zoom = f; },
      close: () => { this.closed = true; },
      isDestroyed: () => this.closed,
    };
    constructor(o: { webPreferences: Record<string, unknown> }) { this.preferences = o.webPreferences; the.views.push(this); }
    setBackgroundColor() {}
    setVisible(v: boolean) { this.visible.push(v); }
    setBounds(b: { width: number }) { this.bounds = b; this.moves.push({ width: b.width }); }
  }
  return {
    app: { isPackaged: false },
    ipcMain: { handle: (name: string, f: Handler) => { the.handlers.set(name, f); } },
    WebContentsView,
    session: { fromPartition: () => ({ webRequest: { onBeforeRequest: (f: Request) => { the.locks++; the.request = f; } }, setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, on() {} }) },
  };
});
// obid: each artifact titled by its id's last letter, with versions 1 and 2 of one file
const ok = (body: object) => Object.setPrototypeOf({ ok: true, status: 200, ...body }, Response.prototype) as Response;
vi.mock('../../../main/obid', () => ({
  send: async (_method: string, path: string) => {
    the.reads.push(path);
    const id = path.split('/')[3];
    if (path === `/v1/artifacts/${id}`) {
      await the.heldReads.get(id);
      return ok({ json: async () => ({ kind: 'html', title: id.slice(-1), versions: [{ version: 1, files: ['index.html'] }, { version: 2, files: ['index.html'] }] }) });
    }
    return ok({ arrayBuffer: async () => new TextEncoder().encode('<h1>Hello</h1>').buffer });
  },
}));
vi.mock('../../../main/sandpackRelay', () => ({
  startRelay: async () => { the.relays++; return { port: 1, origin: 'http://preview.localhost:1', close: async () => {} }; },
}));

const id = (letter: string) => `00000000-0000-4000-8000-00000000000${letter}`;
const win = { isDestroyed: () => false, contentView: { addChildView() {}, removeChildView() {} }, webContents: { on() {} } };
// main's previewing() fresh, its stop function, and an ask on its channel
async function main() {
  vi.resetModules();
  const { previewing } = await import('../../../main/artifactPreview');
  const stop = previewing(() => true, () => win as never);
  const ask = (letter: string, version = 1) => the.handlers.get('artifact-thumb')!({}, { id: id(letter), version }) as Promise<{ src?: string; none?: true }>;
  return { stop, ask };
}
// a picture's pixels: the title of what it shows
const pixels = (t: { src?: string; none?: true }) => (t.src ? Buffer.from(t.src.replace('data:image/png;base64,', ''), 'base64').toString() : 'none');
const hold = (on: Map<string, Promise<void>>, key: string) => { let go!: () => void; on.set(key, new Promise((done) => { go = done; })); return () => { on.delete(key); go(); }; };
const settled = <T>(p: Promise<T>) => { const s = { done: false, value: undefined as T | undefined }; void p.then((v) => { s.done = true; s.value = v; }); return s; };
// an answer, its work's timers let pass
const answer = async <T>(p: Promise<T>) => { const s = settled(p); await vi.advanceTimersByTimeAsync(1_000); return s.value as T; };

beforeEach(() => {
  vi.useFakeTimers();
  Object.assign(the, { views: [], relays: 0, locks: 0, reads: [], drawn: true, status: { drawn: true }, captures: [], size: { width: 1280, height: 370 }, resized: [], request: null });
  the.heldReads.clear();
  the.heldCaptures.clear();
});
afterEach(() => { vi.useRealTimers(); });

describe('ARTIFACTSAPP-18: a card’s picture', () => {
  it('artifactsapp_18_a_slow_picture_never_answers_another_card', async () => {
    let m = await main();
    // A's capture hangs past the limit: A gets none, its view is closed, and B runs on a fresh one
    const letA = hold(the.heldCaptures, 'a');
    const a = settled(m.ask('a'));
    const b = settled(m.ask('b'));
    await vi.advanceTimersByTimeAsync(9_000);
    expect(the.views).toHaveLength(1);
    expect(the.views[0].captured).toBe(1);
    expect(a.done).toBe(false);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(a).toEqual({ done: true, value: { none: true } });
    expect(the.views[0].closed).toBe(true);
    await vi.advanceTimersByTimeAsync(300);
    expect(the.views).toHaveLength(2);
    expect(the.views[1].shown).toBe('b');
    expect(pixels(b.value!)).toBe('b');
    // A's late pixels go into no answer
    letA();
    await vi.advanceTimersByTimeAsync(0);
    expect(pixels(b.value!)).toBe('b');
    expect(pixels(await m.ask('a'))).toBe('none');
    expect(pixels(await m.ask('b'))).toBe('b');
    // C, stalled in obid's read past its limit, then resumes: it touches nothing of D's view
    const letC = hold(the.heldReads, id('c'));
    const c = settled(m.ask('c'));
    const d = settled(m.ask('d'));
    await vi.advanceTimersByTimeAsync(10_000);
    expect(c.value).toEqual({ none: true });
    await vi.advanceTimersByTimeAsync(300);
    expect(pixels(d.value!)).toBe('d');
    const dView = the.views.at(-1)!;
    const before = dView.scripts.length;
    letC();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(dView.scripts.length).toBe(before);
    expect(the.views.every((v) => v.shown !== 'c')).toBe(true);
    // one relay, one lock, over all of it
    expect([the.relays, the.locks]).toEqual([1, 1]);

    // three pictures that come at once leave no limit behind to close their view
    the.views = [];
    the.relays = 0;
    the.locks = 0;
    m = await main();
    expect((await answer(Promise.all([m.ask('a'), m.ask('b'), m.ask('c')]))).map(pixels)).toEqual(['a', 'b', 'c']);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(the.views.map((v) => v.closed)).toEqual([false]);
  });

  it('artifactsapp_18_a_picture_waits_for_the_preview_not_a_fixed_time', async () => {
    // drawn at once: captured at once, not after 4 s
    let m = await main();
    const a = settled(m.ask('a'));
    await vi.advanceTimersByTimeAsync(999);
    expect(the.views[0].captured).toBe(1);
    expect(pixels(a.value!)).toBe('a');
    // never drawn: captured at 4 s, not before
    the.drawn = false;
    the.views = [];
    m = await main();
    const b = settled(m.ask('b'));
    await vi.advanceTimersByTimeAsync(3_999);
    expect(the.views[0].captured).toBe(0);
    await vi.advanceTimersByTimeAsync(1);
    expect(the.views[0].captured).toBe(1);
    await vi.advanceTimersByTimeAsync(0);
    expect(pixels(b.value!)).toBe('b');
    // an ask still queued when its card asked for a newer version is dropped, obid never asked for it
    the.drawn = true;
    m = await main();
    const letC = hold(the.heldCaptures, 'c');
    const c = settled(m.ask('c'));
    const e1 = settled(m.ask('e', 1));
    const e2 = settled(m.ask('e', 2));
    await vi.advanceTimersByTimeAsync(300);
    letC();
    await vi.advanceTimersByTimeAsync(600);
    expect(pixels(c.value!)).toBe('c');
    expect(e1.value).toEqual({ none: true });
    expect(pixels(e2.value!)).toBe('e');
    expect(the.reads.filter((p) => p.startsWith(`/v1/artifacts/${id('e')}/1/`))).toEqual([]);
    expect(the.reads).toContain(`/v1/artifacts/${id('e')}/2/index.html`);
  });

  it('artifactsapp_18_the_picture_is_drawn_at_half_width', async () => {
    // a Retina display's capture of the page drawn at 640x185 CSS px and scale 2
    the.size = { width: 2560, height: 740 };
    const m = await main();
    expect(pixels(await answer(m.ask('a')))).toBe('a');
    const [view] = the.views;
    expect(view.bounds).toEqual({ x: 0, y: 0, width: 640, height: 185 });
    expect(view.emulation).toMatchObject({ viewSize: { width: 640, height: 185 }, deviceScaleFactor: 2, scale: 1 });
    expect(view.zoom).toBe(1);
    expect(the.resized).toEqual([{ width: 1280 }]);
    // one already 1280x370 goes as it is
    the.size = { width: 1280, height: 370 };
    expect(pixels(await answer(m.ask('b')))).toBe('b');
    expect(the.resized).toEqual([{ width: 1280 }]);
  });

  it('artifactsapp_18_a_picture_asked_as_the_window_closes_starts_nothing', async () => {
    const m = await main();
    const letA = hold(the.heldReads, id('a'));
    const a = settled(m.ask('a'));
    await vi.advanceTimersByTimeAsync(0);
    m.stop();
    letA();
    await vi.advanceTimersByTimeAsync(0);
    expect(a.value).toEqual({ none: true });
    expect(the.views).toEqual([]);
    expect(the.relays).toBe(0);
    // the window back: a picture again
    expect(pixels(await answer(m.ask('b')))).toBe('b');
    expect(the.relays).toBe(1);
  });
});

describe('ARTIFACTSAPP-24: the page’s error, from main', () => {
  const preview = (letter: string, version = 1) => the.handlers.get('artifact-preview')!({}, { id: id(letter), version, bounds: { x: 0, y: 0, width: 400, height: 300 } }) as Promise<{ shown?: true; pageError?: string; error?: string }>;
  const refuse = (url: string, webContentsId?: number) => the.request!({ url, webContentsId }, () => {});

  it('artifactsapp_24_main_answers_the_pages_error_and_refused_hosts', async () => {
    const m = await main();
    // the picture's view first, so its own refusal can come during the pane's show
    expect(pixels(await answer(m.ask('b')))).toBe('b');
    const thumb = the.views[0];
    expect(thumb.preferences.backgroundThrottling).toBe(false);
    refuse('https://before.example.com/a.js', 99);
    // the page's status held back: the pane's view shows only once it settles
    let settle!: (s: unknown) => void;
    the.status = new Promise((done) => { settle = done; });
    const a = settled(preview('a'));
    await vi.advanceTimersByTimeAsync(100);
    const pane = the.views[1];
    expect('backgroundThrottling' in pane.preferences).toBe(false);
    expect(pane.visible).not.toContain(true);
    refuse('https://cdn.example.com/x.js?k=1', pane.webContents.id);
    refuse('https://cdn.example.com/y.css', pane.webContents.id);
    refuse('https://fonts.example.org/f.woff2#x', pane.webContents.id);
    refuse('not a url', pane.webContents.id);
    refuse('https://thumb.example.com/t.png', thumb.webContents.id);
    settle({ error: 'boom', console: ['and then'] });
    await vi.advanceTimersByTimeAsync(0);
    expect(pane.visible.at(-1)).toBe(true);
    expect(a.value).toEqual({ shown: true, pageError: 'boom\nand then\nThe page asked for cdn.example.com: previews have no network.\nThe page asked for fonts.example.org: previews have no network.' });
    expect(a.value!.pageError).not.toMatch(/x\.js|k=1|thumb|before/);

    // a status of any other shape is a timeout: no pageError
    for (const [letter, status] of [['c', { error: 'boom', console: [], more: 1 }], ['d', { error: 'x'.repeat(4_001), console: [] }], ['e', { error: 'boom', console: Array.from({ length: 11 }, () => 'c') }], ['f', { error: '', console: [] }], ['9', 'boom']] as const) {
      the.status = status;
      expect(await answer(preview(letter)), letter).toEqual({ shown: true });
    }
    // drawn, and nothing refused: none either
    the.status = { drawn: true };
    expect(await answer(preview('a', 2))).toEqual({ shown: true });
    // one that never settles: shown at main's own 4 s
    the.drawn = false;
    const h = settled(preview('8'));
    await vi.advanceTimersByTimeAsync(3_900);
    expect(h.done).toBe(false);
    await vi.advanceTimersByTimeAsync(200);
    expect(h.value).toEqual({ shown: true });
    expect(pane.visible.at(-1)).toBe(true);
  });
});

describe('ARTIFACTSAPP-25: a blank picture taken again', () => {
  it('artifactsapp_25_a_blank_capture_is_nudged_and_taken_again_at_most_3_times', async () => {
    // drawn: the first capture at once, no fixed settle
    let m = await main();
    the.captures = ['empty', 'one', 'two'];
    const a = settled(m.ask('a'));
    await vi.advanceTimersByTimeAsync(10);
    const [view] = the.views;
    expect(view.captured).toBe(1);
    expect(view.preferences.backgroundThrottling).toBe(false);
    // an empty capture: nudged a pixel narrower, back 150 ms later, and taken again
    expect(view.moves.slice(1)).toEqual([{ width: 639 }]);
    await vi.advanceTimersByTimeAsync(130);
    expect(view.captured).toBe(1);
    await vi.advanceTimersByTimeAsync(20);
    expect(view.captured).toBe(2);
    // one colour: again; two colours: that one answered
    expect(view.moves.slice(1)).toEqual([{ width: 639 }, { width: 640 }, { width: 639 }]);
    await vi.advanceTimersByTimeAsync(150);
    expect(view.captured).toBe(3);
    expect(view.moves.slice(1)).toEqual([{ width: 639 }, { width: 640 }, { width: 639 }, { width: 640 }]);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(view.captured).toBe(3);
    expect(pixels(a.value!)).toBe('a');

    // blank every time: four captures in all, the last answered
    the.captures = ['one', 'one', 'one', 'one'];
    expect(pixels(await answer(m.ask('b')))).toBe('b');
    expect(view.captured).toBe(3 + 4);
    // two colours at once: one capture
    expect(pixels(await answer(m.ask('c')))).toBe('c');
    expect(view.captured).toBe(3 + 4 + 1);

    // the pane's view is made without it
    the.views = [];
    m = await main();
    await answer(the.handlers.get('artifact-preview')!({}, { id: id('d'), version: 1, bounds: { x: 0, y: 0, width: 10, height: 10 } }) as Promise<unknown>);
    expect('backgroundThrottling' in the.views[0].preferences).toBe(false);
  });
});

describe('ARTIFACTSAPP-20: the pane’s Reload', () => {
  it('artifactsapp_20_reload_runs_the_version_on_view_again', async () => {
    const m = await main();
    const reload = () => the.handlers.get('artifact-reload')!({}) as Promise<null>;
    const preview = (ask: object | null) => the.handlers.get('artifact-preview')!({}, ask);
    const scripts = () => the.views.flatMap((v) => v.scripts).filter((js) => js === 'window.obyReload?.()');
    // none on view: nothing runs anywhere
    expect(await reload()).toBeNull();
    expect(the.views).toEqual([]);
    expect(await preview({ id: id('a'), version: 1, bounds: { x: 0, y: 0, width: 10, height: 10 } })).toEqual({ shown: true });
    const [pane] = the.views;
    expect(pixels(await answer(m.ask('b')))).toBe('b');
    const thumb = the.views[1];
    expect(await reload()).toBeNull();
    expect(pane.scripts.at(-1)).toBe('window.obyReload?.()');
    expect(thumb.scripts).not.toContain('window.obyReload?.()');
    expect(scripts()).toHaveLength(1);
    // the pane's preview taken away: nothing runs again
    await preview(null);
    expect(await reload()).toBeNull();
    expect(scripts()).toHaveLength(1);
  });
});
