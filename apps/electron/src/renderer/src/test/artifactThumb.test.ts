import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// docs/contracts/artifacts-app.md, ARTIFACTSAPP-18: a card's picture, taken in main
// (main/artifactPreview.ts), run for real with Electron's views, obid and the relay stood in for.
// Each stand-in view says what it was told and what it drew: its picture's pixels are the title of
// the artifact last shown in it.
type Handler = (e: unknown, ...args: unknown[]) => unknown;
type Image = { getSize(): { width: number; height: number }; resize(o: { width: number }): Image; toPNG(): Buffer };
const the = vi.hoisted(() => ({
  handlers: new Map<string, Handler>(),
  views: [] as {
    bounds: unknown; emulation: unknown; zoom: number; scripts: string[]; shown: string; closed: boolean; captured: number;
    webContents: { executeJavaScript(js: string): Promise<unknown> };
  }[],
  relays: 0,
  locks: 0,
  reads: [] as string[],
  // an artifact whose read from obid waits, and one whose capture does, until let go
  heldReads: new Map<string, Promise<void>>(),
  heldCaptures: new Map<string, Promise<void>>(),
  drawn: true,
  size: { width: 1280, height: 370 },
  resized: [] as { width: number }[],
}));
vi.mock('electron', () => {
  const image = (pixels: string, size: { width: number; height: number }): Image => ({
    getSize: () => size,
    resize: (o) => { the.resized.push(o); return image(pixels, { width: o.width, height: Math.round(size.height * o.width / size.width) }); },
    toPNG: () => Buffer.from(pixels),
  });
  class WebContentsView {
    bounds: unknown = null;
    emulation: unknown = null;
    zoom = 1;
    scripts: string[] = [];
    shown = '';
    closed = false;
    captured = 0;
    webContents = {
      setWindowOpenHandler() {}, on() {},
      loadURL: async () => {}, loadFile: async () => {},
      executeJavaScript: (js: string): Promise<unknown> => {
        this.scripts.push(js);
        const shows = /^window\.obyShow\(JSON\.parse\((.*)\)\)$/.exec(js);
        if (shows) this.shown = (JSON.parse(JSON.parse(shows[1])) as { title: string }).title;
        if (js === 'window.obyDrawn' && !the.drawn) return new Promise(() => {});
        return Promise.resolve();
      },
      capturePage: async () => {
        this.captured++;
        const pixels = this.shown;
        await the.heldCaptures.get(pixels);
        return image(pixels, the.size);
      },
      enableDeviceEmulation: (p: unknown) => { this.emulation = p; },
      setZoomFactor: (f: number) => { this.zoom = f; },
      close: () => { this.closed = true; },
      isDestroyed: () => this.closed,
    };
    constructor() { the.views.push(this); }
    setBackgroundColor() {}
    setVisible() {}
    setBounds(b: unknown) { this.bounds = b; }
  }
  return {
    app: { isPackaged: false },
    ipcMain: { handle: (name: string, f: Handler) => { the.handlers.set(name, f); } },
    WebContentsView,
    session: { fromPartition: () => ({ webRequest: { onBeforeRequest: () => { the.locks++; } }, setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, on() {} }) },
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
// an answer, the settles after "drawn" (250 ms each) let pass
const answer = async <T>(p: Promise<T>) => { const s = settled(p); await vi.advanceTimersByTimeAsync(1_000); return s.value as T; };

beforeEach(() => {
  vi.useFakeTimers();
  Object.assign(the, { views: [], relays: 0, locks: 0, reads: [], drawn: true, size: { width: 1280, height: 370 }, resized: [] });
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
