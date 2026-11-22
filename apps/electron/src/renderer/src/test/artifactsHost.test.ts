// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';

// The preview's host page's status (docs/contracts/artifacts-app.md, ARTIFACTSAPP-18 and 23): what main
// waits on before it shows the pane's view or takes a card's picture. Sandpack's static client is a
// stand-in that gives the frame its address, as the real one does once its relay is up, and a window
// of its own (happy-dom loads no frame).

// Each stand-in client, what it was given and whether it was destroyed; each message it was given goes in `dispatched`.
const the = vi.hoisted(() => ({ dispatched: [] as unknown[], clients: [] as { options: { externalResources?: string[] }; destroyed: boolean }[] }));
vi.mock('oby-sandpack-static', () => ({
  SandpackStatic: class {
    options: unknown;
    destroyed = false;
    constructor(frame: HTMLIFrameElement, _setup: unknown, options: unknown) {
      Object.defineProperty(frame, 'contentWindow', { value: {} });
      frame.setAttribute('src', 'about:blank');
      this.options = options;
      the.clients.push(this as never);
    }
    destroy(): void { this.destroyed = true; }
    dispatch(message: unknown): void { the.dispatched.push(message); }
  },
}));
await import('../artifacts/host');

type Files = Record<string, { text: string }>;
type Status = { drawn: true } | { error: string; console: string[] } | { timeout: true };
const shown = (files: Files = { 'index.html': { text: '<h1>Hi</h1>' } }) => { window.obyShow!({ kind: 'html', title: 'Card', files, bundler: 'http://x-preview.localhost' }); return document.querySelector<HTMLIFrameElement>('iframe[data-artifact-preview]')!; };
// (the newest frame is the first: laid under the last until it settles)
// what the status is once the page's microtasks ran: undefined while it waits
const status = () => { const s: { value?: Status } = {}; void window.obyStatus!.then((v) => { s.value = v; }); return s; };
const loaded = (frame: HTMLIFrameElement) => frame.dispatchEvent(new Event('load'));
const posted = (source: unknown, data: unknown) => window.dispatchEvent(new MessageEvent('message', { data, source: source as Window }));
const tick = () => vi.advanceTimersByTimeAsync(0);

afterEach(() => { vi.useRealTimers(); });

it('artifactsapp_23_obystatus_settles_on_its_own_frames_first_word_or_4_s', async () => {
  vi.useFakeTimers();
  // drawn, from this show's frame only: another source's word, or Sandpack's own, is no word
  const a = shown();
  expect(the.clients.at(-1)!.options.externalResources).toEqual(['/__csb_oby/bridge.js']);
  let s = status();
  posted(window, { oby: 'drawn' });
  posted(a.contentWindow, { codesandbox: true, type: 'done' });
  posted(a.contentWindow, 'oby:rendered');
  posted(a.contentWindow, { oby: 'error', message: 7 });
  await tick();
  expect(s.value).toBeUndefined();
  posted(a.contentWindow, { oby: 'drawn' });
  await tick();
  expect(s.value).toEqual({ drawn: true });

  // the last show's frame stays on view under the new one until the new one settles, then goes
  const aClient = the.clients.at(-1)!;
  const b = shown();
  expect([a.isConnected, b.isConnected, aClient.destroyed]).toEqual([true, true, false]);
  expect(document.querySelectorAll('iframe')[0]).toBe(b);
  s = status();
  posted(b.contentWindow, { oby: 'drawn' });
  await tick();
  expect(s.value).toEqual({ drawn: true });
  expect([a.isConnected, b.isConnected, aClient.destroyed]).toEqual([false, true, true]);

  // an error before the frame's load waits for it, the errors after it kept; one after it settles at once
  const c = shown();
  s = status();
  posted(c.contentWindow, { oby: 'error', message: 'boom' });
  posted(c.contentWindow, { oby: 'console-error', message: 'and then' });
  await tick();
  expect(s.value).toBeUndefined();
  loaded(c);
  await tick();
  expect(s.value).toEqual({ error: 'boom', console: ['and then'] });
  const d = shown();
  s = status();
  loaded(d);
  posted(d.contentWindow, { oby: 'error', message: 'late' });
  await tick();
  expect(s.value).toEqual({ error: 'late', console: [] });
  // an error that came before drawn wins over it
  const e = shown();
  s = status();
  posted(e.contentWindow, { oby: 'error', message: 'first' });
  posted(e.contentWindow, { oby: 'drawn' });
  await tick();
  expect(s.value).toEqual({ error: 'first', console: [] });

  // the first error, cut to 4,000 characters, and the last ten posts after it
  const f = shown();
  s = status();
  posted(f.contentWindow, { oby: 'error', message: 'x'.repeat(10_000) });
  for (let n = 1; n <= 11; n++) posted(f.contentWindow, { oby: 'console-error', message: `c${n}` });
  posted(f.contentWindow, { oby: 'error', message: 'e12' });
  loaded(f);
  await tick();
  expect(s.value).toEqual({ error: 'x'.repeat(4_000), console: ['c3', 'c4', 'c5', 'c6', 'c7', 'c8', 'c9', 'c10', 'c11', 'e12'] });

  // nothing at all: a timeout at 4 s; an error with no load: that error at 4 s
  shown();
  s = status();
  await vi.advanceTimersByTimeAsync(3_999);
  expect(s.value).toBeUndefined();
  await vi.advanceTimersByTimeAsync(1);
  expect(s.value).toEqual({ timeout: true });
  const g = shown();
  s = status();
  posted(g.contentWindow, { oby: 'error', message: 'stuck' });
  await vi.advanceTimersByTimeAsync(4_000);
  expect(s.value).toEqual({ error: 'stuck', console: [] });

  // a show that replaces one still waiting: the waiting one's word counts for nothing, and the new
  // one's settle takes every frame before it away
  const h = shown();
  const i = shown();
  s = status();
  posted(h.contentWindow, { oby: 'drawn' });
  await tick();
  expect(s.value).toBeUndefined();
  expect([g.isConnected, h.isConnected]).toEqual([true, true]);
  posted(i.contentWindow, { oby: 'drawn' });
  await tick();
  expect(s.value).toEqual({ drawn: true });
  expect([...document.querySelectorAll('iframe')]).toEqual([i]);
  expect(the.clients.slice(0, -1).every((x) => x.destroyed)).toBe(true);
});

it('artifactsapp_18_a_react_artifacts_picture_waits_for_its_first_render', async () => {
  const react = shown({ 'App.tsx': { text: 'export default function App() { return <h1>Hi</h1>; }' } });
  expect(react.contentWindow).not.toBeNull();
  const s = status();
  loaded(react);
  posted(window, { oby: 'drawn' });
  await new Promise((done) => setTimeout(done, 20));
  expect(s.value).toBeUndefined();
  posted(react.contentWindow, { oby: 'drawn' });
  await new Promise((done) => setTimeout(done, 20));
  expect(s.value).toEqual({ drawn: true });
});

it('artifactsapp_20_reload_runs_the_version_on_view_again', () => {
  shown({ 'index.html': { text: '<h1>Hi</h1>' } });
  the.dispatched.length = 0;
  window.obyReload!();
  expect(the.dispatched).toEqual([{ type: 'refresh' }]);
});
